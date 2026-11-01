//! The quick fix writing a `.. noqa:` (roadmap #35): one click silences any
//! finding the editor shows that a comment can silence.
//!
//! Where the comment goes is not decided here. The parser records each block
//! a comment written before it would cover — a `rinx_ast::SuppressionSite`,
//! built where it resolves a written comment onto its block — and this picks
//! the innermost one whose suppression covers the finding, so the comment it
//! writes silences exactly that block: what the parser will read it as.
//!
//! A site's block can be written before when its first line holds nothing
//! before the block but its indentation. That rules out the first block of a
//! list item, whose marker stands there, and a block quote, whose comment
//! would take the quote for its own body; the list, or the blocks inside the
//! quote, are written before instead. A `.. noqa:` already listing codes
//! before the block is extended rather than joined by a second one.

use lsp_types::{Range, TextEdit, Uri};
use rinx_ast::{DiagnosticCode, FileId, Position, Span, SuppressionCodes, SuppressionSite};

use crate::diagnostics::{Placed, to_lsp_diagnostic};
use crate::position::{PositionEncoding, to_lsp_position};

/// One `.. noqa:` edit, and the diagnostics it silences.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NoqaFix {
    /// The file edited: the one the diagnostics are in.
    pub uri: Uri,
    pub edit: TextEdit,
    pub title: String,
    /// The diagnostics silenced, as published.
    pub diagnostics: Vec<lsp_types::Diagnostic>,
}

/// The `.. noqa:` fixes for what `findings` — the shown findings of one
/// parse, whose `sites` its source map records — shows in `uri` within
/// `range`. `file_text` names the file a span's file id points into, and its
/// text. One fix per block and code, however many findings it silences.
pub(crate) fn noqa_fixes<'t>(
    findings: &[Placed<'_>],
    sites: &[SuppressionSite],
    file_text: &dyn Fn(Option<FileId>) -> Option<(Uri, &'t str)>,
    uri: &Uri,
    range: Range,
    encoding: PositionEncoding,
) -> Vec<NoqaFix> {
    let mut fixes: Vec<NoqaFix> = Vec::new();
    for found in findings {
        let Some(span) = found.diagnostic.span else {
            continue;
        };
        if found.uri != *uri || found.range.end < range.start || range.end < found.range.start {
            continue;
        }
        let Some((file, text)) = file_text(span.file).filter(|(file, _)| file == uri) else {
            continue;
        };
        let code = found.diagnostic.code;
        let Some(edit) = innermost_site(sites, code, span, text)
            .and_then(|site| suppression_edit(site, code, text, encoding))
        else {
            continue;
        };
        let diagnostic = to_lsp_diagnostic(found.diagnostic, &found.judgement, found.range);
        match fixes.iter_mut().find(|fix| fix.edit == edit) {
            Some(fix) => fix.diagnostics.push(diagnostic),
            None => fixes.push(NoqaFix {
                uri: file,
                edit,
                title: format!("Suppress {} with .. noqa:", code.as_str()),
                diagnostics: vec![diagnostic],
            }),
        }
    }
    fixes
}

/// The innermost of `sites` a comment naming `code` can be written for that
/// silences `code` found at `span`, in the file whose text is `text`.
fn innermost_site<'s>(
    sites: &'s [SuppressionSite],
    code: DiagnosticCode,
    span: Span,
    text: &str,
) -> Option<&'s SuppressionSite> {
    let codes = SuppressionCodes::Only(vec![code]);
    sites
        .iter()
        .filter(|site| site.suppression(codes.clone()).suppresses(code, Some(span)))
        .filter(|site| site.comment.is_some() || is_written_before(site, text))
        .max_by_key(|site| {
            (
                site.block.start.line,
                std::cmp::Reverse(site.block.end.line),
                site.block.start.column,
            )
        })
}

/// Whether a comment can be written before `site`'s block: on its first line
/// of `text`, only spaces stand before the block's column, and the block
/// starts there.
fn is_written_before(site: &SuppressionSite, text: &str) -> bool {
    let Some(line) = line_of(text, site.block.start.line) else {
        return false;
    };
    let indent = column_index(site.block.start.column);
    let mut characters = line.chars();
    characters.by_ref().take(indent).all(|c| c == ' ')
        && characters.next().is_some_and(|c| !c.is_whitespace())
}

