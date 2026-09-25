//! Structured, machine-readable domain-object warnings for the `render`
//! subcommand's optional `--warnings-output` sidecar.
//!
//! The renderer reports domain-object cross-reference problems as two
//! non-serializable diagnostic types — [`rinx_renderer::BrokenLink`]
//! (only its [`rinx_renderer::BrokenLinkKind::DomainObjectReference`]
//! variant is relevant here) and [`rinx_renderer::ObjectTypeMismatch`].
//! This module folds those into a single JSON-serializable
//! [`DomainWarningReport`] so an external, developer-only tool
//! (`scripts/benchmark.py`) can diff them against a whitelist of accepted
//! warnings.
//!
//! The JSON wire format is deliberately owned here (the CLI/tooling boundary)
//! rather than in the renderer crate: the renderer's diagnostic types stay
//! plain in-memory values, and only the worker knows about the on-disk shape.

use rinx_ast::{ObjectType, Span};
use rinx_renderer::{BrokenLink, BrokenLinkKind, ObjectTypeMismatch};
use serde::{Deserialize, Serialize};

/// The kind of domain-object problem a [`DomainWarning`] records.
///
/// Serializes in `snake_case` (`"domain_object_reference"`,
/// `"object_type_mismatch"`) — the `kind` field the whitelist matches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainWarningKind {
    /// A domain-object role (`:func:`, `:py:class:`, `:c:type:`, …) that
    /// failed to resolve against the project index at all.
    DomainObjectReference,
    /// A domain-object role that resolved only via an object-type alias
    /// fallback — the definition's own type differs from the one the role
    /// asked for.
    ObjectTypeMismatch,
    /// A dot-prefixed domain-object role whose suffix search matched several
    /// objects. Reported separately from
    /// [`Self::DomainObjectReference`] because the fix differs: the object
    /// exists, the target just has to say which one.
    AmbiguousDomainObjectReference,
}

/// A single domain-object warning, flattened into a serializable shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainWarning {
    pub kind: DomainWarningKind,
    /// For a reference: the unresolved target. For a mismatch: the qualified
    /// name the reference resolved against.
    pub target: String,
    /// The object type the role asked for. Present only for mismatches.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub requested_type: Option<ObjectType>,
    /// The object type the definition actually has. Present only for
    /// mismatches.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub resolved_type: Option<ObjectType>,
    /// The qualified names an ambiguous reference matched. Present only for
    /// ambiguities, where naming the options *is* the actionable part of the
    /// warning.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub candidates: Option<Vec<String>>,
    /// Where the offending role was written. Absent for a reference the parser
    /// could not place — content generated after the parse, such as a
    /// `.. csv-table::` cell.
    ///
    /// The whole range is recorded, not just its start: the terminal prints a
    /// position, but a tool reading this file may want to highlight the role.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub span: Option<Span>,
}

/// All domain-object warnings emitted while rendering one document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainWarningReport {
    pub doc_path: String,
    pub warnings: Vec<DomainWarning>,
}

