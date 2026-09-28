//! Reporting the `:numref:` roles the inline scan refused, and lowering each
//! to the unlinked reference it is shown as.
//!
//! A whole-document pass for the reason substitution resolution is one: the
//! inline scan builds nodes and has nowhere to report, while every refusal —
//! a title Sphinx could not apply, an `:external:` prefix — belongs at the
//! role the author wrote. The refusal travels on the node as an
//! [`InlineNode::RefusedNumberReference`] until this pass reaches it.

use rinx_ast::{
    Diagnostic, DiagnosticCode, InlineNode, Node, NumberFormat, NumberReferenceRefusal,
};

use crate::diagnostics::Diagnostics;

use super::inline_lists::for_each_inline_list_mut;

/// Reports every refused `:numref:` in `nodes` and replaces it, in place, with
/// an unlinked [`InlineNode::NumberReference`] showing its text.
pub(super) fn report_refused_number_references(nodes: &mut [Node], diagnostics: &mut Diagnostics) {
    for_each_inline_list_mut(nodes, &mut |list| {
        for node in list.iter_mut() {
            if let InlineNode::RefusedNumberReference {
                text,
                refusal,
                span,
            } = node
            {
                diagnostics.push(refusal_diagnostic(text, *refusal, *span));
                *node = InlineNode::NumberReference {
                    title: None,
                    target: std::mem::take(text),
                    link: false,
                    span: *span,
                };
            }
        }
    });
}

/// The diagnostic one refusal is reported as.
fn refusal_diagnostic(
    text: &str,
    refusal: NumberReferenceRefusal,
    span: Option<rinx_ast::Span>,
) -> Diagnostic {
    match refusal {
        NumberReferenceRefusal::InvalidTitle => Diagnostic::at(
            DiagnosticCode::NumrefInvalidFormat,
            // The inline scan kept only the text, so the reason is re-derived
            // from it; parsing is cheap, and a second copy of the error in the
            // AST would be one more thing to keep in step.
            NumberFormat::parse(text).err().map_or_else(
                || format!("':numref:' title '{text}' cannot be applied"),
                |error| format!(":numref: title {error}"),
            ),
            span,
        ),
        NumberReferenceRefusal::External => Diagnostic::at(
            DiagnosticCode::NumrefExternal,
            format!(
                ":numref:`{text}` cannot resolve through an inventory, which holds no numbers; \
                 drop the ':external:' prefix"
            ),
            span,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Position, Span};

    fn refused(text: &str, refusal: NumberReferenceRefusal, span: Option<Span>) -> InlineNode {
        InlineNode::RefusedNumberReference {
            text: text.to_string(),
            refusal,
            span,
        }
    }

    #[test]
    fn test_reports_a_refused_title_and_shows_it_unlinked() {
        // Given a refused title inside a nested paragraph
        let at = Span::new(Position::new(3, 5), Position::new(3, 30));
        let mut nodes = vec![Node::BlockQuote {
            content: vec![Node::Paragraph(vec![refused(
                "see this",
                NumberReferenceRefusal::InvalidTitle,
                Some(at),
            )])],
            attribution: None,
        }];
        let mut diagnostics = Diagnostics::default();

        // When
        report_refused_number_references(&mut nodes, &mut diagnostics);

        // Then
        let Node::BlockQuote { content, .. } = &nodes[0] else {
            unreachable!()
        };
        assert_eq!(
            content[0],
            Node::Paragraph(vec![InlineNode::NumberReference {
                title: None,
                target: "see this".to_string(),
                link: false,
                span: Some(at),
            }])
        );
        let (entries, _, _) = diagnostics.into_parts();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, DiagnosticCode::NumrefInvalidFormat);
        assert_eq!(entries[0].span, Some(at));
        assert!(
            entries[0].message.contains("'see this'"),
            "{}",
            entries[0].message
        );
    }

    #[test]
    fn test_reports_an_external_prefix() {
        // Given
        let mut nodes = vec![Node::Paragraph(vec![refused(
            "fig-root",
            NumberReferenceRefusal::External,
            None,
        )])];
        let mut diagnostics = Diagnostics::default();

        // When
        report_refused_number_references(&mut nodes, &mut diagnostics);

        // Then
        let (entries, _, _) = diagnostics.into_parts();
        assert_eq!(entries[0].code, DiagnosticCode::NumrefExternal);
        assert!(
            entries[0].message.contains("fig-root"),
            "{}",
            entries[0].message
        );
    }

    #[test]
    fn test_leaves_an_accepted_reference_alone() {
        // Given
        let reference = InlineNode::NumberReference {
            title: None,
            target: "fig".to_string(),
            link: true,
            span: None,
        };
        let mut nodes = vec![Node::Paragraph(vec![reference.clone()])];
        let mut diagnostics = Diagnostics::default();

        // When
        report_refused_number_references(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(nodes, vec![Node::Paragraph(vec![reference])]);
        assert!(diagnostics.into_parts().0.is_empty());
    }

    #[test]
    fn test_refusal_diagnostic_explains_an_invalid_title() {
        // Given / When
        let diagnostic = refusal_diagnostic("a%sb%s", NumberReferenceRefusal::InvalidTitle, None);

        // Then
        assert!(
            diagnostic.message.contains("more than one %s"),
            "{}",
            diagnostic.message
        );
    }
}
