//! Fixtures the heading test modules share.

use rinx_ast::{DiagnosticCode, Document};

/// The codes of every diagnostic `doc` reports, in order.
pub(super) fn diagnostic_codes(doc: &Document) -> Vec<DiagnosticCode> {
    doc.diagnostics.iter().map(|d| d.code).collect()
}
