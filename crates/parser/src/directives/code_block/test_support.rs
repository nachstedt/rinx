//! The fixtures the code-block test modules share.
//!
//! Its own module because all five sibling test modules drive the parsers
//! through the same two entry points, and each would otherwise restate the
//! same dozen lines of context setup.
#![cfg(test)]

use rinx_ast::{CodeBlock, CodeBlockSource, DiagnosticCode, Directive, Domain};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::reader::{test_invocation, written_after_blank};

use super::block::{CODE_BLOCK_SPEC, parse_code_block};
use super::highlight::{HIGHLIGHT_SPEC, parse_highlight};

/// Parses a `.. code-block::` body, returning the block and any diagnostics.
/// `body_lines` are indented as `collect_directive_body` hands them over, and
/// written after a blank line unless they open with an option.
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
    let ctx = ParseCtx::with_domain(Domain::Py);
    let written = written_after_blank(body_lines);
    let invocation = test_invocation(
        source.directive_name(),
        CODE_BLOCK_SPEC.shape,
        argument,
        &written,
        &mut diagnostics,
        &ctx,
    );
    let directive = parse_code_block(source, &invocation, &mut diagnostics);
    let Directive::CodeBlock(block) = directive else {
        panic!("Expected a CodeBlock, got {directive:?}");
    };
    (block, diagnostics)
}

/// Parses a `.. highlight::`, returning the directive and any diagnostics.
pub(super) fn highlight(argument: &str, body_lines: &[&str]) -> (Directive, Diagnostics) {
    let mut diagnostics = Diagnostics::default();
    let ctx = ParseCtx::with_domain(Domain::Py);
    let invocation = test_invocation(
        "highlight",
        HIGHLIGHT_SPEC.shape,
        argument,
        body_lines,
        &mut diagnostics,
        &ctx,
    );
    let directive = parse_highlight(&invocation, &mut diagnostics);
    (directive, diagnostics)
}

/// The codes of everything recorded, for asserting on a diagnostic set
/// without depending on message wording.
pub(super) fn codes(diagnostics: &Diagnostics) -> Vec<DiagnosticCode> {
    diagnostics.iter().map(|d| d.code).collect()
}