/// Builds the per-document domain-object warning report from a render's raw
/// diagnostics.
///
/// Only the two domain-object broken-link kinds are kept — the others
/// (`:ref:`, hyperlink, anonymous, `:term:`) are deliberately excluded, as
/// this sidecar is scoped to domain-object references. Every
/// [`ObjectTypeMismatch`] is included.
#[must_use]
pub fn build_domain_warning_report(
    doc_path: &str,
    broken_links: &[BrokenLink],
    mismatches: &[ObjectTypeMismatch],
) -> DomainWarningReport {
    let mut warnings = Vec::new();

    for link in broken_links {
        match &link.kind {
            BrokenLinkKind::DomainObjectReference(requested_type) => warnings.push(DomainWarning {
                kind: DomainWarningKind::DomainObjectReference,
                target: link.target.clone(),
                // The role's requested object type ("missed type") — nothing
                // resolved, so there is no resolved_type.
                requested_type: Some(*requested_type),
                resolved_type: None,
                candidates: None,
                span: link.span,
            }),
            BrokenLinkKind::AmbiguousDomainObjectReference {
                object_type,
                candidates,
            } => warnings.push(DomainWarning {
                kind: DomainWarningKind::AmbiguousDomainObjectReference,
                target: link.target.clone(),
                requested_type: Some(*object_type),
                resolved_type: None,
                candidates: Some(candidates.clone()),
                span: link.span,
            }),
            _ => {}
        }
    }

    for mismatch in mismatches {
        warnings.push(DomainWarning {
            kind: DomainWarningKind::ObjectTypeMismatch,
            target: mismatch.name.clone(),
            requested_type: Some(mismatch.requested_type),
            resolved_type: Some(mismatch.resolved_type),
            candidates: None,
            span: mismatch.span,
        });
    }

    DomainWarningReport {
        doc_path: doc_path.to_string(),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::PyObjectType;

    fn py(t: PyObjectType) -> ObjectType {
        ObjectType::Py(t)
    }

    #[test]
    fn test_build_report_filters_out_non_domain_object_broken_links() {
        // Given a mix of broken-link kinds
        let broken_links = vec![
            BrokenLink {
                kind: BrokenLinkKind::Reference,
                target: "some-ref".to_string(),
                span: None,
            },
            BrokenLink {
                kind: BrokenLinkKind::Hyperlink,
                target: "some link".to_string(),
                span: None,
            },
            BrokenLink {
                kind: BrokenLinkKind::AnonymousReference,
                target: "anon".to_string(),
                span: None,
            },
            BrokenLink {
                kind: BrokenLinkKind::TermReference,
                target: "glossary term".to_string(),
                span: None,
            },
            BrokenLink {
                kind: BrokenLinkKind::DomainObjectReference(py(PyObjectType::Class)),
                target: "os.PathLike".to_string(),
                span: None,
            },
        ];

        // When
        let report = build_domain_warning_report("Doc/library/os", &broken_links, &[]);

        // Then only the domain-object reference survives, carrying its
        // requested ("missed") type but no resolved type
        assert_eq!(report.doc_path, "Doc/library/os");
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(
            report.warnings[0].kind,
            DomainWarningKind::DomainObjectReference
        );
        assert_eq!(report.warnings[0].target, "os.PathLike");
        assert_eq!(
            report.warnings[0].requested_type,
            Some(py(PyObjectType::Class))
        );
        assert!(report.warnings[0].resolved_type.is_none());
        assert!(report.warnings[0].candidates.is_none());
    }

    #[test]
    fn test_build_report_records_the_candidates_of_an_ambiguous_reference() {
        // Given a dot-prefixed reference that matched several objects
        let broken_links = vec![BrokenLink {
            kind: BrokenLinkKind::AmbiguousDomainObjectReference {
                object_type: py(PyObjectType::Method),
                candidates: vec![
                    "tarfile.tarfile.close".to_string(),
                    "zipfile.zipfile.close".to_string(),
                ],
            },
            target: "close".to_string(),
            span: None,
        }];

        // When
        let report = build_domain_warning_report("Doc/library/shutil", &broken_links, &[]);

        // Then the candidates are carried through, so the whitelist diff can
        // show what has to be disambiguated between
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(
            report.warnings[0].kind,
            DomainWarningKind::AmbiguousDomainObjectReference
        );
        assert_eq!(report.warnings[0].target, "close");
        assert_eq!(
            report.warnings[0].requested_type,
            Some(py(PyObjectType::Method))
        );
        assert_eq!(
            report.warnings[0].candidates,
            Some(vec![
                "tarfile.tarfile.close".to_string(),
                "zipfile.zipfile.close".to_string()
            ])
        );
    }

    #[test]
    fn test_ambiguous_warning_serializes_with_its_own_kind_and_candidates() {
        // Given
        let warning = DomainWarning {
            kind: DomainWarningKind::AmbiguousDomainObjectReference,
            target: "close".to_string(),
            requested_type: Some(py(PyObjectType::Method)),
            resolved_type: None,
            candidates: Some(vec!["tarfile.tarfile.close".to_string()]),
            span: None,
        };

        // When
        let json = serde_json::to_string(&warning).expect("Failed to serialize");

        // Then — the `kind` string is what the whitelist matches on
        assert!(json.contains(r#""kind":"ambiguous_domain_object_reference""#));
        assert!(json.contains(r#""candidates":["tarfile.tarfile.close"]"#));
        assert!(!json.contains("resolved_type"));
    }

    #[test]
    fn test_build_report_carries_mismatch_types() {
        // Given a single object-type mismatch
        let mismatches = vec![ObjectTypeMismatch {
            name: "Fault".to_string(),
            requested_type: py(PyObjectType::Exception),
            resolved_type: py(PyObjectType::Class),
            span: None,
        }];

        // When
        let report = build_domain_warning_report("Doc/library/xmlrpc.client", &[], &mismatches);

        // Then
        assert_eq!(report.warnings.len(), 1);
        let warning = &report.warnings[0];
        assert_eq!(warning.kind, DomainWarningKind::ObjectTypeMismatch);
        assert_eq!(warning.target, "Fault");
        assert_eq!(warning.requested_type, Some(py(PyObjectType::Exception)));
        assert_eq!(warning.resolved_type, Some(py(PyObjectType::Class)));
    }

    #[test]
    fn test_build_report_with_no_diagnostics_yields_empty_warnings() {
        // Given / When
        let report = build_domain_warning_report("Doc/index", &[], &[]);

        // Then
        assert!(report.warnings.is_empty());
        assert_eq!(report.doc_path, "Doc/index");
    }

    #[test]
    fn test_mismatch_serializes_types_as_flat_domain_qualified_strings() {
        // Given a report containing one mismatch
        let mismatches = vec![ObjectTypeMismatch {
            name: "Fault".to_string(),
            requested_type: py(PyObjectType::Exception),
            resolved_type: py(PyObjectType::Class),
            span: None,
        }];
        let report = build_domain_warning_report("Doc/library/xmlrpc.client", &[], &mismatches);

        // When
        let json = serde_json::to_value(&report).expect("serialize");

        // Then — pins the exact field names/shape the Python side depends on
        let warning = &json["warnings"][0];
        assert_eq!(warning["kind"], "object_type_mismatch");
        assert_eq!(warning["target"], "Fault");
        assert_eq!(warning["requested_type"], "py:exception");
        assert_eq!(warning["resolved_type"], "py:class");
    }

    #[test]
    fn test_reference_serializes_requested_type_but_omits_resolved() {
        // Given a report containing one broken domain-object reference
        let broken_links = vec![BrokenLink {
            kind: BrokenLinkKind::DomainObjectReference(py(PyObjectType::Function)),
            target: "os.PathLike".to_string(),
            span: None,
        }];
        let report = build_domain_warning_report("Doc/library/os", &broken_links, &[]);

        // When
        let json = serde_json::to_value(&report).expect("serialize");

        // Then — the requested ("missed") type is present; resolved is absent
        // (nothing resolved), not null
        let warning = &json["warnings"][0];
        assert_eq!(warning["kind"], "domain_object_reference");
        assert_eq!(warning["requested_type"], "py:function");
        assert!(warning.get("resolved_type").is_none());
    }

    #[test]
    fn test_report_round_trips_through_json() {
        // Given a report with both kinds of warning
        let broken_links = vec![BrokenLink {
            kind: BrokenLinkKind::DomainObjectReference(py(PyObjectType::Function)),
            target: "os.PathLike".to_string(),
            span: None,
        }];
        let mismatches = vec![ObjectTypeMismatch {
            name: "Fault".to_string(),
            requested_type: py(PyObjectType::Exception),
            resolved_type: py(PyObjectType::Class),
            span: None,
        }];
        let report = build_domain_warning_report("Doc/library/os", &broken_links, &mismatches);

        // When
        let json = serde_json::to_string(&report).expect("serialize");
        let restored: DomainWarningReport = serde_json::from_str(&json).expect("deserialize");

        // Then
        assert_eq!(report, restored);
    }

    #[test]
    fn test_domain_warning_kind_serializes_snake_case() {
        // Given / When / Then
        assert_eq!(
            serde_json::to_string(&DomainWarningKind::DomainObjectReference).unwrap(),
            "\"domain_object_reference\""
        );
        assert_eq!(
            serde_json::to_string(&DomainWarningKind::ObjectTypeMismatch).unwrap(),
            "\"object_type_mismatch\""
        );
    }

    #[test]
    fn test_a_warning_carries_the_span_of_the_role_that_produced_it() {
        // Given a broken domain-object reference the parser could place
        let span = Span::new(
            rinx_ast::Position::new(42, 18),
            rinx_ast::Position::new(42, 35),
        );
        let links = [BrokenLink {
            kind: BrokenLinkKind::DomainObjectReference(py(PyObjectType::Function)),
            target: "missing".to_string(),
            span: Some(span),
        }];

        // When
        let report = build_domain_warning_report("guide.rst", &links, &[]);

        // Then — the sidecar records the whole range, so a tool reading it can
        // highlight the role rather than just jump to a line
        assert_eq!(report.warnings[0].span, Some(span));
    }

    #[test]
    fn test_a_warning_without_a_position_omits_the_span_entirely() {
        // Given a reference from generated content
        let links = [BrokenLink {
            kind: BrokenLinkKind::DomainObjectReference(py(PyObjectType::Function)),
            target: "missing".to_string(),
            span: None,
        }];

        // When
        let report = build_domain_warning_report("guide.rst", &links, &[]);
        let json = serde_json::to_string(&report.warnings[0]).expect("Failed to serialize");

        // Then — absent rather than null, matching the other optional fields
        assert!(!json.contains("span"), "{json}");
    }
}
