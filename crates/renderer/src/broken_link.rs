//! Diagnostics a render produces alongside its HTML: cross-references that
//! failed to resolve against the [`ProjectIndex`](rinx_index::ProjectIndex),
//! and references that resolved only through an object-type fallback.

use rinx_ast::{DiagnosticCode, DiagnosticSubject, Domain, InventoryName, ObjectType, Span};

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
    /// A `:doc:` role (`InlineNode::DocReference`) naming no document, here
    /// or in any inventory it may search.
    DocReference,
    /// An `:any:` role whose target names nothing, here or in any inventory
    /// it may search.
    AnyReference,
    /// An `:any:` role whose target names several things of this site at
    /// once. Deliberately unresolved, as an ambiguous domain-object reference
    /// is (real Sphinx links the first); each candidate is the role that would
    /// name it alone, e.g. ``:py:func:`pkg.close` ``.
    AmbiguousAnyReference { candidates: Vec<String> },
    /// A reference to a name several documents define — a label, glossary
    /// term, equation, domain object or entity — which therefore names none
    /// of them. Carries the claiming documents, which is where the fix is;
    /// each definition is also reported there, under its family's code.
    AmbiguousTarget { documents: Vec<String> },
    /// A `:numref:` whose label names nothing a number is given to — no
    /// label at all, or one on a paragraph or an uncaptioned code block.
    NumberReference,
    /// A `:numref:` to a figure, table or code block while `numfig` is off.
    NumberingDisabled,
    /// A `:numref:` to an element or section that was given no number.
    UnnumberedReference,
    /// A `:numref:` whose format shows `{name}` for something with no caption.
    UncaptionedReference,
    /// An `:external+name:` role whose `name` the build declared no
    /// inventory under — so nothing was searched, and the fix is the name or
    /// the site's `inventories`, not the target.
    UnknownInventory(InventoryName),
}

impl BrokenLinkKind {
    /// [`Self::AmbiguousTarget`] when several documents claim the name a
    /// reference missed (`claimants`, from
    /// `ProjectIndex::ambiguous_definitions`), else `self` — so the author is
    /// told the name is defined twice, not that it is missing.
    #[must_use]
    pub(crate) fn unless_contested(
        self,
        claimants: Option<&std::collections::BTreeSet<String>>,
    ) -> Self {
        match claimants {
            Some(documents) => Self::AmbiguousTarget {
                documents: documents.iter().cloned().collect(),
            },
            None => self,
        }
    }

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
            Self::DocReference => DiagnosticCode::LinkBrokenDoc,
            Self::AnyReference => DiagnosticCode::LinkBrokenAny,
            Self::AmbiguousAnyReference { .. } => DiagnosticCode::LinkAmbiguousAny,
            Self::AmbiguousTarget { .. } => DiagnosticCode::LinkAmbiguousTarget,
            Self::UnknownInventory(_) => DiagnosticCode::LinkUnknownInventory,
            Self::NumberReference => DiagnosticCode::LinkBrokenNumref,
            Self::NumberingDisabled => DiagnosticCode::NumrefDisabled,
            Self::UnnumberedReference => DiagnosticCode::NumrefUnnumbered,
            Self::UncaptionedReference => DiagnosticCode::NumrefNoCaption,
        }
    }

    /// What a reference of this kind was about, for the language server's
    /// strictness filter: the domain a missed object would have been defined
    /// in. Only a plain miss has one — an ambiguous reference found several
    /// definitions, which no unanalysed extension explains away.
    #[must_use]
    pub fn subject(&self) -> Option<DiagnosticSubject> {
        match self {
            Self::DomainObjectReference(object_type) => {
                Some(DiagnosticSubject::DomainReference(object_type.domain()))
            }
            Self::OptionReference => Some(DiagnosticSubject::DomainReference(Domain::Std)),
            Self::AnyReference => Some(DiagnosticSubject::AnyReference),
            _ => None,
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
            Self::DocReference => "doc reference",
            Self::AnyReference => "any reference",
            Self::AmbiguousAnyReference { .. } => "ambiguous any reference",
            Self::AmbiguousTarget { .. } => "ambiguous target",
            Self::UnknownInventory(_) => "reference into an undeclared inventory",
            Self::NumberReference
            | Self::NumberingDisabled
            | Self::UnnumberedReference
            | Self::UncaptionedReference => "numref",
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

    /// What the author is told: the kind and the target as written, and —
    /// where it is known when resolution fails — what would fix it.
    ///
    /// For a broken domain-object reference that is the requested object
    /// type, which pinpoints what could not be found; an ambiguous reference
    /// lists what it matched, since picking one of them is the fix.
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "broken {} '{}'{}",
            self.kind.as_str(),
            self.target,
            self.kind.explanation()
        )
    }
}

