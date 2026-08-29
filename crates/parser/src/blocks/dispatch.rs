//! The block-level dispatch chain: the crate's `parse` entry points, the
//! `parse_blocks` loop each construct plugs into, and the paragraph fallback
//! every line that matches nothing else lands in.

use crate::context::ParseCtx;
use crate::directives::try_parse_directive;
use crate::headings::{Adornment, detect_adornment, try_parse_heading};
use crate::inline::parse_inline_text;
use rusty_sphinx_ast::{Document, Domain, Node};

use super::bullet_list::try_parse_bullet_list;
use super::comment::try_parse_comment;
use super::definition_list::try_parse_definition_list;
use super::doctest_block::try_parse_doctest_block;
use super::enumerated_list::try_parse_enumerated_list;
use super::literal_block::collect_literal_block_body;
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
    let lines: Vec<&str> = input.lines().collect();
    let mut adornment_order: Vec<Adornment> = Vec::new();
    let mut diagnostics = Vec::new();

    let mut nodes = parse_blocks(&lines, &mut adornment_order, &mut diagnostics, ctx);

    let mut index_id_counter = 0;
    super::index_ids::assign_index_ids(&mut nodes, &mut index_id_counter);

    if !diagnostics.is_empty() {
        eprintln!("Diagnostics for '{path}':");
        for diag in &diagnostics {
            eprintln!("  - {diag}");
        }
    }

    let mut doc = Document::new(path.to_string(), nodes);
    doc.diagnostics = diagnostics;
    doc
}
pub(crate) fn parse_blocks(
    lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    ctx: &ParseCtx<'_>,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim_end();

        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_directive(lines, i, adornment_order, diagnostics, ctx)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_target(lines, i) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_comment(lines, i) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_transition(lines, i, &nodes, diagnostics) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_heading(lines, i, adornment_order, ctx.default_domain)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_grid_table(lines, i, adornment_order, diagnostics, ctx)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_simple_table(lines, i, adornment_order, diagnostics, ctx)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_bullet_list(lines, i, adornment_order, diagnostics, ctx)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        // Before the definition list: `detect_definition_term` matches any line
        // followed by a more-indented one, which every multi-line enumerated
        // item also is. docutils resolves this the same way, trying its
        // `enumerator` transition before falling through to text.
        if let Some((consumed, node)) =
            try_parse_enumerated_list(lines, i, adornment_order, diagnostics, ctx)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_definition_list(lines, i, adornment_order, diagnostics, ctx)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_doctest_block(lines, i) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        let (consumed, new_nodes) = parse_paragraph(lines, i, ctx);
        nodes.extend(new_nodes);
        i += consumed;
    }
    nodes
}
fn parse_paragraph(lines: &[&str], i: usize, ctx: &ParseCtx<'_>) -> (usize, Vec<Node>) {
    let mut paragraph_text = String::new();
    let mut current_pos_line = i;

    while current_pos_line < lines.len() {
        let line = lines[current_pos_line].trim_end();
        if line.trim().is_empty() {
            break;
        }

        if !paragraph_text.is_empty() {
            paragraph_text.push('\n');
        }
        paragraph_text.push_str(line.trim());
        current_pos_line += 1;

        // Peek at next line to ensure we don't consume a heading's text line or overline
        if current_pos_line < lines.len() && detect_adornment(lines, current_pos_line).is_some() {
            break;
        }
    }

    let inlines = parse_inline_text(&paragraph_text, ctx.default_domain);

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
            language: None,
            content,
        };
        if only_colon {
            // The "::" line itself is suppressed; emit only the literal block
            return (total_consumed, vec![literal_node]);
        }
        // Strip trailing "::" → ":" and emit paragraph + literal block
        let stripped = trimmed[..trimmed.len() - 1].trim_end().to_string();
        let inlines = parse_inline_text(&stripped, ctx.default_domain);
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
        let mut diagnostics = Vec::new();
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
        let mut diagnostics = Vec::new();
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
        let mut diagnostics = Vec::new();

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
        let mut diagnostics = Vec::new();

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
        assert!(diagnostics[0].contains("Invalid or non-standard Sphinx toctree option"));
    }

    // --- try_parse_comment unit tests ---
}
