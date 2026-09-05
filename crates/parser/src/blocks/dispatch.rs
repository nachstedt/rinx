//! The block-level dispatch chain: the crate's `parse` entry points, the
//! `parse_blocks` loop each construct plugs into, and the paragraph fallback
//! every line that matches nothing else lands in.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::try_parse_directive;
use crate::headings::{Adornment, detect_adornment, try_parse_heading};
use crate::inline::{SourceMap, parse_inline_text_mapped};
use rusty_sphinx_ast::{
    CodeLanguage, Diagnostic, DiagnosticCode, Document, Domain, Node, Suppression, SuppressionCodes,
};

use super::block_quote::try_parse_block_quote;
use super::bullet_list::try_parse_bullet_list;
use super::comment::{parse_noqa_comment, try_parse_comment};
use super::definition_list::try_parse_definition_list;
use super::doctest_block::try_parse_doctest_block;
use super::enumerated_list::try_parse_enumerated_list;
use super::line_block::try_parse_line_block;
use super::literal_block::collect_literal_block_body;
use super::option_list::try_parse_option_list;
use super::simple_table::try_parse_simple_table;
use super::table::try_parse_grid_table;
use super::target::try_parse_target;
use super::transition::try_parse_transition;

/// Parses an RST-formatted string into a Document.
///
/// Current logic:
/// - A heading is defined as a line of text followed by a line composed purely
///   of punctuation character(s) (e.g. `===` or `---`) that is at least as long
///   as the text line above it.
/// - Heading levels are determined by the order in which each underline character
///   is first encountered in the document: the first character seen becomes level
///   1, the second distinct character level 2, etc.
/// - Otherwise, consecutive non-blank lines are grouped into a Paragraph.
///
/// # Panics
///
/// The internal implementation uses `expect()` on an iterator that is guaranteed
/// to be non-empty by preceding checks.
#[must_use]
pub fn parse(path: &str, input: &str) -> Document {
    parse_with_domain(path, input, Domain::Py)
}

/// Parses an RST-formatted string into a `Document`, resolving any bare
/// (unprefixed) domain directive or role — e.g. `.. function::` or `:func:` —
/// via `default_domain`. `parse` is a thin wrapper defaulting to `Domain::Py`.
///
/// The parse runs with no filesystem access, so a `.. csv-table::` carrying a
/// `:file:` option reports a diagnostic instead of reading anything. Callers
/// that *can* resolve paths use [`parse_with_ctx`] instead.
#[must_use]
pub fn parse_with_domain(path: &str, input: &str, default_domain: Domain) -> Document {
    parse_with_ctx(path, input, &ParseCtx::with_domain(default_domain))
}

/// Parses an RST-formatted string into a `Document` under a fully specified
/// [`ParseCtx`] — the entry point for callers that can supply a
/// [`crate::CsvFileLoader`] alongside the default domain.
///
/// # Panics
///
/// The internal implementation uses `expect()` on an iterator that is guaranteed
/// to be non-empty by preceding checks.
#[must_use]
pub fn parse_with_ctx(path: &str, input: &str, ctx: &ParseCtx<'_>) -> Document {
    let all_lines: Vec<&str> = input.lines().collect();
    // The document's leading field list is metadata, not content: consuming it
    // here keeps it out of the block parser, where it would otherwise render as
    // a stray paragraph.
    let (metadata, metadata_lines) = super::docinfo::split_document_metadata(&all_lines);
    let lines = &all_lines[metadata_lines..];
    let ctx = &ctx.nested(metadata_lines, 0);

    let mut adornment_order: Vec<Adornment> = Vec::new();
    let mut diagnostics = Diagnostics::default();

    let mut nodes = parse_blocks(lines, &mut adornment_order, &mut diagnostics, ctx);

    let mut index_id_counter = 0;
    super::index_ids::assign_index_ids(&mut nodes, &mut index_id_counter);

    let (entries, suppressions, source_files) = diagnostics.into_parts();
    let mut doc = Document::new(path.to_string(), nodes);
    doc.diagnostics = entries;
    doc.suppressions = suppressions;
    doc.metadata = metadata;
    doc.source_files = source_files;
    doc
}
pub(crate) fn parse_blocks(
    lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut i = 0;
    // `.. noqa:` comments seen since the last block. They accumulate rather
    // than replace one another so that two written in a row both apply, and
    // they survive the blank-line skip below — a suppression is nearly always
    // separated from its block by one.
    let mut pending_noqa: Vec<SuppressionCodes> = Vec::new();

    while i < lines.len() {
        let line = lines[i].trim_end();

        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // Read before the comment parser below, which consumes the line and
        // discards its text. A `.. noqa:` is still a comment and still renders
        // to nothing; only its payload is taken first.
        if let Some(noqa) = parse_noqa_comment(line) {
            for unknown in &noqa.unknown {
                diagnostics.push(Diagnostic::at(
                    DiagnosticCode::NoqaUnknownCode,
                    format!(
                        "'{unknown}' is not a diagnostic code, so this `.. noqa:` suppresses nothing"
                    ),
                    ctx.line_span(i, lines[i]),
                ));
            }
            pending_noqa.push(noqa.codes);
        }

        // A comment is the one construct that does *not* consume the pending
        // suppressions: whatever sits between a `.. noqa:` and its block —
        // including a second `.. noqa:` — is not the thing being suppressed.
        if let Some((consumed, node)) = try_parse_comment(lines, i) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        let (consumed, new_nodes) =
            try_parse_construct(lines, i, &nodes, adornment_order, diagnostics, ctx)
                .unwrap_or_else(|| parse_paragraph(lines, i, ctx));
        attach_pending_noqa(&mut pending_noqa, diagnostics, ctx, i, consumed);
        nodes.extend(new_nodes);
        i += consumed;
    }
    nodes
}