impl BrokenLinkKind {
    /// The parenthetical a broken link's message ends with, with its leading
    /// space — or nothing, for a kind whose code and target say it all.
    fn explanation(&self) -> String {
        match self {
            Self::DomainObjectReference(object_type) => {
                format!(" (referenced as {})", object_type.domain_qualified_str())
            }
            Self::AmbiguousDomainObjectReference {
                object_type,
                candidates,
            } => format!(
                " (referenced as {}, matches {})",
                object_type.domain_qualified_str(),
                candidates.join(", ")
            ),
            Self::AmbiguousAnyReference { candidates } => {
                format!(" (could be {})", candidates.join(" or "))
            }
            Self::AmbiguousTarget { documents } => {
                format!(" (defined in {})", documents.join(" and "))
            }
            Self::UnknownInventory(name) => {
                format!(" (no inventory is declared as '{name}')")
            }
            Self::NumberReference => {
                " (no captioned figure, table or code block, and no heading, has this label)"
                    .to_string()
            }
            Self::NumberingDisabled => {
                " (numfig is off in rinx.toml, so figures, tables and code blocks have no numbers)"
                    .to_string()
            }
            Self::UnnumberedReference => {
                " (it has no number: no toctree reaches its document, or its section is not numbered)"
                    .to_string()
            }
            Self::UncaptionedReference => {
                " (its format shows {name}, but it has no caption)".to_string()
            }
            _ => String::new(),
        }
    }
}

/// A domain-object reference that *did* resolve, but only via
/// [`rinx_ast::ObjectType::role_alias_candidates`]'s fallback — the
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

impl ObjectTypeMismatch {
    /// What the author is told: the name, and both object types spelled with
    /// their domain, so the reader need not assume the two domains agree.
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "domain object '{}' referenced as '{}' but defined as '{}'",
            self.name,
            self.requested_type.domain_qualified_str(),
            self.resolved_type.domain_qualified_str(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::PyObjectType;

    fn link(kind: BrokenLinkKind, target: &str) -> BrokenLink {
        BrokenLink {
            kind,
            target: target.to_string(),
            span: None,
        }
    }

    #[test]
    fn test_subject_names_the_domain_a_domain_object_reference_missed() {
        // Given
        let kind = BrokenLinkKind::DomainObjectReference(ObjectType::Py(PyObjectType::Function));

        // When / Then
        assert_eq!(
            kind.subject(),
            Some(DiagnosticSubject::DomainReference(Domain::Py))
        );
    }

    #[test]
    fn test_subject_puts_an_option_reference_in_the_std_domain() {
        // Given / When / Then
        assert_eq!(
            BrokenLinkKind::OptionReference.subject(),
            Some(DiagnosticSubject::DomainReference(Domain::Std))
        );
    }

    #[test]
    fn test_subject_of_an_any_reference_is_any_domain() {
        // Given / When / Then
        assert_eq!(
            BrokenLinkKind::AnyReference.subject(),
            Some(DiagnosticSubject::AnyReference)
        );
    }

    #[test]
    fn test_subject_is_absent_for_ambiguous_and_label_references() {
        // Given — an ambiguity is a real finding, and labels and documents are
        // never an extension's to define
        let kinds = [
            BrokenLinkKind::Reference,
            BrokenLinkKind::DocReference,
            BrokenLinkKind::AmbiguousDomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Function),
                candidates: vec!["a.f".to_string(), "b.f".to_string()],
            },
        ];