/// The edit making a comment before `site`'s block name `code`: adding it to
/// the listed `.. noqa:` already there, or writing one, at the block's own
/// indentation, followed by the blank line ending it.
fn suppression_edit(
    site: &SuppressionSite,
    code: DiagnosticCode,
    text: &str,
    encoding: PositionEncoding,
) -> Option<TextEdit> {
    if let Some(comment) = site.comment {
        let line = line_of(text, comment.start.line)?;
        let end = Position::new(
            comment.start.line,
            u32::try_from(line.trim_end().chars().count() + 1).ok()?,
        );
        let at = to_lsp_position(end, line, encoding);
        return Some(TextEdit::new(
            Range::new(at, at),
            format!(", {}", code.as_str()),
        ));
    }
    let at = lsp_types::Position::new(site.block.start.line.checked_sub(1)?, 0);
    let indent = " ".repeat(column_index(site.block.start.column));
    Some(TextEdit::new(
        Range::new(at, at),
        format!("{indent}.. noqa: {}\n\n", code.as_str()),
    ))
}

/// The 1-based line `line` of `text`.
fn line_of(text: &str, line: u32) -> Option<&str> {
    text.split('\n')
        .nth(usize::try_from(line.checked_sub(1)?).ok()?)
}

/// The 0-based character index of the 1-based `column`.
fn column_index(column: u32) -> usize {
    usize::try_from(column.saturating_sub(1)).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    use rinx_ast::Diagnostic;
    use rinx_parser::{ParseCtx, parse};

    use crate::position::to_lsp_range;
    use crate::project::Judgement;
    use crate::uri::file_uri;

    const CODE: DiagnosticCode = DiagnosticCode::LinkBrokenRef;

    fn uri() -> Uri {
        file_uri(std::path::Path::new("/docs/index.rst")).unwrap()
    }

    /// `CODE` found on `line` (1-based) of `text` from `column` on.
    fn found_at(line: u32, column: u32) -> Diagnostic {
        Diagnostic::new(
            CODE,
            "found",
            Span::new(Position::new(line, column), Position::new(line, column + 1)),
        )
    }

    /// The fixes for `diagnostics`, found in the document `text`, within
    /// `range`.
    fn fixes_in(text: &str, diagnostics: &[Diagnostic], range: Range) -> Vec<NoqaFix> {
        let parsed = parse("index.rst", text, &ParseCtx::default());
        let uri = uri();
        let findings: Vec<Placed<'_>> = diagnostics
            .iter()
            .map(|diagnostic| Placed {
                diagnostic,
                judgement: Judgement::as_reported(diagnostic),
                uri: uri.clone(),
                range: diagnostic.span.map_or_else(Range::default, |span| {
                    to_lsp_range(span, text, PositionEncoding::Utf16)
                }),
            })
            .collect();
        noqa_fixes(
            &findings,
            parsed.source_map.suppression_sites(),
            &|file| file.is_none().then(|| (uri.clone(), text)),
            &uri,
            range,
            PositionEncoding::Utf16,
        )
    }

    /// The fixes for `CODE` found on `line` (1-based) of `text`, at its
    /// first non-space character.
    fn fixes_for(text: &str, line: u32) -> Vec<NoqaFix> {
        let written = line_of(text, line).unwrap();
        let column = u32::try_from(written.len() - written.trim_start().len() + 1).unwrap();
        let diagnostic = found_at(line, column);
        let range = to_lsp_range(diagnostic.span.unwrap(), text, PositionEncoding::Utf16);
        fixes_in(text, &[diagnostic], range)
    }

    /// `text` with `fix` applied.
    fn applied(text: &str, fix: &NoqaFix) -> String {
        let start = fix.edit.range.start;
        let offset: usize = text
            .split('\n')
            .take(start.line as usize)
            .map(|line| line.len() + 1)
            .sum::<usize>()
            + start.character as usize;
        let mut edited = text.to_string();
        edited.insert_str(offset, &fix.edit.new_text);
        edited
    }

    fn whole() -> Range {
        Range::new(
            lsp_types::Position::new(0, 0),
            lsp_types::Position::new(u32::MAX, 0),
        )
    }

    #[test]
    fn test_a_paragraphs_finding_is_suppressed_before_the_paragraph() {
        // Given
        let text = "Title\n=====\n\nSee :ref:`x`.\n";

        // When
        let fixes = fixes_for(text, 4);

        // Then
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0].title, "Suppress link.broken-ref with .. noqa:");
        assert_eq!(fixes[0].diagnostics.len(), 1);
        assert_eq!(
            applied(text, &fixes[0]),
            "Title\n=====\n\n.. noqa: link.broken-ref\n\nSee :ref:`x`.\n"
        );
    }

    #[test]
    fn test_a_finding_in_a_directive_body_is_suppressed_inside_it() {
        // Given
        let text = ".. note::\n\n   First.\n\n   See :ref:`x`.\n";

        // When
        let fixes = fixes_for(text, 5);

        // Then — at the body's indentation, before its own paragraph
        assert_eq!(
            applied(text, &fixes[0]),
            ".. note::\n\n   First.\n\n   .. noqa: link.broken-ref\n\n   See :ref:`x`.\n"
        );
    }

    #[test]
    fn test_a_list_items_first_block_is_suppressed_before_the_list() {
        // Given
        let text = "- See :ref:`x`.\n- Other.\n";

        // When
        let fixes = fixes_for(text, 1);

        // Then
        assert_eq!(
            applied(text, &fixes[0]),
            ".. noqa: link.broken-ref\n\n- See :ref:`x`.\n- Other.\n"
        );
    }

    #[test]
    fn test_a_list_items_later_block_is_suppressed_inside_the_item() {
        // Given
        let text = "- First.\n\n  See :ref:`x`.\n";

        // When
        let fixes = fixes_for(text, 3);

        // Then
        assert_eq!(
            applied(text, &fixes[0]),
            "- First.\n\n  .. noqa: link.broken-ref\n\n  See :ref:`x`.\n"
        );
    }

    #[test]
    fn test_a_block_quotes_finding_is_suppressed_inside_the_quote() {
        // Given
        let text = "Text.\n\n   See :ref:`x`.\n";

        // When
        let fixes = fixes_for(text, 3);

        // Then — a comment before the quote would take it for its body
        assert_eq!(
            applied(text, &fixes[0]),
            "Text.\n\n   .. noqa: link.broken-ref\n\n   See :ref:`x`.\n"
        );
    }

    #[test]
    fn test_a_table_cells_finding_is_suppressed_before_the_table() {
        // Given
        let text = "=====  =====\nA      :ref:`x`\n=====  =====\n";

        // When
        let fixes = fixes_for(text, 2);

        // Then
        assert_eq!(
            applied(text, &fixes[0]),
            ".. noqa: link.broken-ref\n\n=====  =====\nA      :ref:`x`\n=====  =====\n"
        );
    }

    #[test]
    fn test_a_listed_noqa_before_the_block_is_extended() {
        // Given
        let text = ".. noqa: link.broken-term  \n\nSee :ref:`x`.\n";

        // When
        let fixes = fixes_for(text, 3);

        // Then
        assert_eq!(
            applied(text, &fixes[0]),
            ".. noqa: link.broken-term, link.broken-ref  \n\nSee :ref:`x`.\n"
        );
    }

    #[test]
    fn test_a_finding_without_a_span_gets_no_fix() {
        // Given / When
        let fixes = fixes_in("Text.\n", &[Diagnostic::at(CODE, "found", None)], whole());

        // Then
        assert_eq!(fixes, []);
    }

    #[test]
    fn test_a_finding_in_a_file_that_cannot_be_named_gets_no_fix() {
        // Given — a span in an included file the lookup does not know
        let span =
            Span::new(Position::new(1, 1), Position::new(1, 2)).with_file(Some(FileId::new(0)));

        // When
        let fixes = fixes_in("Text.\n", &[Diagnostic::new(CODE, "found", span)], whole());

        // Then
        assert_eq!(fixes, []);
    }

    #[test]
    fn test_two_findings_of_one_code_in_one_block_share_a_fix() {
        // Given / When
        let fixes = fixes_in(
            "See :ref:`x` and :ref:`y`.\n",
            &[found_at(1, 5), found_at(1, 18)],
            whole(),
        );

        // Then
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0].diagnostics.len(), 2);
    }

    #[test]
    fn test_a_finding_outside_the_range_gets_no_fix() {
        // Given
        let first_line = Range::new(
            lsp_types::Position::new(0, 0),
            lsp_types::Position::new(0, 2),
        );

        // When
        let fixes = fixes_in("One.\n\nSee :ref:`x`.\n", &[found_at(3, 5)], first_line);

        // Then
        assert_eq!(fixes, []);
    }

    #[test]
    fn test_line_of_counts_from_one() {
        // Given / When / Then
        assert_eq!(line_of("a\nb\n", 2), Some("b"));
        assert_eq!(line_of("a\nb\n", 0), None);
    }

    #[test]
    fn test_column_index_counts_from_zero() {
        // Given / When / Then
        assert_eq!(column_index(1), 0);
        assert_eq!(column_index(4), 3);
    }

    #[test]
    fn test_is_written_before_needs_only_spaces_before_the_block() {
        // Given
        let site = |column| SuppressionSite {
            block: Span::new(Position::new(1, column), Position::new(1, column)),
            comment: None,
        };

        // When / Then
        assert!(is_written_before(&site(3), "  Text."));
        assert!(!is_written_before(&site(3), "- Text."));
        assert!(!is_written_before(&site(1), "   Quoted."));
    }
}
