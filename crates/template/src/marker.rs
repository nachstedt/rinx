//! How a rendered line is traced back to the line somebody wrote.
//!
//! `MiniJinja` has no source map, and a Jinja pass moves lines around freely:
//! an `{% include %}` splices a whole file in, so every position below it
//! shifts by however long that file is. Without an answer, every diagnostic
//! raised after the first include in a document would name a line that has
//! nothing on it.
//!
//! The answer is to make the text carry it. Before rendering, a marker naming
//! `(template, line)` is injected at the start of every line that does not
//! begin inside a tag; rendering moves those markers around exactly as it
//! moves the text they precede; afterwards they are read off and stripped.
//! What survives is one [`SourceLine`] per rendered line.
//!
//! Two details decide how well it works:
//!
//! - **The last marker on a line wins.** An `{% include %}` written on its own
//!   line renders as that line's marker immediately followed by the included
//!   file's first line and *its* marker. The text visible on that rendered
//!   line is the included file's, so that is what it is attributed to.
//! - **A line with no marker inherits the one above it**, unchanged rather
//!   than incremented. Such a line was produced by a construct spanning
//!   several lines, and the construct is the thing worth pointing at.

/// Where one rendered line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLine {
    /// The index into [`RenderedSource::templates`] of the file this line was
    /// written in, or `None` for the document itself.
    ///
    /// [`RenderedSource::templates`]: crate::RenderedSource::templates
    pub template: Option<usize>,
    /// The 1-based line within that file.
    pub line: u32,
}

/// Opens a marker. A C0 control character, because it cannot occur in the
/// reStructuredText being rendered and needs no escaping in a Jinja template.
const MARKER_START: char = '\u{1}';

/// Closes a marker.
const MARKER_END: char = '\u{2}';

/// Returns `text` with a `(source, line)` marker on every line that may carry
/// one, per `line_starts_inside_tag`.
///
/// `source` is 0 for the document and `index + 1` for a template, so the two
/// share one numbering and a marker never has to say which kind it names.
pub(crate) fn inject(text: &str, source: usize, line_starts_inside_tag: &[bool]) -> String {
    let mut marked = String::with_capacity(text.len());
    for (index, line) in text.lines().enumerate() {
        if index > 0 {
            marked.push('\n');
        }
        if !line_starts_inside_tag.get(index).copied().unwrap_or(false) {
            let number = index + 1;
            marked.push(MARKER_START);
            marked.push_str(&source.to_string());
            marked.push(':');
            marked.push_str(&number.to_string());
            marked.push(MARKER_END);
        }
        marked.push_str(line);
    }
    if text.ends_with('\n') {
        marked.push('\n');
    }
    marked
}

/// Strips every marker from `rendered`, returning the clean text and where
/// each of its lines came from.
pub(crate) fn strip(rendered: &str) -> (String, Vec<SourceLine>) {
    let mut text = String::with_capacity(rendered.len());
    let mut lines = Vec::new();
    let mut previous = SourceLine {
        template: None,
        line: 1,
    };
    for (index, line) in rendered.lines().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        let (clean, found) = read_line(line);
        previous = found.unwrap_or(previous);
        lines.push(previous);
        text.push_str(&clean);
    }
    if rendered.ends_with('\n') {
        text.push('\n');
    }
    (text, lines)
}

/// One line without its markers, and the last marker it carried.
fn read_line(line: &str) -> (String, Option<SourceLine>) {
    if !line.contains(MARKER_START) {
        return (line.to_string(), None);
    }
    let mut clean = String::with_capacity(line.len());
    let mut found = None;
    let mut rest = line;
    while let Some(start) = rest.find(MARKER_START) {
        clean.push_str(&rest[..start]);
        let after = &rest[start + MARKER_START.len_utf8()..];
        let Some(end) = after.find(MARKER_END) else {
            // An unterminated marker is text the document wrote itself.
            clean.push_str(&rest[start..]);
            return (clean, found);
        };
        if let Some(parsed) = parse_marker(&after[..end]) {
            found = Some(parsed);
        }
        rest = &after[end + MARKER_END.len_utf8()..];
    }
    clean.push_str(rest);
    (clean, found)
}

