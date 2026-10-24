//! Which directive name, option name or option value the cursor is
//! completing, read from the lines around it.
//!
//! As for a role, the parse is no help: a directive being typed is not one
//! yet. The lines are read the way the reader splits a block instead
//! (`rinx_parser`'s `directives/reader.rs`): a marker is `.. name::` at the
//! start of a line, and its option block is the lines indented below it,
//! up to the first blank line. A line there opening with a field marker,
//! `:name:`, is an option, and the text after it the option's value.

use std::ops::Range;

/// What the cursor is completing in a directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectiveContext {
    /// A directive's name, after a line's `..`.
    Name(NameContext),
    /// An option's name, after the `:` opening an option line.
    Option(OptionContext),
    /// An option's value, after its `:name:`.
    Value(ValueContext),
}

/// A directive name being typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameContext {
    /// The name typed so far, up to the cursor.
    pub typed: String,
    /// The characters a completion replaces, as 0-based character indices on
    /// the line: the whole name written, up to the `::`.
    pub replace: Range<usize>,
    /// Whether the name still needs the space separating it from the `..`:
    /// the cursor is right after the dots.
    pub needs_space: bool,
    /// Whether the `::` is already written after the name.
    pub closed: bool,
}

/// An option name being typed in a directive's option block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionContext {
    /// The directive whose option block the line is in.
    pub directive: WrittenDirective,
    /// The `+` or `-` written before the name, which `.. entity-update::`
    /// reads as appending to or removing from a list.
    pub mode: Option<char>,
    /// The name typed so far, up to the cursor.
    pub typed: String,
    /// The characters a completion replaces: the whole name written, up to
    /// its closing `:`.
    pub replace: Range<usize>,
    /// Whether the closing `:` is already written.
    pub closed: bool,
    /// The names of the block's other option lines, mode signs stripped.
    pub written: Vec<String>,
}

/// An option value being typed in a directive's option block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueContext {
    /// The directive whose option block the line is in.
    pub directive: WrittenDirective,
    /// The option's name, its mode sign stripped.
    pub option: String,
    /// The value written, from its start to the end of the line, trimmed.
    pub value: String,
    /// Where the value starts on the line, as a character index.
    pub value_start: usize,
    /// The cursor's offset within `value`.
    pub cursor: usize,
    /// The value of the block's `:id:` line, when another line of it is
    /// one: the entity the block declares, which none of its relations
    /// should name.
    pub id: Option<String>,
}

/// A directive marker, as written above the line being completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenDirective {
    /// The name before the `::`, a substitution definition's `|name|` left
    /// out.
    pub name: String,
    /// What follows the `::` on the marker line, trimmed.
    pub argument: String,
}

/// What the cursor at character index `cursor` on line `line` of `lines`
/// completes in a directive, or `None` when it is in none.
#[must_use]
pub fn directive_at(lines: &[&str], line: usize, cursor: usize) -> Option<DirectiveContext> {
    let chars: Vec<char> = lines.get(line)?.chars().collect();
    let cursor = cursor.min(chars.len());
    if let Some(name) = name_at(&chars, cursor) {
        return Some(DirectiveContext::Name(name));
    }
    let field = field_at(&chars, cursor)?;
    let OptionBlock {
        directive,
        written,
        id,
    } = enclosing_directive(lines, line)?;
    let without_mode = |name: &str| name.trim_start_matches(['+', '-']).to_string();
    match field {
        Field::Name {
            mode,
            typed,
            replace,
            closed,
        } => Some(DirectiveContext::Option(OptionContext {
            directive,
            mode,
            typed,
            replace,
            closed,
            written: written.iter().map(|name| without_mode(name)).collect(),
        })),
        Field::Value {
            option,
            value,
            value_start,
            cursor,
        } => Some(DirectiveContext::Value(ValueContext {
            directive,
            option: without_mode(&option),
            value,
            value_start,
            cursor,
            id,
        })),
    }
}

/// Whether `c` may be part of a directive's name: docutils' simple
/// reference name characters, `:` for a domain-qualified one among them.
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || "-_.:+".contains(c)
}

/// Whether `c` may be part of an option's name.
fn is_option_char(c: char) -> bool {
    c.is_alphanumeric() || "-_.".contains(c)
}

