//! Resolving a domain-object cross-reference (`:func:`, `:py:class:`,
//! `:c:macro:`, …) against the project index.
//!
//! Kept apart from the HTML emission in [`crate::inline`] so the search
//! strategy — which order names are tried in, and what counts as a match —
//! can be read and tested on its own.
//!
//! # Relationship to real Sphinx
//!
//! Sphinx's `PythonDomain.find_obj` is the reference implementation, and the
//! tier lists here (via
//! [`rinx_scope::PythonScope::reference_candidates`]) follow it. Two
//! behaviours deliberately differ:
//!
//! 1. **The requested object type is checked in every tier, in both search
//!    orders.** Sphinx checks it only for dot-prefixed targets; for a plain
//!    target it takes the first name that matches whatever its type. That
//!    makes `` :func:`Thread.run` `` silently link to a *method*, hiding an
//!    authoring mistake rather than reporting it.
//! 2. **An ambiguous suffix match does not resolve.** Sphinx warns and links
//!    the first candidate; here nothing is linked and the candidates are
//!    named in the diagnostic, so the author can disambiguate rather than
//!    inherit an alphabetical accident.
//!
//! A consequence of (1) is that Sphinx's "only exact matches allowed for
//! modules" rule is not reproduced: it exists to stop type-blind matching
//! from resolving `` :mod:`foo` `` onto a function, and with the type always
//! checked that hazard is gone — so `` :mod:`minidom` `` written under
//! `.. module:: xml.dom` is allowed to find `xml.dom.minidom`.

use rinx_ast::{InventorySelector, ObjectType, TargetName, TargetSearchOrder};
use rinx_index::{ExternalInventory, ProjectIndex};
use rinx_scope::Scope;

use super::{ExternalHit, external_entry_types, resolve_external};
use std::cell::OnceCell;
use std::collections::BTreeMap;

/// The outcome of resolving one domain-object reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainObjectResolution<'a> {
    /// Exactly one object matched.
    Resolved {
        /// The object type the *definition* has, which may differ from the
        /// one the role asked for when
        /// [`ObjectType::role_alias_candidates`] accepted an alias.
        object_type: ObjectType,
        /// The qualified name the reference resolved to — the anchor is
        /// built from this, not from the text the author wrote.
        qualified_name: String,
        doc_path: &'a str,
    },
    /// The suffix search found several equally good objects, so the
    /// reference does not name one thing. Candidates are qualified names, in
    /// index order.
    Ambiguous { candidates: Vec<String> },
    /// No document of this site defines the object, but another site's
    /// inventory lists it — reached only after every local tier, the suffix
    /// search included, has missed.
    External(ExternalHit<'a>),
    /// Nothing matched.
    NotFound,
}

/// Resolves domain-object references against one [`ProjectIndex`].
///
/// Holds the index for the lifetime of a document render so the derived
/// suffix index (see [`Self::suffix_index`]) is built at most once and reused
/// by every reference on the page.
pub(crate) struct DomainObjectResolver<'a> {
    index: &'a ProjectIndex,
    /// Lazily derived: only a dot-prefixed reference that misses every exact
    /// tier ever needs it, and most pages have none.
    suffix_index: OnceCell<BTreeMap<String, &'a TargetName>>,
}

impl<'a> DomainObjectResolver<'a> {
    pub(crate) fn new(index: &'a ProjectIndex) -> Self {
        Self {
            index,
            suffix_index: OnceCell::new(),
        }
    }

