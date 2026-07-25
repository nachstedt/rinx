//! Structured, machine-readable domain-object warnings for the `render`
//! subcommand's optional `--warnings-output` sidecar.
//!
//! The renderer reports domain-object cross-reference problems as two
//! non-serializable diagnostic types — [`rusty_sphinx_renderer::BrokenLink`]
//! (only its [`rusty_sphinx_renderer::BrokenLinkKind::DomainObjectReference`]
//! variant is relevant here) and [`rusty_sphinx_renderer::ObjectTypeMismatch`].
//! This module folds those into a single JSON-serializable
//! [`DomainWarningReport`] so an external, developer-only tool
//! (`scripts/benchmark.py`) can diff them against a whitelist of accepted
//! warnings.
//!
//! The JSON wire format is deliberately owned here (the CLI/tooling boundary)
//! rather than in the renderer crate: the renderer's diagnostic types stay
//! plain in-memory values, and only the worker knows about the on-disk shape.

use rusty_sphinx_ast::ObjectType;
use rusty_sphinx_renderer::{BrokenLink, BrokenLinkKind, ObjectTypeMismatch};
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
/// Only [`BrokenLinkKind::DomainObjectReference`] broken links are kept — the
/// other broken-link kinds (`:ref:`, hyperlink, anonymous, `:term:`) are
/// deliberately excluded, as this sidecar is scoped to domain-object
/// references. Every [`ObjectTypeMismatch`] is included.
#[must_use]
pub fn build_domain_warning_report(
    doc_path: &str,
    broken_links: &[BrokenLink],
    mismatches: &[ObjectTypeMismatch],
) -> DomainWarningReport {
    let mut warnings = Vec::new();

    for link in broken_links {
        if let BrokenLinkKind::DomainObjectReference(requested_type) = link.kind {
            warnings.push(DomainWarning {
                kind: DomainWarningKind::DomainObjectReference,
                target: link.target.clone(),
                // The role's requested object type ("missed type") — nothing
                // resolved, so there is no resolved_type.
                requested_type: Some(requested_type),
                resolved_type: None,
            });
        }
    }

    for mismatch in mismatches {
        warnings.push(DomainWarning {
            kind: DomainWarningKind::ObjectTypeMismatch,
            target: mismatch.name.clone(),
            requested_type: Some(mismatch.requested_type),
            resolved_type: Some(mismatch.resolved_type),
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
    use rusty_sphinx_ast::PyObjectType;

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
            },
            BrokenLink {
                kind: BrokenLinkKind::Hyperlink,
                target: "some link".to_string(),
            },
            BrokenLink {
                kind: BrokenLinkKind::AnonymousReference,
                target: "anon".to_string(),
            },
            BrokenLink {
                kind: BrokenLinkKind::TermReference,
                target: "glossary term".to_string(),
            },
            BrokenLink {
                kind: BrokenLinkKind::DomainObjectReference(py(PyObjectType::Class)),
                target: "os.PathLike".to_string(),
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
    }

    #[test]
    fn test_build_report_carries_mismatch_types() {
        // Given a single object-type mismatch
        let mismatches = vec![ObjectTypeMismatch {
            name: "Fault".to_string(),
            requested_type: py(PyObjectType::Exception),
            resolved_type: py(PyObjectType::Class),
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
        }];
        let mismatches = vec![ObjectTypeMismatch {
            name: "Fault".to_string(),
            requested_type: py(PyObjectType::Exception),
            resolved_type: py(PyObjectType::Class),
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
}