/// The directive name the cursor is typing on `chars`, or `None` when the
/// line is no marker being written — a target's `.. _`, a substitution's
/// `.. |`, a citation's `.. [`, or anything after the `::`.
fn name_at(chars: &[char], cursor: usize) -> Option<NameContext> {
    let indent = chars.iter().take_while(|c| c.is_whitespace()).count();
    let rest = &chars[indent..];
    if rest.len() < 2 || rest[0] != '.' || rest[1] != '.' {
        return None;
    }
    let dots_end = indent + 2;
    if cursor == dots_end {
        // Right after the dots: the name, and its space, are still to come.
        let after = &chars[dots_end..];
        return after
            .iter()
            .all(|c| c.is_whitespace())
            .then(|| NameContext {
                typed: String::new(),
                replace: cursor..cursor,
                needs_space: true,
                closed: false,
            });
    }
    if chars.get(dots_end) != Some(&' ') || cursor <= dots_end {
        return None;
    }
    let start = dots_end + 1;
    let typed: String = chars[start..cursor].iter().collect();
    if !chars[start..cursor].iter().all(|&c| is_name_char(c))
        || typed.starts_with('_')
        || typed.contains("::")
    {
        return None;
    }
    let mut end = cursor;
    while end < chars.len() && is_name_char(chars[end]) && !closes_at(chars, end) {
        end += 1;
    }
    Some(NameContext {
        closed: closes_at(chars, end),
        replace: start..end,
        typed,
        needs_space: false,
    })
}

/// Whether the `::` ending a directive's name starts at `at`.
fn closes_at(chars: &[char], at: usize) -> bool {
    chars.get(at) == Some(&':') && chars.get(at + 1) == Some(&':')
}

/// What the cursor is typing on an option line.
enum Field {
    /// The option's name.
    Name {
        mode: Option<char>,
        typed: String,
        replace: Range<usize>,
        closed: bool,
    },
    /// The option's value.
    Value {
        option: String,
        value: String,
        value_start: usize,
        cursor: usize,
    },
}

/// What the cursor is typing on `chars` if it is an indented option line,
/// or `None` when it is no option line or the cursor is in its indentation.
fn field_at(chars: &[char], cursor: usize) -> Option<Field> {
    let indent = chars.iter().take_while(|c| c.is_whitespace()).count();
    if indent == 0 || chars.get(indent) != Some(&':') || cursor <= indent {
        return None;
    }
    let mut name_start = indent + 1;
    let mode = chars
        .get(name_start)
        .copied()
        .filter(|&c| c == '+' || c == '-');
    if mode.is_some() {
        name_start += 1;
    }
    let mut name_end = name_start;
    while name_end < chars.len() && is_option_char(chars[name_end]) {
        name_end += 1;
    }
    let closed = chars.get(name_end) == Some(&':');
    if cursor <= name_end {
        if cursor < name_start {
            return None;
        }
        return Some(Field::Name {
            mode,
            typed: chars[name_start..cursor].iter().collect(),
            replace: name_start..name_end,
            closed,
        });
    }
    if !closed || name_end == name_start {
        return None;
    }
    // A value is separated from its field marker by whitespace.
    let after_marker = name_end + 1;
    if cursor <= after_marker || !chars[after_marker].is_whitespace() {
        return None;
    }
    let value_start = (after_marker..chars.len())
        .find(|&at| !chars[at].is_whitespace())
        .unwrap_or(chars.len())
        .min(cursor);
    let value: String = chars[value_start..].iter().collect();
    Some(Field::Value {
        option: chars[indent + 1..name_end].iter().collect(),
        value: value.trim_end().to_string(),
        value_start,
        cursor: cursor - value_start,
    })
}

/// The option block a line is in, as [`enclosing_directive`] reads it.
struct OptionBlock {
    /// The directive whose block it is.
    directive: WrittenDirective,
    /// The names of the block's other option lines.
    written: Vec<String>,
    /// The value of the block's other `:id:` line, if it has one.
    id: Option<String>,
}

/// The option block holding line `line` of `lines` — or `None` when the line
/// is in none: a blank line, or a line indented no deeper than the marker,
/// comes between it and any marker above.
fn enclosing_directive(lines: &[&str], line: usize) -> Option<OptionBlock> {
    let marker_line = marker_above(lines, line)?;
    let indent = indent_of(lines[line]);
    let below = lines[line + 1..]
        .iter()
        .take_while(|below| !below.trim().is_empty() && indent_of(below) >= indent);
    let others: Vec<&str> = lines[marker_line + 1..line]
        .iter()
        .chain(below)
        .copied()
        .collect();
    let id = others.iter().find_map(|text| {
        let value = text.trim_start().strip_prefix(":id:")?;
        (indent_of(text) == indent).then(|| value.trim().to_string())
    });
    Some(OptionBlock {
        directive: marker(lines[marker_line])?,
        written: others
            .iter()
            .filter_map(|text| option_name(text, indent))
            .collect(),
        id,
    })
}