/// Reads a marker's `source:line` payload.
fn parse_marker(payload: &str) -> Option<SourceLine> {
    let (source, line) = payload.split_once(':')?;
    let source: usize = source.parse().ok()?;
    Some(SourceLine {
        template: source.checked_sub(1),
        line: line.parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_marks_every_line_it_may() {
        // Given
        let text = "one\ntwo\n";

        // When
        let marked = inject(text, 0, &[false, false]);

        // Then
        assert_eq!(marked, "\u{1}0:1\u{2}one\n\u{1}0:2\u{2}two\n");
    }

    #[test]
    fn test_inject_skips_a_line_that_begins_inside_a_tag() {
        // Given
        let text = "{% if a\n   or b %}\nyes";

        // When
        let marked = inject(text, 2, &[false, true, false]);

        // Then
        assert_eq!(marked, "\u{1}2:1\u{2}{% if a\n   or b %}\n\u{1}2:3\u{2}yes");
    }

    #[test]
    fn test_inject_keeps_a_missing_trailing_newline_missing() {
        // Given
        let text = "one";

        // When
        let marked = inject(text, 0, &[false]);

        // Then
        assert_eq!(marked, "\u{1}0:1\u{2}one");
    }

    #[test]
    fn test_strip_removes_the_markers_and_reports_their_origin() {
        // Given
        let rendered = "\u{1}0:1\u{2}one\n\u{1}1:4\u{2}two\n";

        // When
        let (text, lines) = strip(rendered);

        // Then
        assert_eq!(text, "one\ntwo\n");
        assert_eq!(
            lines,
            vec![
                SourceLine {
                    template: None,
                    line: 1
                },
                SourceLine {
                    template: Some(0),
                    line: 4
                },
            ]
        );
    }

    #[test]
    fn test_strip_lets_the_last_marker_on_a_line_win() {
        // Given: what an `{% include %}` on its own line renders as.
        let rendered = "\u{1}0:2\u{2}\u{1}1:1\u{2}header";

        // When
        let (text, lines) = strip(rendered);

        // Then
        assert_eq!(text, "header");
        assert_eq!(
            lines,
            vec![SourceLine {
                template: Some(0),
                line: 1
            }]
        );
    }

    #[test]
    fn test_strip_lets_an_unmarked_line_inherit_the_one_above() {
        // Given
        let rendered = "\u{1}0:7\u{2}one\ntwo";

        // When
        let (_, lines) = strip(rendered);

        // Then
        assert_eq!(lines[1], lines[0], "the construct above produced both");
    }

    #[test]
    fn test_strip_leaves_text_that_only_looks_like_a_marker() {
        // Given: a start byte with nothing closing it.
        let rendered = "a\u{1}0:1 b";

        // When
        let (text, lines) = strip(rendered);

        // Then
        assert_eq!(text, "a\u{1}0:1 b");
        assert_eq!(
            lines,
            vec![SourceLine {
                template: None,
                line: 1
            }]
        );
    }

    #[test]
    fn test_parse_marker_reads_a_payload_and_refuses_nonsense() {
        // Given / When / Then
        assert_eq!(
            parse_marker("0:12"),
            Some(SourceLine {
                template: None,
                line: 12
            })
        );
        assert_eq!(
            parse_marker("3:1"),
            Some(SourceLine {
                template: Some(2),
                line: 1
            })
        );
        assert_eq!(parse_marker("nope"), None);
        assert_eq!(parse_marker("0:x"), None);
    }

    #[test]
    fn test_read_line_returns_the_line_untouched_when_it_holds_no_marker() {
        // Given
        let line = "plain text";

        // When
        let (clean, found) = read_line(line);

        // Then
        assert_eq!(clean, "plain text");
        assert_eq!(found, None);
    }
}
