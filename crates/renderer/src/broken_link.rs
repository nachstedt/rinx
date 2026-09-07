//! Diagnostics a render produces alongside its HTML: cross-references that
//! failed to resolve against the [`ProjectIndex`](rusty_sphinx_index::ProjectIndex),
//! and references that resolved only through an object-type fallback.

use rusty_sphinx_ast::{DiagnosticCode, ObjectType, Span};

/// The kind of cross-reference role that produced a [`BrokenLink`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokenLinkKind {
    /// A `:ref:` role (`InlineNode::Reference`).
    Reference,
    /// A named hyperlink (`InlineNode::Hyperlink`).
    Hyperlink,
    /// An anonymous `__` reference with no matching anonymous target left.
    AnonymousReference,
    /// A `:term:` role (`InlineNode::TermReference`).
    TermReference,
    /// A `:option:` role (`InlineNode::OptionReference`).
    OptionReference,
    /// An `:eq:` role (`InlineNode::EquationReference`).
    EquationReference,
    /// A domain-object role (`:func:`, `:py:func:`, etc.). Carries the object
    /// type the role asked for (e.g. `py:function`) — the "missed type", known
    /// at the point resolution failed and worth surfacing in diagnostics even
    /// though nothing resolved.
    DomainObjectReference(ObjectType),
    /// An entity role (`:req:`, `:need:`, `:entity:`) naming no entity.
    /// Carries the role as written, since a schema names its own roles and the
    /// diagnostic should quote what the author typed.
    EntityReference(String),
    /// An entity role that resolved, but to an entity of a type the role does
    /// not accept. Links anyway, as a domain-object type mismatch does — the
    /// target exists, and refusing the link would help nobody.
    EntityTypeMismatch {
        role: String,
        /// The type the entity actually has.
        found_type: String,
    },
    /// A dot-prefixed domain-object role whose suffix search matched several
    /// objects, so the target names no single one. Deliberately unresolved
    /// rather than linked to an arbitrary candidate (real Sphinx links the
    /// first) — the candidates are carried here so the author is told what to
    /// disambiguate between.
    AmbiguousDomainObjectReference {
        object_type: ObjectType,
        /// The qualified names that matched, in index order.
        candidates: Vec<String>,
    },
}

impl BrokenLinkKind {
    /// The diagnostic code this kind reports under — what a `.. noqa:`
    /// comment names to suppress it.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::Reference => DiagnosticCode::LinkBrokenRef,
            Self::Hyperlink => DiagnosticCode::LinkBrokenHyperlink,
            Self::AnonymousReference => DiagnosticCode::LinkBrokenAnonymous,
            Self::TermReference => DiagnosticCode::LinkBrokenTerm,
            Self::OptionReference => DiagnosticCode::LinkBrokenOption,
            Self::EquationReference => DiagnosticCode::LinkBrokenEquation,
            Self::EntityReference(_) => DiagnosticCode::EntityUnknownTarget,
            Self::EntityTypeMismatch { .. } => DiagnosticCode::EntityRoleTypeMismatch,
            Self::DomainObjectReference(_) => DiagnosticCode::LinkBrokenObject,
            Self::AmbiguousDomainObjectReference { .. } => DiagnosticCode::LinkAmbiguousObject,
        }
    }

    /// Returns a short, human-readable label for this kind, used in CLI diagnostics.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Reference => "ref",
            Self::Hyperlink => "hyperlink",
            Self::AnonymousReference => "anonymous reference",
            Self::TermReference => "term",
            Self::OptionReference => "option",
            Self::EquationReference => "equation",
            Self::EntityReference(_) => "entity",
            Self::EntityTypeMismatch { .. } => "entity type mismatch",
            Self::DomainObjectReference(_) => "domain object",
            Self::AmbiguousDomainObjectReference { .. } => "ambiguous domain object",
        }
    }
}

/// A cross-reference that failed to resolve while rendering a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenLink {
    pub kind: BrokenLinkKind,
    pub target: String,
    /// Where the offending role was written, when the AST node carried a
    /// position. `None` for a reference the parser could not place — content
    /// generated after the parse, such as a `.. csv-table::` cell.
    pub span: Option<Span>,
}

impl BrokenLink {
    /// The diagnostic code this reports under, delegated to its
    /// [`BrokenLinkKind`].
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.kind.code()
    }
}

/// A domain-object reference that *did* resolve, but only via
/// [`rusty_sphinx_ast::ObjectType::role_alias_candidates`]'s fallback — the
/// definition's own object type doesn't match the one the role asked for
/// (e.g. a `:exc:` role resolved against a `.. class::` definition, as
/// `CPython`'s `xmlrpc.client.rst` does with `Fault`). Deliberately not a
/// [`BrokenLink`]: the reference works and the build is never failed for it
/// (not even under `--strict-links`) — this only flags a source
/// inconsistency the author may want to clean up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectTypeMismatch {
    /// The qualified name the reference resolved against.
    pub name: String,
    /// Where the offending role was written, when known — see
    /// [`BrokenLink::span`].
    pub span: Option<Span>,
    /// The object type the role asked for (e.g. `exception`, from `:exc:`).
    pub requested_type: ObjectType,
    /// The object type the definition actually has (e.g. `class`).
    pub resolved_type: ObjectType,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::PyObjectType;

    #[test]
    fn test_as_str_labels_each_simple_kind() {
        // Given the kinds that carry no payload
        let kinds = [
            (BrokenLinkKind::Reference, "ref"),
            (BrokenLinkKind::Hyperlink, "hyperlink"),
            (BrokenLinkKind::AnonymousReference, "anonymous reference"),
            (BrokenLinkKind::TermReference, "term"),
            (BrokenLinkKind::OptionReference, "option"),
        ];

        // When / Then
        for (kind, expected) in kinds {
            assert_eq!(kind.as_str(), expected);
        }
    }

    #[test]
    fn test_as_str_labels_domain_object_kind() {
        // Given
        let kind = BrokenLinkKind::DomainObjectReference(ObjectType::Py(PyObjectType::Function));

        // When
        let label = kind.as_str();

        // Then
        assert_eq!(label, "domain object");
    }

    #[test]
    fn test_as_str_labels_ambiguous_domain_object_kind() {
        // Given
        let kind = BrokenLinkKind::AmbiguousDomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Function),
            candidates: vec!["a.f".to_string(), "b.f".to_string()],
        };

        // When
        let label = kind.as_str();

        // Then
        assert_eq!(label, "ambiguous domain object");
    }
}