/// Tries every block construct in turn, in the order the RST grammar requires,
/// returning the lines consumed and the nodes produced.
///
/// Split out of [`parse_blocks`] so that the loop is about *bookkeeping* — the
/// blank-line skip, the pending `.. noqa:` comments, accumulating nodes — and
/// this is about *recognition*. Every arm did the same three things with its
/// result, and saying that once is what keeps a new construct to one line here.
///
/// Returns `None` when the line opens no construct, which the caller turns
/// into a paragraph.
fn try_parse_construct(
    lines: &[&str],
    i: usize,
    nodes: &[Node],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Vec<Node>)> {
    let single = |(consumed, node): (usize, Node)| (consumed, vec![node]);

    // Indentation is checked first, ahead of every marker below, matching
    // docutils' own precedence: an indented directive, list or table nests
    // inside a block quote rather than being matched in place. Every other
    // branch here only ever sees a zero-indent line as a result.
    if let Some(found) = try_parse_block_quote(lines, i, adornment_order, diagnostics, ctx) {
        return Some(found);
    }
    if let Some(found) = try_parse_directive(lines, i, adornment_order, diagnostics, ctx) {
        return Some(found);
    }
    if let Some(found) = try_parse_target(lines, i) {
        return Some(single(found));
    }
    if let Some(found) = try_parse_transition(lines, i, nodes, diagnostics, ctx) {
        return Some(single(found));
    }
    if let Some(found) = try_parse_heading(lines, i, adornment_order, ctx) {
        return Some(single(found));
    }
    // docutils' own relative order: `line_block` is tried immediately before
    // `grid_table_top`. A `|`-marker line can't collide with anything tried
    // earlier in this chain (directive/target/transition/heading markers are
    // all visually distinct), so it slots in here unmodified.
    if let Some(found) = try_parse_line_block(lines, i, diagnostics, ctx) {
        return Some(single(found));
    }
    if let Some(found) = try_parse_grid_table(lines, i, adornment_order, diagnostics, ctx) {
        return Some(single(found));
    }
    if let Some(found) = try_parse_simple_table(lines, i, adornment_order, diagnostics, ctx) {
        return Some(single(found));
    }
    if let Some(found) = try_parse_bullet_list(lines, i, adornment_order, diagnostics, ctx) {
        return Some(single(found));
    }
    // Before the definition list: `detect_definition_term` matches any line
    // followed by a more-indented one, which every multi-line enumerated
    // item also is. docutils resolves this the same way, trying its
    // `enumerator` transition before falling through to text.
    if let Some(found) = try_parse_enumerated_list(lines, i, adornment_order, diagnostics, ctx) {
        return Some(single(found));
    }
    // Before the definition list, for the same reason as the enumerated list
    // above: an option marker with no same-line description (`--long`
    // followed by an indented line) has the exact "line, then more-indented
    // line" shape `detect_definition_term` matches. docutils' own transition
    // order is bullet -> enumerated -> option -> definition.
    if let Some(found) = try_parse_option_list(lines, i, adornment_order, diagnostics, ctx) {
        return Some(single(found));
    }
    if let Some(found) = try_parse_definition_list(lines, i, adornment_order, diagnostics, ctx) {
        return Some(single(found));
    }
    try_parse_doctest_block(lines, i).map(single)
}

