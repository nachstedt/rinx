//! Reporting the roles the inline scan refused, and lowering each to what it
//! is shown as instead.
//!
//! A whole-document pass for the reason substitution resolution is one: the
//! inline scan builds nodes and has nowhere to report, while every refusal —
//! a `:numref:` title Sphinx could not apply, an `:external:` prefix, a
//! registry role (`:pep:`, `:rfc:`, `:cve:`, `:cwe:`) or docutils'
//! `:pep-reference:`/`:rfc-reference:` naming nothing it can link, an
//! `:index:` entry its type cannot split — belongs
//! at the role the author wrote. The refusal travels on the node as an
//! [`InlineNode::RefusedRole`] until this pass reaches it.

use rinx_ast::{
    Diagnostic, DiagnosticCode, DocutilsPepNumber, DocutilsRfcNumber, IndexEntryType, InlineNode,
    Node, NumberFormat, NumberReferenceRefusal, Registry, RegistryTarget, RoleRefusal, Span,
    for_each_inline_list_mut,
};

use crate::diagnostics::Diagnostics;

/// Reports every refused role in `nodes` and replaces it, in place, with the
/// node its refusal says it is shown as.
pub(super) fn report_refused_roles(nodes: &mut [Node], diagnostics: &mut Diagnostics) {
    for_each_inline_list_mut(nodes, &mut |list| {
        for node in list.iter_mut() {
            if let InlineNode::RefusedRole {
                text,
                refusal,
                span,
            } = node
            {
                diagnostics.push(refusal_diagnostic(text, refusal, *span));
                *node = lowered(std::mem::take(text), refusal, *span);
            }
        }
    });
}

/// What a refused role is shown as: an unlinked reference for a `:numref:`,
/// an `:index:`'s text with no entries — Sphinx's warning leaves the text in
/// place — and the source text for every other role, as its `problematic`
/// node shows it.
fn lowered(text: String, refusal: &RoleRefusal, span: Option<Span>) -> InlineNode {
    match refusal {
        RoleRefusal::NumberReference(_) => InlineNode::NumberReference {
            title: None,
            target: text,
            link: false,
            span,
        },
        RoleRefusal::RegistryTarget { .. }
        | RoleRefusal::DocutilsPepNumber { .. }
        | RoleRefusal::DocutilsRfcNumber { .. } => InlineNode::Text(text),
        RoleRefusal::IndexEntry { title, .. } => InlineNode::IndexReference {
            title: title.clone(),
            entries: Vec::new(),
            index_id: String::new(),
            span,
        },
    }
}

/// The diagnostic one refusal is reported as.
fn refusal_diagnostic(text: &str, refusal: &RoleRefusal, span: Option<Span>) -> Diagnostic {
    match refusal {
        RoleRefusal::NumberReference(refusal) => number_reference_diagnostic(text, *refusal, span),
        RoleRefusal::RegistryTarget { registry, target } => Diagnostic::at(
            invalid_target_code(*registry),
            // As for a `:numref:` title, the reason is re-derived from the
            // text rather than stored beside it.
            RegistryTarget::parse(*registry, target).err().map_or_else(
                || format!("invalid {} number '{target}'", registry.label()),
                |error| format!(":{}: {error}", registry.role_name()),
            ),
            span,
        ),
        RoleRefusal::DocutilsPepNumber { target } => Diagnostic::at(
            DiagnosticCode::PepReferenceInvalidNumber,
            DocutilsPepNumber::parse(target).err().map_or_else(
                || format!("invalid PEP number '{target}'"),
                |error| format!(":pep-reference: {error}"),
            ),
            span,
        ),
        RoleRefusal::DocutilsRfcNumber { target } => Diagnostic::at(
            DiagnosticCode::RfcReferenceInvalidNumber,
            DocutilsRfcNumber::parse(target).err().map_or_else(
                || format!("invalid RFC number '{target}'"),
                |error| format!(":rfc-reference: {error}"),
            ),
            span,
        ),
        RoleRefusal::IndexEntry { entry, .. } => Diagnostic::at(
            index_role_code(entry.entry_type),
            format!(":index: {entry}"),
            span,
        ),
    }
}

