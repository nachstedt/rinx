//! What every directive is interpreted from, so that one signature serves
//! them all and the registry can pair each spec with its interpreter.
//!
//! Still the reStructuredText view of a directive — its marker line and its
//! raw, indented body — because each directive splits its own block into
//! argument, options and content. Each `interpret_*` reads the part its
//! parser needs: a directive whose content may begin on the marker line the
//! [`DirectiveInput::marker`] and [`DirectiveInput::body`], a splicing one the
//! [`DirectiveInput::splice_ctx`], every other one the body lines under the
//! [`DirectiveInput::body_ctx`].

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use rinx_ast::{Directive, Node, Span};

use super::body::{DirectiveBody, MarkerLine};
use super::error_node::unknown_directive;

/// Everything a directive's marker line and body say.
pub(in crate::directives) struct DirectiveInput<'a, 'c> {
    /// The name, as written before the `::`.
    pub(in crate::directives) name: &'a str,
    /// What follows the `::`, trimmed.
    pub(in crate::directives) argument: &'a str,
    /// The whole marker line, for a diagnostic about the directive itself.
    pub(in crate::directives) span: Option<Span>,
    /// The marker line, for a directive whose content may begin on it.
    pub(in crate::directives) marker: MarkerLine<'a>,
    /// The body below the marker, as collected.
    pub(in crate::directives) body: &'a DirectiveBody<'a>,
    /// The body's lines, its indentation intact.
    pub(in crate::directives) body_lines: &'a [&'a str],
    /// The context the marker line was read in.
    pub(in crate::directives) ctx: &'a ParseCtx<'c>,
    /// The context positioned at the body's first line and column, taking no
    /// section titles — docutils' default for a nested parse.
    pub(in crate::directives) body_ctx: &'a ParseCtx<'c>,
}

impl<'c> DirectiveInput<'_, 'c> {
    /// The body context of a directive that splices its nodes into the
    /// enclosing block: transparent, so its titles are allowed exactly where
    /// the directive itself stands allows them.
    pub(in crate::directives) fn splice_ctx(&self) -> ParseCtx<'c> {
        self.body_ctx.with_section_titles(self.ctx.section_titles())
    }
}

/// The document-wide state every directive's parse may add to.
pub(in crate::directives) struct ParseState<'s> {
    /// The section adornments seen so far, in order.
    pub(in crate::directives) adornment_order: &'s mut Vec<Adornment>,
    /// The diagnostics found so far.
    pub(in crate::directives) diagnostics: &'s mut Diagnostics,
}

/// The nodes of a directive yielding exactly one.
pub(in crate::directives) fn single(directive: Directive) -> Vec<Node> {
    vec![Node::Directive(directive)]
}

/// The nodes of a directive whose name is no spelling its parser reads back:
/// reported and kept as [`Directive::Unknown`], as a name without a table entry
/// is. The directive table's own tests rule this out.
pub(in crate::directives) fn unknown(
    input: &DirectiveInput<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<Node> {
    single(unknown_directive(
        input.name.to_string(),
        input.argument.to_string(),
        input.body_lines,
        input.span,
        state.diagnostics,
    ))
}

/// A test's input for the one-line directive `lines[0]`, read under `ctx` with
/// its body under `body_ctx`.
#[cfg(test)]
pub(in crate::directives) fn test_input<'a, 'c>(
    name: &'a str,
    lines: &'a [&'a str],
    body: &'a DirectiveBody<'a>,
    (ctx, body_ctx): (&'a ParseCtx<'c>, &'a ParseCtx<'c>),
) -> DirectiveInput<'a, 'c> {
    DirectiveInput {
        name,
        argument: "",
        span: None,
        marker: MarkerLine {
            index: 0,
            span: None,
            argument: super::body::MarkerText::after_marker(lines[0]),
        },
        body,
        body_lines: &[],
        ctx,
        body_ctx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directives::body::collect_directive_body;
    use rinx_ast::Domain;

    #[test]
    fn test_single_wraps_one_directive_as_its_only_node() {
        // Given / When
        let nodes = single(Directive::CNamespacePop);

        // Then
        assert_eq!(nodes, vec![Node::Directive(Directive::CNamespacePop)]);
    }

    #[test]
    fn test_unknown_keeps_the_directive_and_reports_it() {
        // Given
        let lines = [".. note::"];
        let body = collect_directive_body(&lines, 1, 0);
        let ctx = ParseCtx::with_domain(Domain::Py);
        let body_ctx = ctx.without_section_titles();
        let input = test_input("note", &lines, &body, (&ctx, &body_ctx));
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let nodes = unknown(
            &input,
            &mut ParseState {
                adornment_order: &mut adornment_order,
                diagnostics: &mut diagnostics,
            },
        );

        // Then
        assert!(matches!(
            nodes[..],
            [Node::Directive(Directive::Unknown { .. })]
        ));
        let (found, _, _) = diagnostics.into_parts();
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn test_splice_ctx_takes_section_titles_where_the_directive_stands() {
        // Given — a marker read where titles are allowed, under a body context
        // that forbids them
        let lines = [".. include:: x.rst"];
        let body = collect_directive_body(&lines, 1, 0);
        let ctx = ParseCtx::with_domain(Domain::Py);
        let body_ctx = ctx.without_section_titles();
        let input = test_input("include", &lines, &body, (&ctx, &body_ctx));

        // When
        let splice_ctx = input.splice_ctx();

        // Then
        assert_eq!(splice_ctx.section_titles(), ctx.section_titles());
        assert_ne!(input.body_ctx.section_titles(), ctx.section_titles());
    }
}
