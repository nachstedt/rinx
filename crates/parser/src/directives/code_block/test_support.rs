//! The fixtures the code-block test modules share.
//!
//! Its own module because all five sibling test modules drive the parsers
//! through the same two entry points, and each would otherwise restate the
//! same dozen lines of context setup.
#![cfg(test)]

use rusty_sphinx_ast::{CodeBlock, CodeBlockSource, DiagnosticCode, Directive, Domain};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

use super::block::parse_code_block;
use super::highlight::parse_highlight;

/// Parses a `.. code-block::` body, returning the block and any diagnostics.
/// `body_lines` are indented as `collect_directive_body` hands them over.
pub(super) fn parse(argument: &str, body_lines: &[&str]) -> (CodeBlock, Diagnostics) {
    parse_as(CodeBlockSource::CodeBlock, argument, body_lines)
}

/// The same, for whichever directive spelling a test is about.
pub(super) fn parse_as(
    source: CodeBlockSource,
    argument: &str,
    body_lines: &[&str],
) -> (CodeBlock, Diagnostics) {
    let mut diagnostics = Diagnostics::default();
    let directive = parse_code_block(
        source,
        argument,
        body_lines,
        &mut diagnostics,
        &ParseCtx::with_domain(Domain::Py),
    );
    let Directive::CodeBlock(block) = directive else {
        panic!("Expected a CodeBlock, got {directive:?}");
    };
    (block, diagnostics)
}

/// Parses a `.. highlight::`, returning the directive and any diagnostics.
pub(super) fn highlight(argument: &str, body_lines: &[&str]) -> (Directive, Diagnostics) {
    let mut diagnostics = Diagnostics::default();
    let directive = parse_highlight(
        argument,
        body_lines,
        &mut diagnostics,
        &ParseCtx::with_domain(Domain::Py),
    );
    (directive, diagnostics)
}

/// The codes of everything recorded, for asserting on a diagnostic set
/// without depending on message wording.
pub(super) fn codes(diagnostics: &Diagnostics) -> Vec<DiagnosticCode> {
    diagnostics.iter().map(|d| d.code).collect()
}
