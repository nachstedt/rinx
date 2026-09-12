//! A scanner over a template's raw text, answering the two questions the
//! renderer must settle *before* handing anything to `MiniJinja`.
//!
//! **Where may a line marker go?** [`Scan::line_starts_inside_tag`] says, for
//! every line, whether its first character lies inside a `{% %}`, `{{ }}` or
//! `{# #}` that began on an earlier line. A marker injected there would land
//! in the middle of an expression and turn a working template into a syntax
//! error, so those lines get none — they emit no text of their own anyway.
//!
//! **Which files does this template read?** [`Scan::includes`] lists every
//! `{% include %}`, `{% extends %}`, `{% import %}` and `{% from %}` target.
//! They are collected here, from the text, rather than resolved by a
//! `MiniJinja` loader callback, for two reasons: the loader seam this crate
//! borrows is a *borrowed* trait object and `Environment::set_loader` demands
//! `'static`, and a build that declares its inputs up front wants the list
//! anyway. It is also what makes a non-literal include name refusable by name.
//!
//! The scanner is not a Jinja parser and does not try to be. It tracks
//! delimiters, quoted strings and `{% raw %}`, which is everything the two
//! questions need.

use crate::error::{TemplateError, TemplateErrorKind};

/// One file a template names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IncludeRef {
    /// The name exactly as written between the quotes.
    pub(crate) name: String,
    /// The 1-based line the tag was written on.
    pub(crate) line: usize,
}

/// What [`scan`] learned about one template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Scan {
    /// For each 0-based line, whether it begins inside an unclosed tag.
    pub(crate) line_starts_inside_tag: Vec<bool>,
    /// Every template this one names, in the order written.
    pub(crate) includes: Vec<IncludeRef>,
}

/// Which delimiter the scanner is currently inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Text,
    Block,
    Variable,
    Comment,
}

impl State {
    /// Whether text at this point belongs to a tag rather than to the output.
    const fn inside_tag(self) -> bool {
        !matches!(self, Self::Text)
    }
}

/// The tag keywords that name another template.
const TEMPLATE_KEYWORDS: [&str; 4] = ["include", "extends", "import", "from"];

/// Scans `text`, or reports the first construct this renderer refuses.
pub(crate) fn scan(text: &str) -> Result<Scan, TemplateError> {
    let chars: Vec<char> = text.chars().collect();
    let mut cursor = Cursor {
        chars: &chars,
        index: 0,
        line: 1,
        state: State::Text,
        raw: false,
        tag: String::new(),
        tag_line: 1,
        scan: Scan {
            line_starts_inside_tag: vec![false],
            includes: Vec::new(),
        },
    };
    cursor.run()?;
    let mut scan = cursor.scan;
    scan.line_starts_inside_tag.truncate(text.lines().count());
    Ok(scan)
}

/// The scanner's working state, split out so each step is a named method
/// rather than another arm of one long loop.
struct Cursor<'a> {
    chars: &'a [char],
    index: usize,
    line: usize,
    state: State,
    raw: bool,
    tag: String,
    tag_line: usize,
    scan: Scan,
}

