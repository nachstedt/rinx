//! The plain-body parse shared by domain object types that have no
//! directive-specific options to strip off first — `c:function`/`c:macro`
//! and `std:cmdoption`.

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rusty_sphinx_ast::Node;

/// Strips the body's common leading indentation and parses the remaining
/// lines as block-level nodes.
pub(super) fn parse_body(
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    ctx: &ParseCtx<'_>,
) -> Vec<Node> {
    let unindented_lines = unindent_body_lines(body_lines);
    let body_content: Vec<&str> = unindented_lines.iter().map(String::as_str).collect();
    parse_blocks(&body_content, adornment_order, diagnostics, ctx)
}