    /// The other sites' inventories this resolver searches after its own
    /// index — what a renderer needs to say why a reference failed.
    pub(crate) fn external_inventories(&self) -> &'a [ExternalInventory] {
        &self.index.external_inventories
    }

    /// Resolves a reference to `name` of type `object_type`, written inside
    /// `scope`, using the search order the role's target asked for — against
    /// this site's own objects unless `selector` is an `:external:` one, then
    /// against the other sites' inventories.
    pub(crate) fn resolve(
        &self,
        scope: &Scope,
        object_type: ObjectType,
        name: &str,
        order: TargetSearchOrder,
        selector: &InventorySelector,
    ) -> DomainObjectResolution<'a> {
        let local = if selector.allows_local() {
            self.resolve_local(scope, object_type, name, order)
        } else {
            DomainObjectResolution::NotFound
        };
        match local {
            DomainObjectResolution::NotFound => resolve_external(
                &self.index.external_inventories,
                &external_entry_types(object_type),
                name,
                selector,
            )
            .map_or(
                DomainObjectResolution::NotFound,
                DomainObjectResolution::External,
            ),
            resolved_or_ambiguous => resolved_or_ambiguous,
        }
    }

    /// The search against this site's own objects: every exact tier, then
    /// the suffix search a dot-prefixed target allows.
    fn resolve_local(
        &self,
        scope: &Scope,
        object_type: ObjectType,
        name: &str,
        order: TargetSearchOrder,
    ) -> DomainObjectResolution<'a> {
        let exact = scope
            .reference_candidates(object_type.domain(), name, order)
            .into_iter()
            .find_map(|candidate| {
                let (matched_type, doc_path) = self.lookup(object_type, &candidate)?;
                Some(DomainObjectResolution::Resolved {
                    object_type: matched_type,
                    qualified_name: candidate,
                    doc_path,
                })
            });

        match exact {
            Some(resolution) => resolution,
            None if order.allows_suffix_search() => self.resolve_by_suffix(object_type, name),
            None => DomainObjectResolution::NotFound,
        }
    }

    /// Looks one exact qualified name up, honouring the object-type aliases
    /// the requested type accepts (most-preferred, i.e. the requested type
    /// itself, first).
    ///
    /// A name that exists under an incompatible object type is *not* a
    /// match, so the caller keeps walking its remaining tiers.
    fn lookup(
        &self,
        object_type: ObjectType,
        qualified_name: &str,
    ) -> Option<(ObjectType, &'a str)> {
        let entries = self
            .index
            .domain_objects
            .get(&TargetName::new(qualified_name))?;
        let matched_type = object_type
            .role_alias_candidates()
            .iter()
            .find(|candidate| entries.contains_key(candidate))?;
        Some((*matched_type, entries.get(matched_type)?.as_str()))
    }

    /// Sphinx's "fuzzy" fallback: treat the target as a dotted *suffix* and
    /// search every indexed object name for it, so `` :meth:`.TarFile.close` ``
    /// finds `tarfile.TarFile.close` from a document that never mentions
    /// `tarfile`.
    fn resolve_by_suffix(&self, object_type: ObjectType, name: &str) -> DomainObjectResolution<'a> {
        let mut matches: Vec<(ObjectType, &'a TargetName, &'a str)> = self
            .names_ending_in(name)
            .into_iter()
            .filter_map(|indexed_name| {
                let (matched_type, doc_path) = self.lookup(object_type, indexed_name.as_str())?;
                Some((matched_type, indexed_name, doc_path))
            })
            .collect();

        match matches.len() {
            0 => DomainObjectResolution::NotFound,
            1 => {
                let (matched_type, indexed_name, doc_path) = matches.remove(0);
                DomainObjectResolution::Resolved {
                    object_type: matched_type,
                    qualified_name: indexed_name.as_str().to_string(),
                    doc_path,
                }
            }
            _ => DomainObjectResolution::Ambiguous {
                candidates: matches
                    .into_iter()
                    .map(|(_, indexed_name, _)| indexed_name.as_str().to_string())
                    .collect(),
            },
        }
    }

    /// Every indexed object name ending in `.name` at a segment boundary, of
    /// whatever object type — the candidates of Sphinx's "fuzzy" search, before
    /// any type is checked. `:any:` checks no type at all, so it takes these
    /// as they are.
    pub(crate) fn names_ending_in(&self, name: &str) -> Vec<&'a TargetName> {
        // A suffix must start at a segment boundary: `.close` may not match
        // `preclose`. Searching the reversed-segment form turns that into a
        // prefix query whose trailing separator enforces the boundary.
        let prefix = format!(
            "{}.",
            reverse_dotted_segments(TargetName::new(name).as_str())
        );
        self.suffix_index()
            .range(prefix.clone()..)
            .take_while(|(reversed, _)| reversed.starts_with(&prefix))
            .map(|(_, indexed_name)| *indexed_name)
            .collect()
    }

    /// Every indexed object name keyed by its dot-segment-reversed form, so a
    /// suffix search is a prefix range scan instead of a scan of the whole
    /// index (~50k names for the `CPython` corpus).
    ///
    /// Derived here rather than stored on [`ProjectIndex`]: the index is a
    /// serialized, build-cached artifact, and only the render phase searches
    /// it this way.
    fn suffix_index(&self) -> &BTreeMap<String, &'a TargetName> {
        self.suffix_index.get_or_init(|| {
            self.index
                .domain_objects
                .keys()
                .map(|name| (reverse_dotted_segments(name.as_str()), name))
                .collect()
        })
    }
}