/// The line of the marker whose option block holds line `line` of `lines`,
/// by the rule of [`enclosing_directive`].
fn marker_above(lines: &[&str], line: usize) -> Option<usize> {
    let indent = indent_of(lines[line]);
    for above in (0..line).rev() {
        let text = lines[above];
        if text.trim().is_empty() {
            return None;
        }
        if indent_of(text) < indent {
            return marker(text).map(|_| above);
        }
    }
    None
}

/// The line of the directive marker whose identity line `line` of `lines`
/// states: the marker itself, or the `:id:` line of its option block — or
/// `None` for any other line.
#[must_use]
pub fn identity_marker_line(lines: &[&str], line: usize) -> Option<usize> {
    let text = lines.get(line)?;
    if marker(text).is_some() {
        return Some(line);
    }
    (option_name(text, indent_of(text)).as_deref() == Some("id"))
        .then(|| marker_above(lines, line))
        .flatten()
}

/// The comma-separated item of `context`'s value the cursor is in, as the
/// characters it covers on the line, and the list's other items.
#[must_use]
pub fn list_item_at(context: &ValueContext) -> (Range<usize>, Vec<&str>) {
    let chars: Vec<char> = context.value.chars().collect();
    let cursor = context.cursor.min(chars.len());
    let mut start = chars[..cursor]
        .iter()
        .rposition(|&c| c == ',')
        .map_or(0, |at| at + 1);
    while start < cursor && chars[start].is_whitespace() {
        start += 1;
    }
    let mut end = chars[cursor..]
        .iter()
        .position(|&c| c == ',')
        .map_or(chars.len(), |offset| cursor + offset);
    while end > start && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    let current: String = chars[start..end].iter().collect();
    let listed = context
        .value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty() && *item != current)
        .collect();
    (
        context.value_start + start..context.value_start + end,
        listed,
    )
}

/// How many characters of indentation `line` has.
fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

/// The name of the option `line` opens at `indent`, if it opens one there.
fn option_name(line: &str, indent: usize) -> Option<String> {
    if indent_of(line) != indent {
        return None;
    }
    let field = line.trim_start().strip_prefix(':')?;
    let (name, _) = field.split_once(':')?;
    (!name.is_empty() && !name.contains(char::is_whitespace)).then(|| name.to_string())
}