/// Resolves every pending `.. noqa:` onto the block just parsed, which spans
/// `consumed` lines from `start`.
///
/// This is where the scope rule lives: a suppression applies to the *next*
/// block, so it is only turned into a line range once that block's extent is
/// known. Because `parse_blocks` recurses, a `.. noqa:` written inside a
/// directive body or a list item resolves against that nested block, and one
/// written before a container covers everything nested in it.
fn attach_pending_noqa(
    pending: &mut Vec<SuppressionCodes>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
    start: usize,
    consumed: usize,
) {
    if pending.is_empty() {
        return;
    }
    // A block with no source position cannot be suppressed by line, so the
    // pending comments are dropped rather than pinned to an invented range.
    let Some(span) = ctx.lines_span(start, start + consumed.saturating_sub(1), "") else {
        pending.clear();
        return;
    };
    for codes in pending.drain(..) {
        diagnostics.suppress(Suppression {
            start_line: span.start.line,
            end_line: span.end.line,
            codes,
            // The comment and the block it covers are always in the same
            // file — an `.. include::` splices whole blocks, never half of
            // one — so the block's own attribution is the comment's.
            file: span.file,
        });
    }
}

fn parse_paragraph(lines: &[&str], i: usize, ctx: &ParseCtx<'_>) -> (usize, Vec<Node>) {
    let mut paragraph_text = String::new();
    // Built alongside the joined text, because this loop is the only place
    // both forms exist at once: once the lines are trimmed and joined, which
    // line a byte offset came from is no longer recoverable.
    let mut map = SourceMap::none();
    let mut current_pos_line = i;

    while current_pos_line < lines.len() {
        let line = lines[current_pos_line].trim_end();
        if line.trim().is_empty() {
            break;
        }

        if !paragraph_text.is_empty() {
            paragraph_text.push('\n');
        }
        let trimmed = line.trim();
        map.push(
            paragraph_text.len(),
            trimmed,
            current_pos_line,
            // The leading whitespace `trim` just removed, in characters.
            line.chars().take_while(|c| c.is_whitespace()).count(),
        );
        paragraph_text.push_str(trimmed);
        current_pos_line += 1;

        // Peek at next line to ensure we don't consume a heading's text line or overline
        if current_pos_line < lines.len() && detect_adornment(lines, current_pos_line).is_some() {
            break;
        }
    }

    let inlines = parse_inline_text_mapped(&paragraph_text, ctx.default_domain, &map, ctx);

    // Detect trailing "::" to introduce a literal block
    let trailing_double_colon = paragraph_text.trim_end().ends_with("::");
    if trailing_double_colon {
        let trimmed = paragraph_text.trim_end();
        // Determine whether the whole paragraph is just "::" (standalone introducer)
        let only_colon = trimmed.trim() == "::";
        let lit_start = current_pos_line;
        let (lit_consumed, content) = collect_literal_block_body(lines, lit_start);
        let total_consumed = current_pos_line - i + lit_consumed;
        let literal_node = Node::LiteralBlock {
            language: CodeLanguage::Inherit,
            content,
        };
        if only_colon {
            // The "::" line itself is suppressed; emit only the literal block
            return (total_consumed, vec![literal_node]);
        }
        // Strip trailing "::" → ":" and emit paragraph + literal block
        let stripped = trimmed[..trimmed.len() - 1].trim_end().to_string();
        let inlines = parse_inline_text_mapped(&stripped, ctx.default_domain, &map, ctx);
        return (total_consumed, vec![Node::Paragraph(inlines), literal_node]);
    }

    (current_pos_line - i, vec![Node::Paragraph(inlines)])
}

#[cfg(test)]
mod tests {

    use super::*;
    use rusty_sphinx_ast::InlineNode;

    #[test]
    fn test_parse_blocks_empty_input() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();
        let nodes = parse_blocks(
            &[],
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_parse_blocks_simple_paragraph() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();
        let lines = vec!["Hello world"];
        let nodes = parse_blocks(
            &lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            Node::Paragraph(inlines) => {
                assert_eq!(inlines.len(), 1);
                assert_eq!(inlines[0], InlineNode::Text("Hello world".to_string()));
            }
            _ => panic!("Expected paragraph"),
        }
    }

    #[test]
    fn test_parse_blocks_maintains_adornment_order() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        let lines1 = vec!["Title 1", "======="];
        let nodes1 = parse_blocks(
            &lines1,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert_eq!(nodes1.len(), 1);

        let lines2 = vec!["Title 2", "-------"];
        let nodes2 = parse_blocks(
            &lines2,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert_eq!(nodes2.len(), 1);

        assert_eq!(adornment_order.len(), 2);
    }

    #[test]
    fn test_parse_blocks_collects_diagnostics() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // This will trigger a diagnostic because of the unknown option
        let lines = vec![".. toctree::", "   :unknown_option: value"];
        let nodes = parse_blocks(
            &lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        assert_eq!(nodes.len(), 1);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics[0]
                .message
                .contains("Invalid or non-standard Sphinx toctree option")
        );
    }

    // --- try_parse_comment unit tests ---
}