/// The code an `:index:` role's malformed entry is reported as: the role's
/// own family, not the `.. index::` directive's, since a code names the
/// construct the author wrote rather than the grammar both share.
const fn index_role_code(entry_type: IndexEntryType) -> DiagnosticCode {
    match entry_type {
        IndexEntryType::Single => DiagnosticCode::IndexRoleInvalidSingle,
        IndexEntryType::Pair => DiagnosticCode::IndexRoleInvalidPair,
        IndexEntryType::Triple => DiagnosticCode::IndexRoleInvalidTriple,
        IndexEntryType::See => DiagnosticCode::IndexRoleInvalidSee,
        IndexEntryType::SeeAlso => DiagnosticCode::IndexRoleInvalidSeeAlso,
    }
}

/// The code a registry role's refused target is reported as — one per
/// registry, since a code names the construct and each has its own rule.
const fn invalid_target_code(registry: Registry) -> DiagnosticCode {
    match registry {
        Registry::Pep => DiagnosticCode::PepInvalidNumber,
        Registry::Rfc => DiagnosticCode::RfcInvalidNumber,
        Registry::Cve => DiagnosticCode::CveInvalidId,
        Registry::Cwe => DiagnosticCode::CweInvalidNumber,
    }
}

/// The diagnostic a refused `:numref:` is reported as.
fn number_reference_diagnostic(
    text: &str,
    refusal: NumberReferenceRefusal,
    span: Option<Span>,
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
    use rinx_ast::Position;

    fn refused(text: &str, refusal: NumberReferenceRefusal, span: Option<Span>) -> InlineNode {
        InlineNode::RefusedRole {
            text: text.to_string(),
            refusal: RoleRefusal::NumberReference(refusal),
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
        report_refused_roles(&mut nodes, &mut diagnostics);

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
        report_refused_roles(&mut nodes, &mut diagnostics);

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
        report_refused_roles(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(nodes, vec![Node::Paragraph(vec![reference])]);
        assert!(diagnostics.into_parts().0.is_empty());
    }

    #[test]
    fn test_refusal_diagnostic_explains_an_invalid_title() {
        // Given / When
        let diagnostic =
            number_reference_diagnostic("a%sb%s", NumberReferenceRefusal::InvalidTitle, None);

        // Then
        assert!(
            diagnostic.message.contains("more than one %s"),
            "{}",
            diagnostic.message
        );
    }

    #[test]
    fn test_reports_a_refused_pep_and_shows_its_source() {
        // Given
        let at = Span::new(Position::new(2, 1), Position::new(2, 12));
        let mut nodes = vec![Node::Paragraph(vec![InlineNode::RefusedRole {
            text: ":pep:`abc`".to_string(),
            refusal: RoleRefusal::RegistryTarget {
                registry: Registry::Pep,
                target: "abc".to_string(),
            },
            span: Some(at),
        }])];
        let mut diagnostics = Diagnostics::default();

        // When
        report_refused_roles(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes,
            vec![Node::Paragraph(vec![InlineNode::Text(
                ":pep:`abc`".to_string()
            )])]
        );
        let (entries, _, _) = diagnostics.into_parts();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, DiagnosticCode::PepInvalidNumber);
        assert_eq!(entries[0].span, Some(at));
        assert!(
            entries[0].message.contains("invalid PEP number 'abc'"),
            "{}",
            entries[0].message
        );
    }

    #[test]
    fn test_reports_a_refused_pep_reference_and_shows_its_source() {
        // Given
        let at = Span::new(Position::new(1, 1), Position::new(1, 23));
        let mut nodes = vec![Node::Paragraph(vec![InlineNode::RefusedRole {
            text: ":pep-reference:`10000`".to_string(),
            refusal: RoleRefusal::DocutilsPepNumber {
                target: "10000".to_string(),
            },
            span: Some(at),
        }])];
        let mut diagnostics = Diagnostics::default();

        // When
        report_refused_roles(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes,
            vec![Node::Paragraph(vec![InlineNode::Text(
                ":pep-reference:`10000`".to_string()
            )])]
        );
        let (entries, _, _) = diagnostics.into_parts();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, DiagnosticCode::PepReferenceInvalidNumber);
        assert_eq!(entries[0].span, Some(at));
        assert_eq!(
            entries[0].message,
            ":pep-reference: PEP number must be a number from 0 to 9999; \"10000\" is invalid"
        );
    }

    #[test]
    fn test_reports_a_refused_cve_with_its_own_code() {
        // Given
        let mut nodes = vec![Node::Paragraph(vec![InlineNode::RefusedRole {
            text: ":cve:`CVE-2024-3094`".to_string(),
            refusal: RoleRefusal::RegistryTarget {
                registry: Registry::Cve,
                target: "CVE-2024-3094".to_string(),
            },
            span: None,
        }])];
        let mut diagnostics = Diagnostics::default();

        // When
        report_refused_roles(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes,
            vec![Node::Paragraph(vec![InlineNode::Text(
                ":cve:`CVE-2024-3094`".to_string()
            )])]
        );
        let (entries, _, _) = diagnostics.into_parts();
        assert_eq!(entries[0].code, DiagnosticCode::CveInvalidId);
        assert!(
            entries[0]
                .message
                .starts_with(":cve: invalid CVE number 'CVE-2024-3094': drop the 'CVE-' prefix"),
            "{}",
            entries[0].message
        );
    }

    #[test]
    fn test_reports_a_refused_rfc_reference_in_docutils_wording() {
        // Given
        let mut nodes = vec![Node::Paragraph(vec![InlineNode::RefusedRole {
            text: ":rfc-reference:`0`".to_string(),
            refusal: RoleRefusal::DocutilsRfcNumber {
                target: "0".to_string(),
            },
            span: None,
        }])];
        let mut diagnostics = Diagnostics::default();

        // When
        report_refused_roles(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes,
            vec![Node::Paragraph(vec![InlineNode::Text(
                ":rfc-reference:`0`".to_string()
            )])]
        );
        let (entries, _, _) = diagnostics.into_parts();
        assert_eq!(entries[0].code, DiagnosticCode::RfcReferenceInvalidNumber);
        assert_eq!(
            entries[0].message,
            ":rfc-reference: RFC number must be a number greater than or equal to 1; \"0\" is invalid"
        );
    }

    #[test]
    fn test_invalid_target_code_names_each_registry() {
        // Given / When / Then
        assert_eq!(
            invalid_target_code(Registry::Pep),
            DiagnosticCode::PepInvalidNumber
        );
        assert_eq!(
            invalid_target_code(Registry::Rfc),
            DiagnosticCode::RfcInvalidNumber
        );
        assert_eq!(
            invalid_target_code(Registry::Cve),
            DiagnosticCode::CveInvalidId
        );
        assert_eq!(
            invalid_target_code(Registry::Cwe),
            DiagnosticCode::CweInvalidNumber
        );
    }

    #[test]
    fn test_lowered_keeps_a_refused_numref_a_reference() {
        // Given / When
        let node = lowered(
            "fig".to_string(),
            &RoleRefusal::NumberReference(NumberReferenceRefusal::External),
            None,
        );

        // Then
        assert_eq!(
            node,
            InlineNode::NumberReference {
                title: None,
                target: "fig".to_string(),
                link: false,
                span: None,
            }
        );
    }

    #[test]
    fn test_reports_a_malformed_index_entry_and_keeps_the_title() {
        // Given
        let at = Span::new(Position::new(1, 1), Position::new(1, 28));
        let mut nodes = vec![Node::Paragraph(vec![InlineNode::RefusedRole {
            text: ":index:`loops <pair: loop>`".to_string(),
            refusal: RoleRefusal::IndexEntry {
                title: "loops".to_string(),
                entry: rinx_ast::InvalidIndexEntry {
                    entry_type: IndexEntryType::Pair,
                    value: "loop".to_string(),
                },
            },
            span: Some(at),
        }])];
        let mut diagnostics = Diagnostics::default();

        // When
        report_refused_roles(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes[0],
            Node::Paragraph(vec![InlineNode::IndexReference {
                title: "loops".to_string(),
                entries: Vec::new(),
                index_id: String::new(),
                span: Some(at),
            }])
        );
        let (entries, _, _) = diagnostics.into_parts();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, DiagnosticCode::IndexRoleInvalidPair);
        assert_eq!(
            entries[0].message,
            ":index: invalid pair index entry 'loop'"
        );
        assert_eq!(entries[0].span, Some(at));
    }

    #[test]
    fn test_index_role_code_names_the_role_family_for_every_type() {
        // Given / When / Then
        for entry_type in IndexEntryType::ALL {
            assert!(
                index_role_code(entry_type)
                    .as_str()
                    .starts_with("index-role."),
                "{entry_type:?}"
            );
        }
    }
}