/// Reverses the dotted segments of a qualified name
/// (`tarfile.TarFile.close` → `close.TarFile.tarfile`).
///
/// A bijection, so distinct names never collide in the derived index.
fn reverse_dotted_segments(name: &str) -> String {
    name.split('.').rev().collect::<Vec<_>>().join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{CObjectType, PyObjectType};

    #[test]
    fn test_resolve_falls_back_to_another_sites_inventory() {
        // Given
        let index = crate::test_support::index_linking_into_python();
        let resolver = DomainObjectResolver::new(&index);

        // When — `:exc:` accepts the `py:exception` entry
        let resolution = resolver.resolve(
            &Scope::default(),
            ObjectType::Py(PyObjectType::Exception),
            "ValueError",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        let DomainObjectResolution::External(hit) = resolution else {
            panic!("expected an external resolution, got {resolution:?}");
        };
        assert_eq!(hit.inventory.name.as_str(), "python");
    }

    #[test]
    fn test_resolve_prefers_a_local_object_over_an_external_one() {
        // Given
        let mut index = crate::test_support::index_linking_into_python();
        index.insert_domain_object(ObjectType::Py(PyObjectType::Class), "dict", "types.rst");
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &Scope::default(),
            ObjectType::Py(PyObjectType::Class),
            "dict",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert!(matches!(
            resolution,
            DomainObjectResolution::Resolved { .. }
        ));
    }

    #[test]
    fn test_resolve_does_not_search_inventories_for_an_object_of_the_wrong_type() {
        // Given — `dict` is a class, and a `:func:` asks for a function
        let index = crate::test_support::index_linking_into_python();
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &Scope::default(),
            ObjectType::Py(PyObjectType::Function),
            "dict",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(resolution, DomainObjectResolution::NotFound);
    }

    fn py(object_type: PyObjectType) -> ObjectType {
        ObjectType::Py(object_type)
    }

    /// An index holding the shape of `CPython`'s `datetime` docs: a module
    /// whose main class has the same name, plus a method and an attribute on
    /// that class.
    fn datetime_index() -> ProjectIndex {
        let mut index = ProjectIndex::default();
        index.insert_domain_object(py(PyObjectType::Module), "datetime", "library/datetime.rst");
        index.insert_domain_object(
            py(PyObjectType::Class),
            "datetime.datetime",
            "library/datetime.rst",
        );
        index.insert_domain_object(
            py(PyObjectType::Class),
            "datetime.time",
            "library/datetime.rst",
        );
        index.insert_domain_object(
            py(PyObjectType::Method),
            "datetime.datetime.strptime",
            "library/datetime.rst",
        );
        index.insert_domain_object(
            py(PyObjectType::Attribute),
            "datetime.datetime.tzinfo",
            "library/datetime.rst",
        );
        index
    }

    fn module_scope(module: &str) -> Scope {
        let mut scope = Scope::default();
        scope.python.set_module(module);
        scope
    }

    #[test]
    fn test_names_ending_in_matches_only_at_a_segment_boundary() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(py(PyObjectType::Method), "pkg.Box.close", "a.rst");
        index.insert_domain_object(py(PyObjectType::Function), "pkg.preclose", "a.rst");
        let resolver = DomainObjectResolver::new(&index);

        // When
        let names = resolver.names_ending_in("close");

        // Then
        assert_eq!(names, vec![&TargetName::new("pkg.Box.close")]);
    }

    #[test]
    fn test_reverse_dotted_segments_reverses_qualified_name() {
        // Given / When / Then
        assert_eq!(
            reverse_dotted_segments("tarfile.TarFile.close"),
            "close.TarFile.tarfile"
        );
    }

    #[test]
    fn test_reverse_dotted_segments_leaves_undotted_name_unchanged() {
        // Given / When / Then
        assert_eq!(reverse_dotted_segments("close"), "close");
    }

    #[test]
    fn test_resolve_dot_prefixed_name_prefers_the_module_qualified_tier() {
        // Given — the 57-occurrence `:class:`.datetime`` case: inside
        // `.. module:: datetime`, both a module and a class are called
        // `datetime`, and the dot says "the nearby one".
        let index = datetime_index();
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &module_scope("datetime"),
            py(PyObjectType::Class),
            "datetime",
            TargetSearchOrder::MostQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: py(PyObjectType::Class),
                qualified_name: "datetime.datetime".to_string(),
                doc_path: "library/datetime.rst",
            }
        );
    }

    #[test]
    fn test_resolve_undotted_name_prefers_the_bare_tier() {
        // Given — the same index and scope, but no leading dot: Sphinx
        // searches the unqualified name first, which here is the *module*.
        let index = datetime_index();
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &module_scope("datetime"),
            py(PyObjectType::Module),
            "datetime",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: py(PyObjectType::Module),
                qualified_name: "datetime".to_string(),
                doc_path: "library/datetime.rst",
            }
        );
    }

    #[test]
    fn test_resolve_dot_prefixed_name_reaches_the_module_and_class_tier() {
        // Given — `:attr:`.tzinfo`` written inside `.. class:: datetime`.
        let index = datetime_index();
        let resolver = DomainObjectResolver::new(&index);
        let mut scope = module_scope("datetime");
        scope.python.push_classes(&["datetime".to_string()]);

        // When
        let resolution = resolver.resolve(
            &scope,
            py(PyObjectType::Attribute),
            "tzinfo",
            TargetSearchOrder::MostQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: py(PyObjectType::Attribute),
                qualified_name: "datetime.datetime.tzinfo".to_string(),
                doc_path: "library/datetime.rst",
            }
        );
    }

    #[test]
    fn test_resolve_skips_a_name_hit_of_an_incompatible_object_type() {
        // Given — `datetime` exists as a module, but a `:class:` role asked
        // for a class. Unlike real Sphinx, the type is checked even without a
        // leading dot, so the bare tier must not swallow the reference.
        let index = datetime_index();
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &module_scope("datetime"),
            py(PyObjectType::Class),
            "datetime",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then — resolution falls through to the module-qualified class.
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: py(PyObjectType::Class),
                qualified_name: "datetime.datetime".to_string(),
                doc_path: "library/datetime.rst",
            }
        );
    }

    #[test]
    fn test_resolve_accepts_an_aliased_object_type() {
        // Given — `CPython` documents `Fault` with `.. class::` but
        // references it with `:exc:`.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(py(PyObjectType::Class), "fault", "library/xmlrpc.rst");
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &Scope::default(),
            py(PyObjectType::Exception),
            "fault",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then — resolved, reporting the type the *definition* has.
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: py(PyObjectType::Class),
                qualified_name: "fault".to_string(),
                doc_path: "library/xmlrpc.rst",
            }
        );
    }

    #[test]
    fn test_resolve_accepts_a_c_macro_definition_for_a_c_function_role() {
        // Given — `CPython`'s `c-api/gcsupport.rst` defines the function-like
        // macro `Py_VISIT` with `.. c:macro::` and references it with
        // `:c:func:` from the same file.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::C(CObjectType::Macro),
            "Py_VISIT",
            "c-api/gcsupport.rst",
        );
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &Scope::default(),
            ObjectType::C(CObjectType::Function),
            "Py_VISIT",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then — resolved, reporting the type the *definition* has.
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: ObjectType::C(CObjectType::Macro),
                qualified_name: "Py_VISIT".to_string(),
                doc_path: "c-api/gcsupport.rst",
            }
        );
    }

    #[test]
    fn test_resolve_accepts_a_c_function_definition_for_a_c_macro_role() {
        // Given — the reverse direction, equally real: `Py_REFCNT` is defined
        // `.. c:function::` in `c-api/refcounting.rst` and referenced via
        // `:c:macro:` from `c-api/structures.rst`.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::C(CObjectType::Function),
            "Py_REFCNT",
            "c-api/refcounting.rst",
        );
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &Scope::default(),
            ObjectType::C(CObjectType::Macro),
            "Py_REFCNT",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: ObjectType::C(CObjectType::Function),
                qualified_name: "Py_REFCNT".to_string(),
                doc_path: "c-api/refcounting.rst",
            }
        );
    }

    #[test]
    fn test_resolve_prefers_the_exact_object_type_over_an_alias() {
        // Given — one name defined as both a `c:function` and a `c:macro`.
        // Self comes first in the candidate list, so the requested type wins.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::C(CObjectType::Function), "MAX", "func.rst");
        index.insert_domain_object(ObjectType::C(CObjectType::Macro), "MAX", "macro.rst");
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &Scope::default(),
            ObjectType::C(CObjectType::Macro),
            "MAX",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then — the `c:macro` definition, not the aliased `c:function` one.
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: ObjectType::C(CObjectType::Macro),
                qualified_name: "MAX".to_string(),
                doc_path: "macro.rst",
            }
        );
    }

    #[test]
    fn test_resolve_falls_back_to_a_suffix_match_for_a_dot_prefixed_name() {
        // Given — the Sphinx documentation's own example: `:meth:`.TarFile.close``
        // resolves even though the current document is not `tarfile`.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            py(PyObjectType::Method),
            "tarfile.TarFile.close",
            "library/tarfile.rst",
        );
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &module_scope("shutil"),
            py(PyObjectType::Method),
            "TarFile.close",
            TargetSearchOrder::MostQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: py(PyObjectType::Method),
                qualified_name: "tarfile.tarfile.close".to_string(),
                doc_path: "library/tarfile.rst",
            }
        );
    }

    #[test]
    fn test_resolve_dot_prefixed_c_member_finds_nested_member_via_suffix_search() {
        // Given — a `c:member` nested under `.. c:struct:: Data` (registered
        // as `Data.count` by the analyzer's `CScope`), referenced with a
        // dot-prefixed target (`` :c:member:`.count` ``). No C-domain-specific
        // resolution code exists for this — `CScope` only qualifies
        // definitions; this proves the existing, domain-agnostic suffix
        // search (used for Python's `.TarFile.close`-style lookups above)
        // already covers Sphinx's "nested symbols found even when omitted"
        // for the `c` domain too.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::C(CObjectType::Member), "Data.count", "api.rst");
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &Scope::default(),
            ObjectType::C(CObjectType::Member),
            "count",
            TargetSearchOrder::MostQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then — `TargetName` normalizes to lowercase, like every other
        // indexed name.
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: ObjectType::C(CObjectType::Member),
                qualified_name: "data.count".to_string(),
                doc_path: "api.rst",
            }
        );
    }

    #[test]
    fn test_resolve_finds_unqualified_c_member_via_enclosing_c_scope() {
        // Given — the bug's own reproducer (`docs/dev/known_bugs.md` #1): a
        // `c:member` referenced *without* a leading dot from inside its own
        // enclosing `.. c:type:: PyLongExport` body. Before `CScope` was
        // threaded into resolution, this only worked with a dot prefix (see
        // the suffix-search test above); a plain reference had exactly one
        // exact-match candidate — the bare name — and missed.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::C(CObjectType::Member),
            "PyLongExport.digits",
            "c-api/long.rst",
        );
        let resolver = DomainObjectResolver::new(&index);
        let mut scope = Scope::default();
        scope.c.push_containers(&["PyLongExport".to_string()]);

        // When
        let resolution = resolver.resolve(
            &scope,
            ObjectType::C(CObjectType::Member),
            "digits",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: ObjectType::C(CObjectType::Member),
                qualified_name: "PyLongExport.digits".to_string(),
                doc_path: "c-api/long.rst",
            }
        );
    }

    #[test]
    fn test_resolve_finds_unqualified_c_object_via_the_current_c_namespace() {
        // Given — real Sphinx: "the subsequent cross-references will be
        // searched for starting in the current scope", so a `c:namespace`
        // feeds reference resolution, not just definition qualification.
        // Needs no resolution-side code of its own: `CScope` exposes the
        // namespace through the same current scope `reference_candidates`
        // already reads.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::C(CObjectType::Macro), "A.B.CONSTANT", "api.rst");
        let resolver = DomainObjectResolver::new(&index);
        let mut scope = Scope::default();
        scope.c.set_namespace(Some("A.B"));

        // When — a bare reference written under that namespace.
        let resolution = resolver.resolve(
            &scope,
            ObjectType::C(CObjectType::Macro),
            "CONSTANT",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: ObjectType::C(CObjectType::Macro),
                qualified_name: "A.B.CONSTANT".to_string(),
                doc_path: "api.rst",
            }
        );
    }

    #[test]
    fn test_resolve_reports_every_candidate_when_a_suffix_match_is_ambiguous() {
        // Given — two classes documenting a `close` method.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            py(PyObjectType::Method),
            "tarfile.TarFile.close",
            "library/tarfile.rst",
        );
        index.insert_domain_object(
            py(PyObjectType::Method),
            "zipfile.ZipFile.close",
            "library/zipfile.rst",
        );
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &Scope::default(),
            py(PyObjectType::Method),
            "close",
            TargetSearchOrder::MostQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then — nothing is linked, and the author is told what to choose
        // between.
        assert_eq!(
            resolution,
            DomainObjectResolution::Ambiguous {
                candidates: vec![
                    "tarfile.tarfile.close".to_string(),
                    "zipfile.zipfile.close".to_string(),
                ],
            }
        );
    }

    #[test]
    fn test_resolve_suffix_match_only_considers_compatible_object_types() {
        // Given — two `close` objects, only one of them a method.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            py(PyObjectType::Method),
            "tarfile.TarFile.close",
            "library/tarfile.rst",
        );
        index.insert_domain_object(
            py(PyObjectType::Attribute),
            "select.poll.close",
            "library/select.rst",
        );
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &Scope::default(),
            py(PyObjectType::Method),
            "close",
            TargetSearchOrder::MostQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then — the attribute is not a candidate, so this is not ambiguous.
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: py(PyObjectType::Method),
                qualified_name: "tarfile.tarfile.close".to_string(),
                doc_path: "library/tarfile.rst",
            }
        );
    }

    #[test]
    fn test_resolve_suffix_match_requires_a_segment_boundary() {
        // Given — a name that merely *ends with* the target's letters.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            py(PyObjectType::Method),
            "io.Buffer.preclose",
            "library/io.rst",
        );
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &Scope::default(),
            py(PyObjectType::Method),
            "close",
            TargetSearchOrder::MostQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(resolution, DomainObjectResolution::NotFound);
    }

    #[test]
    fn test_resolve_does_not_suffix_search_an_undotted_name() {
        // Given — the same index, but the target carries no leading dot, so
        // Sphinx's fuzzy fallback must not apply.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            py(PyObjectType::Method),
            "tarfile.TarFile.close",
            "library/tarfile.rst",
        );
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &Scope::default(),
            py(PyObjectType::Method),
            "close",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(resolution, DomainObjectResolution::NotFound);
    }

    #[test]
    fn test_resolve_ignores_python_scope_for_a_c_domain_reference() {
        // Given — a `c` object referenced from a document with a current
        // module; the module must not be prepended.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::C(CObjectType::Function),
            "PyList_Append",
            "c-api/list.rst",
        );
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &module_scope("zipimport"),
            ObjectType::C(CObjectType::Function),
            "PyList_Append",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(
            resolution,
            DomainObjectResolution::Resolved {
                object_type: ObjectType::C(CObjectType::Function),
                qualified_name: "PyList_Append".to_string(),
                doc_path: "c-api/list.rst",
            }
        );
    }

    #[test]
    fn test_resolve_reports_not_found_for_an_unknown_name() {
        // Given
        let index = datetime_index();
        let resolver = DomainObjectResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            &module_scope("datetime"),
            py(PyObjectType::Function),
            "nonexistent",
            TargetSearchOrder::LeastQualifiedFirst,
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(resolution, DomainObjectResolution::NotFound);
    }
}
