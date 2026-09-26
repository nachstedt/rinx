//! Test fixtures shared by [`super::data_table`]'s and
//! [`super::table_directive`]'s renderer tests: a document-rendering
//! shortcut and a helper building a row of single-paragraph text cells.
#![cfg(test)]

use rinx_ast::{Document, InlineNode, Node, TableCell, TableRow};
use rinx_index::ProjectIndex;

/// Renders `doc` against an empty project index, returning the resulting
/// HTML — the shortcut every table-rendering test in this crate uses.
pub(super) fn render_doc(doc: &Document) -> String {
    let index = ProjectIndex::default();
    crate::render(doc, &index, &doc.path).html
}

/// Builds one [`TableRow`] whose cells each hold a single-paragraph,
/// plain-text `Node`, one cell per given string.
pub(super) fn table_row(cells: &[&str]) -> TableRow {
    TableRow {
        cells: cells
            .iter()
            .map(|text| TableCell {
                colspan: 1,
                rowspan: 1,
                content: vec![Node::Paragraph(vec![InlineNode::Text((*text).to_string())])],
            })
            .collect(),
    }
}
