//! `:dedent:` — how much leading whitespace comes off every line of a block.
//!
//! A source transform rather than a presentation option, which is why it is
//! applied here and never stored: the AST holds exactly what the reader will
//! see. Shared with `.. literalinclude::`, whose `:dedent:` means the same
//! thing over text read from a file.

use crate::context::ParseCtx;
use crate::directives::option_value::OptionValueType;
use crate::directives::options::OptionLine;
use crate::directives::spec::OptionValue;
use crate::indent::strip_common_indent;

/// What a `:dedent:` asked for.
///
/// A separate type rather than an `Option<usize>` because "dedent fully" and
/// "dedent by zero columns" are different instructions that an `Option` would
/// have to spell the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Dedent {
    /// `:dedent:` with no value — strip whatever indent the lines share.
    Full,
    /// `:dedent: N` — remove exactly `N` columns from every line.
    Columns(usize),
}

/// A `:dedent:`: with no value, all shared indentation; with a number,
/// exactly that many columns.
impl OptionValueType for Dedent {
    const VALUE: OptionValue = OptionValue::Integer;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        let trimmed = line.value.trim();
        if trimmed.is_empty() {
            return Ok(Dedent::Full);
        }
        trimmed
            .parse::<usize>()
            .map(Dedent::Columns)
            .map_err(|_| format!("needs a non-negative integer, found '{}'", line.value))
    }
}

/// Applies `:dedent:` to the already-unindented body.
///
/// With no option the body is returned with its shared indent stripped, which
/// is what every literal block does. `Columns(n)` removes exactly `n` columns,
/// leaving a shorter line untouched rather than erroring — docutils' own
/// behaviour, and the only choice that keeps a blank line blank.
pub(super) fn apply_dedent(body: &[&str], dedent: Option<Dedent>) -> String {
    match dedent {
        None | Some(Dedent::Full) => strip_common_indent(body),
        Some(Dedent::Columns(columns)) => {
            let stripped: Vec<String> = body
                .iter()
                .map(|line| {
                    let leading = line
                        .chars()
                        .take_while(|c| c.is_whitespace() && *c != '\n')
                        .count();
                    let cut = leading.min(columns);
                    line.chars().skip(cut).collect()
                })
                .collect();
            trim_blank_edges(&stripped)
        }
    }
}

/// Drops leading and trailing blank lines and joins the rest, matching what
/// [`strip_common_indent`] does so both `:dedent:` paths agree.
fn trim_blank_edges(lines: &[String]) -> String {
    let start = lines.iter().position(|line| !line.trim().is_empty());
    let Some(start) = start else {
        return String::new();
    };
    let end = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .unwrap_or(start);
    lines[start..=end].join("\n")
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{codes, parse};
    use super::*;
    use rinx_ast::DiagnosticCode;

    #[test]
    fn test_parse_code_block_dedents_by_an_explicit_column_count() {
        // Given — a body whose author wants two columns removed
        let body = ["   :dedent: 2", "", "     x = 1", "       y = 2"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then — relative indentation survives, two columns lighter
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(block.content, "x = 1\n  y = 2");
    }

    #[test]
    fn test_parse_code_block_dedent_without_a_value_strips_all_shared_indent() {
        // Given
        let body = ["   :dedent:", "", "     x = 1", "       y = 2"];

        // When
        let (block, _) = parse("python", &body);

        // Then
        assert_eq!(block.content, "x = 1\n  y = 2");
    }

    #[test]
    fn test_apply_dedent_leaves_a_shorter_line_alone() {
        // Given — a line with less indent than the dedent asks to remove.
        // Tested against `apply_dedent` directly rather than through a
        // document: a directive body line indented less than its own option
        // lines would have ended the directive, so this shape only reaches
        // here from an explicit `:dedent:` wider than some line's indent.
        let body = ["    x = 1", "  y = 2"];

        // When
        let content = apply_dedent(&body, Some(Dedent::Columns(4)));

        // Then — it loses its indent, and no content is cut off with it
        assert_eq!(content, "x = 1\ny = 2");
    }

    #[test]
    fn test_apply_dedent_keeps_a_blank_line_blank() {
        // Given — a blank line has no indent to remove
        let body = ["    x = 1", "", "    y = 2"];

        // When
        let content = apply_dedent(&body, Some(Dedent::Columns(4)));

        // Then
        assert_eq!(content, "x = 1\n\ny = 2");
    }

    #[test]
    fn test_parse_code_block_reports_a_non_numeric_dedent() {
        // Given
        let body = ["   :dedent: lots", "", "   x = 1"];

        // When
        let (_, diagnostics) = parse("python", &body);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockInvalidInteger]
        );
    }

    #[test]
    fn test_apply_dedent_returns_an_empty_string_for_a_blank_body() {
        // Given — a directive with nothing but blank lines under it
        let body = ["   ", "   "];

        // When
        let content = apply_dedent(&body, Some(Dedent::Columns(2)));

        // Then
        assert_eq!(content, "");
    }
}