/// The directive `line` is the marker of: `.. name:: argument`, or a
/// substitution definition's `.. |name| name:: argument`.
fn marker(line: &str) -> Option<WrittenDirective> {
    let rest = line.trim_start().strip_prefix(".. ")?;
    let rest = match rest.strip_prefix('|') {
        Some(substitution) => substitution.split_once('|')?.1.trim_start(),
        None => rest,
    };
    let (name, argument) = rest.split_once("::")?;
    (!name.is_empty() && name.chars().all(is_name_char)).then(|| WrittenDirective {
        name: name.to_string(),
        argument: argument.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The context at the `|` in the last line of `marked`, which is removed
    /// first.
    fn at_marker(marked: &str) -> Option<DirectiveContext> {
        let lines: Vec<&str> = marked.lines().collect();
        let last = lines.len() - 1;
        let cursor = lines[last]
            .chars()
            .position(|c| c == '|')
            .expect("a marker");
        let unmarked = lines[last].replacen('|', "", 1);
        let mut lines: Vec<&str> = lines[..last].to_vec();
        lines.push(&unmarked);
        directive_at(&lines, last, cursor)
    }

    /// The name context `typed` over `replace`.
    fn name(typed: &str, replace: Range<usize>, closed: bool) -> DirectiveContext {
        DirectiveContext::Name(NameContext {
            typed: typed.to_string(),
            replace,
            needs_space: false,
            closed,
        })
    }

    fn written(name: &str, argument: &str) -> WrittenDirective {
        WrittenDirective {
            name: name.to_string(),
            argument: argument.to_string(),
        }
    }

    #[test]
    fn test_directive_at_completes_a_name_after_the_dots_and_space() {
        // When / Then
        assert_eq!(at_marker(".. |"), Some(name("", 3..3, false)));
        assert_eq!(at_marker("   .. no|"), Some(name("no", 6..8, false)));
    }

    #[test]
    fn test_directive_at_replaces_the_whole_name_up_to_its_colons() {
        // When / Then
        assert_eq!(at_marker(".. no|te:: Title"), Some(name("no", 3..7, true)));
        assert_eq!(at_marker(".. py:fu|"), Some(name("py:fu", 3..8, false)));
    }

    #[test]
    fn test_directive_at_completes_right_after_the_dots_with_a_space_to_come() {
        // When / Then
        assert_eq!(
            at_marker("  ..|"),
            Some(DirectiveContext::Name(NameContext {
                typed: String::new(),
                replace: 4..4,
                needs_space: true,
                closed: false,
            }))
        );
    }

    #[test]
    fn test_directive_at_completes_no_target_substitution_citation_or_argument() {
        // When / Then
        assert_eq!(at_marker(".. _lab|"), None);
        assert_eq!(directive_at(&[".. |sub|"], 0, 8), None);
        assert_eq!(at_marker(".. [1|"), None);
        assert_eq!(at_marker(".. note:: ti|"), None);
        assert_eq!(at_marker("Some text .. no|"), None);
        assert_eq!(at_marker("...|"), None);
    }

    #[test]
    fn test_directive_at_completes_an_option_name_below_the_marker() {
        // When
        let context = at_marker(".. toctree::\n   :maxdepth: 2\n   :ca|");

        // Then
        assert_eq!(
            context,
            Some(DirectiveContext::Option(OptionContext {
                directive: written("toctree", ""),
                mode: None,
                typed: "ca".to_string(),
                replace: 4..6,
                closed: false,
                written: vec!["maxdepth".to_string()],
            }))
        );
    }

    #[test]
    fn test_directive_at_reads_the_options_below_the_line_and_the_closing_colon() {
        // When
        let lines = [".. image:: a.png", "   :wi:", "   :alt: A"];
        let context = directive_at(&lines, 1, 6);

        // Then
        let Some(DirectiveContext::Option(option)) = context else {
            panic!("an option context, not {context:?}");
        };
        assert_eq!(option.directive, written("image", "a.png"));
        assert_eq!(option.replace, 4..6);
        assert!(option.closed);
        assert_eq!(option.written, ["alt"]);
    }

    #[test]
    fn test_directive_at_reads_an_update_s_mode_sign_and_strips_it_from_written_names() {
        // When
        let context = at_marker(".. entity-update:: REQ_1\n   :-links: X\n   :+ta|");

        // Then
        let Some(DirectiveContext::Option(option)) = context else {
            panic!("an option context, not {context:?}");
        };
        assert_eq!(option.mode, Some('+'));
        assert_eq!(option.typed, "ta");
        assert_eq!(option.replace, 5..7);
        assert_eq!(option.written, ["links"]);
    }

    #[test]
    fn test_directive_at_finds_the_marker_of_a_nested_directive() {
        // When
        let context = at_marker(".. note::\n\n   .. code-block:: python\n      :line|");

        // Then
        let Some(DirectiveContext::Option(option)) = context else {
            panic!("an option context, not {context:?}");
        };
        assert_eq!(option.directive, written("code-block", "python"));
    }

    #[test]
    fn test_directive_at_reads_a_substitution_definition_s_inner_directive() {
        // When
        let context = at_marker(".. |logo| image:: logo.png\n   :wid|");

        // Then
        let Some(DirectiveContext::Option(option)) = context else {
            panic!("an option context, not {context:?}");
        };
        assert_eq!(option.directive, written("image", "logo.png"));
    }

    #[test]
    fn test_directive_at_completes_no_option_in_the_content() {
        // When / Then — the blank line ends the option block
        assert_eq!(at_marker(".. note::\n\n   :ref|"), None);
        // and a field list is no directive's
        assert_eq!(at_marker("Text\n   :fie|"), None);
        assert_eq!(at_marker(":fie|"), None);
    }

    #[test]
    fn test_directive_at_completes_an_option_value_after_its_name() {
        // When
        let context = at_marker(".. figure:: a.png\n   :align: ce|nter");

        // Then
        assert_eq!(
            context,
            Some(DirectiveContext::Value(ValueContext {
                directive: written("figure", "a.png"),
                option: "align".to_string(),
                value: "center".to_string(),
                value_start: 11,
                cursor: 2,
                id: None,
            }))
        );
    }

    #[test]
    fn test_directive_at_reads_the_id_another_line_of_the_block_writes() {
        // When
        let context = at_marker(".. req:: Brakes\n   :id: R_1\n   :links: S|");

        // Then
        let Some(DirectiveContext::Value(value)) = context else {
            panic!("a value context, not {context:?}");
        };
        assert_eq!(value.id.as_deref(), Some("R_1"));
    }

    #[test]
    fn test_directive_at_completes_an_empty_value_and_strips_the_mode_sign() {
        // When
        let context = at_marker(".. entity-update:: X\n   :+tags: |");

        // Then
        let Some(DirectiveContext::Value(value)) = context else {
            panic!("a value context, not {context:?}");
        };
        assert_eq!(value.option, "tags");
        assert_eq!(value.value, "");
        assert_eq!(value.value_start, 11);
        assert_eq!(value.cursor, 0);
    }

    #[test]
    fn test_directive_at_completes_nothing_in_the_indentation_or_an_unclosed_name() {
        // When / Then
        assert_eq!(at_marker(".. note::\n | :class: x"), None);
        assert_eq!(at_marker(".. note::\n   :class x|"), None);
    }

    #[test]
    fn test_directive_at_completes_no_value_before_the_space_after_the_marker() {
        // When / Then
        assert_eq!(at_marker(".. figure:: a.png\n   :align:|"), None);
    }

    #[test]
    fn test_marker_reads_the_name_and_argument() {
        // When / Then
        assert_eq!(
            marker("  .. py:function:: f(x)"),
            Some(written("py:function", "f(x)"))
        );
        assert_eq!(marker(".. |a| replace:: b"), Some(written("replace", "b")));
        assert_eq!(marker(".. _label:"), None);
        assert_eq!(marker("Text"), None);
    }

    #[test]
    fn test_option_name_reads_a_field_marker_at_the_indent() {
        // When / Then
        assert_eq!(option_name("   :alt: A", 3), Some("alt".to_string()));
        assert_eq!(option_name("   :+tags: a", 3), Some("+tags".to_string()));
        assert_eq!(option_name("      continued", 3), None);
        assert_eq!(option_name("   :not a field", 3), None);
    }

    #[test]
    fn test_indent_of_counts_leading_whitespace() {
        // When / Then
        assert_eq!(indent_of("   x"), 3);
        assert_eq!(indent_of("x"), 0);
    }

    #[test]
    fn test_closes_at_finds_the_double_colon() {
        // Given
        let chars: Vec<char> = "a::b".chars().collect();

        // When / Then
        assert!(closes_at(&chars, 1));
        assert!(!closes_at(&chars, 2));
    }

    #[test]
    fn test_identity_marker_line_is_the_marker_or_the_id_line_below_it() {
        // Given
        let lines = [
            ".. req:: Brakes",
            "   :status: open",
            "   :id: R_1",
            "",
            "Text.",
        ];

        // When / Then
        assert_eq!(identity_marker_line(&lines, 0), Some(0));
        assert_eq!(identity_marker_line(&lines, 2), Some(0));
        assert_eq!(identity_marker_line(&lines, 1), None);
        assert_eq!(identity_marker_line(&lines, 4), None);
        assert_eq!(identity_marker_line(&lines, 9), None);
    }

    #[test]
    fn test_identity_marker_line_needs_the_id_line_in_the_option_block() {
        // Given — a blank line ends the block
        let lines = [".. req:: Brakes", "", "   :id: R_1"];

        // When / Then
        assert_eq!(identity_marker_line(&lines, 2), None);
    }

    #[test]
    fn test_list_item_at_finds_the_item_under_the_cursor() {
        // Given
        let context = ValueContext {
            directive: WrittenDirective {
                name: "req".to_string(),
                argument: String::new(),
            },
            option: "tags".to_string(),
            value: "ui,  co , docs".to_string(),
            value_start: 0,
            cursor: 6,
            id: None,
        };

        // When
        let (range, listed) = list_item_at(&context);

        // Then
        assert_eq!(range, 5..7);
        assert_eq!(listed, ["ui", "docs"]);
    }

    proptest::proptest! {
        #[test]
        fn test_list_item_at_stays_within_the_value(
            value in "[ a-z,é]{0,16}",
            cursor in 0usize..20,
        ) {
            // Given
            let context = ValueContext {
                directive: WrittenDirective {
                    name: "req".to_string(),
                    argument: String::new(),
                },
                option: "tags".to_string(),
                value: value.clone(),
                value_start: 4,
                cursor,
                id: None,
            };

            // When
            let (range, _) = list_item_at(&context);

            // Then
            proptest::prop_assert!(range.start <= range.end);
            proptest::prop_assert!(range.end <= 4 + value.chars().count());
        }
    }

    proptest::proptest! {
        #[test]
        fn test_directive_at_answers_any_text_at_any_cursor_without_panicking(
            lines in proptest::collection::vec("[ .:|_a-z+,-]{0,12}", 1..5),
            line in 0usize..6,
            cursor in 0usize..16,
        ) {
            // Given
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();

            // When / Then — a cursor past the line or the text is no context
            let context = directive_at(&lines, line, cursor);
            proptest::prop_assert!(line < lines.len() || context.is_none());
        }
    }
}