impl Cursor<'_> {
    /// Walks the whole template.
    fn run(&mut self) -> Result<(), TemplateError> {
        while let Some(&current) = self.chars.get(self.index) {
            if current == '\n' {
                self.index += 1;
                self.line += 1;
                self.scan
                    .line_starts_inside_tag
                    .push(self.state.inside_tag());
                continue;
            }
            match self.state {
                State::Text => self.step_text(current)?,
                State::Block | State::Variable | State::Comment => self.step_tag(current)?,
            }
        }
        Ok(())
    }

    /// One character of ordinary output text: the only thing that matters is a
    /// delimiter opening.
    fn step_text(&mut self, current: char) -> Result<(), TemplateError> {
        let Some(opened) = self.opening_at(current) else {
            self.index += 1;
            return Ok(());
        };
        // Inside `{% raw %}` everything but the closing tag is literal, so a
        // `{{` there opens nothing and must not be treated as a tag.
        if self.raw && opened != State::Block {
            self.index += 1;
            return Ok(());
        }
        if !self.raw && matches!(self.chars.get(self.index + 2), Some('-' | '+')) {
            return Err(self.refuse(TemplateErrorKind::WhitespaceControl));
        }
        self.state = opened;
        self.tag.clear();
        self.tag_line = self.line;
        self.index += 2;
        Ok(())
    }

    /// The state a delimiter starting at the cursor opens, if any.
    fn opening_at(&self, current: char) -> Option<State> {
        if current != '{' {
            return None;
        }
        match self.chars.get(self.index + 1) {
            Some('%') => Some(State::Block),
            Some('{') => Some(State::Variable),
            Some('#') => Some(State::Comment),
            _ => None,
        }
    }

    /// One character inside a tag: a quoted string, the closing delimiter, or
    /// content to remember.
    fn step_tag(&mut self, current: char) -> Result<(), TemplateError> {
        if matches!(current, '\'' | '"') && self.state != State::Comment {
            self.copy_string(current);
            return Ok(());
        }
        if self.closes_here() {
            if matches!(self.chars.get(self.index.wrapping_sub(1)), Some('-')) && !self.raw {
                return Err(self.refuse(TemplateErrorKind::WhitespaceControl));
            }
            self.close_tag()?;
            return Ok(());
        }
        self.tag.push(current);
        self.index += 1;
        Ok(())
    }

    /// Whether the current tag's closing delimiter starts at the cursor.
    fn closes_here(&self) -> bool {
        let expected = match self.state {
            State::Block => '%',
            State::Variable => '}',
            State::Comment => '#',
            State::Text => return false,
        };
        self.chars.get(self.index) == Some(&expected)
            && self.chars.get(self.index + 1) == Some(&'}')
    }

    /// Consumes a quoted string whole, so a delimiter written inside one does
    /// not close the tag around it.
    fn copy_string(&mut self, quote: char) {
        self.tag.push(quote);
        self.index += 1;
        while let Some(&current) = self.chars.get(self.index) {
            self.index += 1;
            if current == '\n' {
                self.line += 1;
                self.scan.line_starts_inside_tag.push(true);
                continue;
            }
            self.tag.push(current);
            if current == '\\' {
                if let Some(&escaped) = self.chars.get(self.index) {
                    self.tag.push(escaped);
                    self.index += 1;
                }
                continue;
            }
            if current == quote {
                return;
            }
        }
    }

    /// Leaves the current tag, acting on what it said.
    fn close_tag(&mut self) -> Result<(), TemplateError> {
        self.index += 2;
        let state = std::mem::replace(&mut self.state, State::Text);
        if state != State::Block {
            return Ok(());
        }
        let tag = std::mem::take(&mut self.tag);
        let mut words = tag.split_whitespace();
        let keyword = words.next().unwrap_or_default();
        if keyword == "raw" {
            self.raw = true;
            return Ok(());
        }
        if keyword == "endraw" {
            self.raw = false;
            return Ok(());
        }
        if self.raw || !TEMPLATE_KEYWORDS.contains(&keyword) {
            return Ok(());
        }
        let rest = tag.trim().strip_prefix(keyword).unwrap_or_default().trim();
        let name = string_literal(rest).ok_or_else(|| {
            TemplateError::in_document(
                TemplateErrorKind::DynamicName(rest.to_string()),
                Some(self.tag_line),
            )
        })?;
        self.scan.includes.push(IncludeRef {
            name,
            line: self.tag_line,
        });
        Ok(())
    }

    /// Builds the error for a construct refused on the tag's own line.
    fn refuse(&self, kind: TemplateErrorKind) -> TemplateError {
        TemplateError::in_document(kind, Some(self.line))
    }
}