        // When / Then
        for kind in kinds {
            assert_eq!(kind.subject(), None, "{kind:?}");
        }
    }

    #[test]
    fn test_a_broken_links_diagnostic_carries_its_subject() {
        // Given
        let broken = link(BrokenLinkKind::OptionReference, "--verbose");

        // When
        let diagnostic = rinx_ast::Reported::to_diagnostic(&broken);

        // Then
        assert_eq!(
            diagnostic.subject,
            Some(DiagnosticSubject::DomainReference(Domain::Std))
        );
    }

    #[test]
    fn test_message_names_the_kind_and_the_target() {
        // When / Then
        assert_eq!(
            link(BrokenLinkKind::Reference, "missing").message(),
            "broken ref 'missing'"
        );
        assert_eq!(
            link(BrokenLinkKind::DocReference, "../missing").message(),
            "broken doc reference '../missing'"
        );
    }

    #[test]
    fn test_message_names_the_requested_object_type() {
        // Given
        let kind = BrokenLinkKind::DomainObjectReference(ObjectType::Py(PyObjectType::Function));

        // When / Then
        assert_eq!(
            link(kind, "f").message(),
            "broken domain object 'f' (referenced as py:function)"
        );
    }

    #[test]
    fn test_message_lists_what_an_ambiguous_reference_matched() {
        // Given
        let object = BrokenLinkKind::AmbiguousDomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Function),
            candidates: vec!["a.f".to_string(), "b.f".to_string()],
        };
        let target = BrokenLinkKind::AmbiguousTarget {
            documents: vec!["a.rst".to_string(), "b.rst".to_string()],
        };

        // When / Then
        assert_eq!(
            link(object, "f").message(),
            "broken ambiguous domain object 'f' (referenced as py:function, matches a.f, b.f)"
        );
        assert_eq!(
            link(target, "setup").message(),
            "broken ambiguous target 'setup' (defined in a.rst and b.rst)"
        );
    }

    #[test]
    fn test_message_of_a_type_mismatch_names_both_types() {
        // Given
        let mismatch = ObjectTypeMismatch {
            name: "fault".to_string(),
            span: None,
            requested_type: ObjectType::Py(PyObjectType::Exception),
            resolved_type: ObjectType::Py(PyObjectType::Class),
        };

        // When / Then
        assert_eq!(
            mismatch.message(),
            "domain object 'fault' referenced as 'py:exception' but defined as 'py:class'"
        );
    }

    #[test]
    fn test_as_str_labels_each_simple_kind() {
        // Given the kinds that carry no payload
        let kinds = [
            (BrokenLinkKind::Reference, "ref"),
            (BrokenLinkKind::Hyperlink, "hyperlink"),
            (BrokenLinkKind::AnonymousReference, "anonymous reference"),
            (BrokenLinkKind::TermReference, "term"),
            (BrokenLinkKind::OptionReference, "option"),
            (BrokenLinkKind::AnyReference, "any reference"),
            (BrokenLinkKind::DocReference, "doc reference"),
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

    #[test]
    fn test_unknown_inventory_has_its_own_code() {
        // Given
        let kind = BrokenLinkKind::UnknownInventory(InventoryName::new("nope").unwrap());

        // When / Then
        assert_eq!(kind.code(), DiagnosticCode::LinkUnknownInventory);
        assert_eq!(kind.as_str(), "reference into an undeclared inventory");
    }

    #[test]
    fn test_the_two_any_reference_kinds_have_codes_of_their_own() {
        // Given
        let broken = BrokenLinkKind::AnyReference;
        let ambiguous = BrokenLinkKind::AmbiguousAnyReference {
            candidates: vec![":py:func:`a.f`".to_string()],
        };

        // When / Then
        assert_eq!(broken.code(), DiagnosticCode::LinkBrokenAny);
        assert_eq!(ambiguous.code(), DiagnosticCode::LinkAmbiguousAny);
        assert_eq!(ambiguous.as_str(), "ambiguous any reference");
    }

    #[test]
    fn test_a_doc_reference_has_a_code_of_its_own() {
        // Given
        let kind = BrokenLinkKind::DocReference;

        // When / Then
        assert_eq!(kind.code(), DiagnosticCode::LinkBrokenDoc);
    }
}
