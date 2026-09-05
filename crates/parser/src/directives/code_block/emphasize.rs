//! `:emphasize-lines:` — which of a block's own lines are called out.
//!
//! Validated against the block's line count while parsing, so an out-of-range
//! line is reported where it was written rather than silently dropped by the
//! renderer. Shared with `.. literalinclude::`, whose emphasized lines are
//! counted over the text it selected from the file.

use std::num::NonZeroU32;

use rusty_sphinx_ast::{Diagnostic, DiagnosticCode};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

/// Expands an `:emphasize-lines:` value into sorted, deduplicated line
/// numbers, dropping and reporting any that the block does not have.
pub(super) fn resolve_emphasize_lines(
    raw: Option<&(String, usize, String)>,
    line_count: usize,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<NonZeroU32> {
    let Some((value, line_index, raw_line)) = raw else {
        return Vec::new();
    };

    let mut report = |message: String| {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::CodeBlockEmphasizeLinesInvalid,
            format!("{directive}: :emphasize-lines: {message}"),
            ctx.line_span(*line_index, raw_line),
        ));
    };

    let mut lines = Vec::new();
    for part in value.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match parse_line_range(part) {
            Some(range) => lines.extend(range),
            None => report(format!("could not read '{part}'")),
        }
    }

    lines.sort_unstable();
    lines.dedup();

    let limit = u32::try_from(line_count).unwrap_or(u32::MAX);
    let (within, beyond): (Vec<NonZeroU32>, Vec<NonZeroU32>) =
        lines.into_iter().partition(|line| line.get() <= limit);
    for line in &beyond {
        report(format!(
            "line {line} is past the end of a {line_count}-line block"
        ));
    }
    within
}

/// Reads one comma-separated entry: either `4` or a `3-5` range.
fn parse_line_range(part: &str) -> Option<Vec<NonZeroU32>> {
    if let Some((first, last)) = part.split_once('-') {
        let first: NonZeroU32 = first.trim().parse().ok()?;
        let last: NonZeroU32 = last.trim().parse().ok()?;
        if last < first {
            return None;
        }
        return Some(
            (first.get()..=last.get())
                .filter_map(NonZeroU32::new)
                .collect(),
        );
    }
    part.parse::<NonZeroU32>().ok().map(|line| vec![line])
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{codes, parse};
    use super::*;
    use rusty_sphinx_ast::DiagnosticCode;

    #[test]
    fn test_parse_code_block_expands_an_emphasize_lines_range() {
        // Given
        let body = [
            "   :emphasize-lines: 1,3-5",
            "",
            "   a",
            "   b",
            "   c",
            "   d",
            "   e",
        ];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let emphasized: Vec<u32> = block.emphasize_lines.iter().map(|l| l.get()).collect();
        assert_eq!(emphasized, vec![1, 3, 4, 5]);
    }

    #[test]
    fn test_parse_code_block_sorts_and_deduplicates_emphasized_lines() {
        // Given — written out of order, with an overlap
        let body = ["   :emphasize-lines: 3,1,2-3", "", "   a", "   b", "   c"];

        // When
        let (block, _) = parse("python", &body);

        // Then
        let emphasized: Vec<u32> = block.emphasize_lines.iter().map(|l| l.get()).collect();
        assert_eq!(emphasized, vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_code_block_reports_an_emphasized_line_past_the_end() {
        // Given — a two-line block asked to emphasize line nine
        let body = ["   :emphasize-lines: 1,9", "", "   a", "   b"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then — the valid one survives, the impossible one is reported
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockEmphasizeLinesInvalid]
        );
        let emphasized: Vec<u32> = block.emphasize_lines.iter().map(|l| l.get()).collect();
        assert_eq!(emphasized, vec![1]);
    }

    #[test]
    fn test_parse_code_block_reports_an_unreadable_emphasize_lines_entry() {
        // Given
        let body = ["   :emphasize-lines: 1,two", "", "   a", "   b"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockEmphasizeLinesInvalid]
        );
        let emphasized: Vec<u32> = block.emphasize_lines.iter().map(|l| l.get()).collect();
        assert_eq!(emphasized, vec![1]);
    }

    #[test]
    fn test_parse_code_block_rejects_a_backwards_emphasize_range() {
        // Given — `5-3` names no lines at all
        let body = ["   :emphasize-lines: 5-3", "", "   a", "   b", "   c"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockEmphasizeLinesInvalid]
        );
        assert!(block.emphasize_lines.is_empty());
    }

    #[test]
    fn test_parse_line_range_reads_a_single_line() {
        // Given / When
        let parsed = parse_line_range("4");

        // Then
        assert_eq!(parsed, Some(vec![NonZeroU32::new(4).unwrap()]));
    }

    #[test]
    fn test_parse_line_range_rejects_zero() {
        // Given — line numbering is 1-based
        // When / Then
        assert_eq!(parse_line_range("0"), None);
    }

    #[test]
    fn test_parse_line_range_reads_an_inclusive_range() {
        // Given / When
        let parsed = parse_line_range("2-4").expect("a well-formed range");

        // Then — inclusive at both ends
        let lines: Vec<u32> = parsed.iter().map(|l| l.get()).collect();
        assert_eq!(lines, vec![2, 3, 4]);
    }
}