/// The contents of `text`'s leading quoted string, if it starts with one.
fn string_literal(text: &str) -> Option<String> {
    let mut characters = text.chars();
    let quote = characters.next().filter(|c| matches!(c, '\'' | '"'))?;
    let mut value = String::new();
    for current in characters {
        if current == quote {
            return Some(value);
        }
        value.push(current);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text_starts_no_line_inside_a_tag() {
        // Given
        let text = "one\ntwo\nthree";

        // When
        let scan = scan(text).expect("scans");

        // Then
        assert_eq!(scan.line_starts_inside_tag, vec![false, false, false]);
        assert!(scan.includes.is_empty());
    }

    #[test]
    fn test_a_tag_spanning_lines_marks_the_lines_it_covers() {
        // Given
        let text = "{% if a\n   or b %}\nyes\n{% endif %}";

        // When
        let scan = scan(text).expect("scans");

        // Then
        assert_eq!(
            scan.line_starts_inside_tag,
            vec![false, true, false, false],
            "only the continuation line lies inside the tag"
        );
    }

    #[test]
    fn test_a_trailing_newline_adds_no_line() {
        // Given
        let text = "one\n";

        // When
        let scan = scan(text).expect("scans");

        // Then
        assert_eq!(scan.line_starts_inside_tag, vec![false]);
    }

    #[test]
    fn test_every_template_keyword_is_collected_with_its_line() {
        // Given
        let text = "{% include \"a.rst\" %}\n{% extends \"b.rst\" %}\n\
                    {% import \"c.rst\" as c %}\n{% from \"d.rst\" import e %}";

        // When
        let scan = scan(text).expect("scans");

        // Then
        assert_eq!(
            scan.includes,
            vec![
                IncludeRef {
                    name: "a.rst".to_string(),
                    line: 1
                },
                IncludeRef {
                    name: "b.rst".to_string(),
                    line: 2
                },
                IncludeRef {
                    name: "c.rst".to_string(),
                    line: 3
                },
                IncludeRef {
                    name: "d.rst".to_string(),
                    line: 4
                },
            ]
        );
    }

    #[test]
    fn test_a_delimiter_inside_a_string_does_not_close_the_tag() {
        // Given
        let text = "{% set x = \"%} {{\" %}\nafter";

        // When
        let scan = scan(text).expect("scans");

        // Then
        assert_eq!(scan.line_starts_inside_tag, vec![false, false]);
    }

    #[test]
    fn test_a_comment_hides_an_include_from_collection() {
        // Given
        let text = "{# {% include \"a.rst\" %} #}";

        // When
        let scan = scan(text).expect("scans");

        // Then
        assert!(scan.includes.is_empty());
    }

    #[test]
    fn test_a_raw_block_hides_an_include_and_keeps_its_lines_markable() {
        // Given
        let text = "{% raw %}\n{% include \"a.rst\" %}\n{{ x }}\n{% endraw %}\nafter";

        // When
        let scan = scan(text).expect("scans");

        // Then
        assert!(scan.includes.is_empty());
        assert_eq!(
            scan.line_starts_inside_tag,
            vec![false, false, false, false, false]
        );
    }

    #[test]
    fn test_an_opening_whitespace_modifier_is_refused() {
        // Given
        let text = "a\nb\n{%- set x = 1 %}";

        // When
        let error = scan(text).expect_err("refused");

        // Then
        assert_eq!(error.kind, TemplateErrorKind::WhitespaceControl);
        assert_eq!(error.line, Some(3));
    }

    #[test]
    fn test_a_closing_whitespace_modifier_is_refused() {
        // Given
        let text = "{{ value -}}";

        // When
        let error = scan(text).expect_err("refused");

        // Then
        assert_eq!(error.kind, TemplateErrorKind::WhitespaceControl);
    }

    #[test]
    fn test_a_whitespace_modifier_inside_a_raw_block_is_left_alone() {
        // Given
        let text = "{% raw %}\n{%- set x = 1 %}\n{% endraw %}";

        // When
        let scan = scan(text).expect("scans");

        // Then
        assert!(scan.includes.is_empty());
    }

    #[test]
    fn test_an_include_of_an_expression_is_refused_with_the_expression() {
        // Given
        let text = "{% include page ~ \".rst\" %}";

        // When
        let error = scan(text).expect_err("refused");

        // Then
        assert_eq!(
            error.kind,
            TemplateErrorKind::DynamicName("page ~ \".rst\"".to_string())
        );
        assert_eq!(error.line, Some(1));
    }

    #[test]
    fn test_string_literal_reads_either_quote_and_refuses_the_rest() {
        // Given / When / Then
        assert_eq!(string_literal("\"a.rst\" x"), Some("a.rst".to_string()));
        assert_eq!(string_literal("'a.rst'"), Some("a.rst".to_string()));
        assert_eq!(string_literal("page"), None);
        assert_eq!(string_literal("\"unterminated"), None);
        assert_eq!(string_literal(""), None);
    }

    #[test]
    fn test_inside_tag_is_true_for_every_delimiter_state() {
        // Given / When / Then
        assert!(!State::Text.inside_tag());
        assert!(State::Block.inside_tag());
        assert!(State::Variable.inside_tag());
        assert!(State::Comment.inside_tag());
    }
}
