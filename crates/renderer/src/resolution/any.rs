//! Resolving an `:any:` cross-reference: searching every kind of target the
//! project index holds for one name.
//!
//! # Search order
//!
//! Sphinx's `ReferencesResolver.resolve_anyref`: the `std` domain first —
//! a label, an option, a glossary term, a document — then every other domain
//! in alphabetical order, which here is `c`, `math` (an `:eq:` label) and
//! `py`. Every kind is searched and *all* hits are collected, because the role
//! resolves only when exactly one thing answers to the name; the order is the
//! order the candidates of an ambiguous reference are listed in.
//!
//! Each kind is searched the way its own role searches it, so `:any:` finds
//! what the dedicated role would have found:
//!
//! - a label as `:ref:` does, including one that sits above no heading — where
//!   Sphinx's `:any:` finds nothing, since its `:ref:` needs a title to show;
//! - an option with the ambient `.. program::`, through
//!   [`OptionResolver::resolve_local`];
//! - a glossary term case-insensitively, as `:term:` does — Sphinx 9's
//!   `:any:` misses every term, keying its lookup differently from how it
//!   stores them;
//! - a document by its name relative to the referencing one, or from the
//!   source root with a leading `/`;
//! - a domain object through the scope tiers of Sphinx's `find_obj` in its
//!   "fuzzy" mode: the first tier naming any object of the domain wins, and
//!   only if none does is a `py` name matched as a dotted suffix. A trailing
//!   `()` is dropped for this lookup alone, as Sphinx does; no object type is
//!   checked, since the role asks for none.
//!
//! Another site's inventory is searched only when nothing local answers, and
//! the first listing wins — intersphinx reports no ambiguity for `:any:`.

use rinx_ast::{Domain, InventorySelector, ObjectType, TargetName, TargetSearchOrder};
use rinx_index::{EquationLocation, ProjectIndex, SpecialPage};
use rinx_scope::Scope;

use super::{
    DomainObjectResolver, ExternalHit, OptionResolution, OptionResolver, resolve_document,
    resolve_external_any,
};

/// One thing of this site an `:any:` target names, with what its link needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AnyHit<'a> {
    /// A `.. _label:` (or any other `:ref:` target, an entity's included).
    Label { name: TargetName, doc_path: &'a str },
    /// A glossary term, defined in `doc_path`.
    Term { name: TargetName, doc_path: &'a str },
    /// A `.. option::`, under the program-qualified name it matched.
    Option {
        qualified_name: String,
        doc_path: &'a str,
    },
    /// A whole document, by its `.rst` path.
    Document { doc_path: &'a str },
    /// A labeled, numbered equation.
    Equation {
        label: TargetName,
        location: &'a EquationLocation,
    },
    /// A page the build writes that no document is, by its predefined label
    /// (`genindex`, `py-modindex`) — see [`ProjectIndex::special_page`].
    SpecialPage { page: SpecialPage },
    /// A `py` or `c` domain object.
    DomainObject {
        object_type: ObjectType,
        qualified_name: String,
        doc_path: &'a str,
    },
}

impl AnyHit<'_> {
    /// The role that names this hit and nothing else, written out with
    /// `target` — what an ambiguous reference's diagnostic suggests instead.
    /// Spelled domain-qualified, as Sphinx's own "more than one target"
    /// warning spells it.
    pub(crate) fn disambiguating_role(&self, target: &str) -> String {
        match self {
            Self::Label { .. } | Self::SpecialPage { .. } => format!(":std:ref:`{target}`"),
            Self::Term { .. } => format!(":std:term:`{target}`"),
            Self::Option { .. } => format!(":std:option:`{target}`"),
            Self::Document { .. } => format!(":std:doc:`{target}`"),
            Self::Equation { .. } => format!(":math:eq:`{target}`"),
            Self::DomainObject {
                object_type,
                qualified_name,
                ..
            } => format!(
                ":{}:{}:`{qualified_name}`",
                object_type.domain().as_str(),
                object_type.role_name()
            ),
        }
    }
}

/// The outcome of resolving one `:any:` reference, before duplicates are
/// merged — that needs the hrefs, which are the renderer's to build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AnyResolution<'a> {
    /// Every local hit, in search order; never empty.
    Local(Vec<AnyHit<'a>>),
    /// Nothing local, but a label, glossary term or equation of that name is
    /// one several documents define; carries every claimant.
    Contested(Vec<String>),
    /// Nothing local, but another site's inventory lists the target.
    External(ExternalHit<'a>),
    NotFound,
}

/// Resolves `:any:` references against one [`ProjectIndex`], reusing the
/// per-page resolvers the dedicated roles search through.
pub(crate) struct AnyResolver<'r, 'a> {
    pub index: &'a ProjectIndex,
    pub domains: &'r DomainObjectResolver<'a>,
    pub options: &'r OptionResolver<'a>,
}

impl<'a> AnyResolver<'_, 'a> {
    /// Resolves `target`, written inside `scope` under `ambient_program`,
    /// against this site unless `selector` is an `:external:` one, then
    /// against the other sites' inventories.
    pub(crate) fn resolve(
        &self,
        scope: &Scope,
        ambient_program: Option<&str>,
        doc_path: &str,
        target: &str,
        selector: &InventorySelector,
    ) -> AnyResolution<'a> {
        let local = if selector.allows_local() {
            self.local_hits(scope, ambient_program, doc_path, target)
        } else {
            Vec::new()
        };
        if !local.is_empty() {
            return AnyResolution::Local(local);
        }
        if selector.allows_local() {
            let claimants = self.contested_claimants(target);
            if !claimants.is_empty() {
                return AnyResolution::Contested(claimants.into_iter().collect());
            }
        }
        resolve_external_any(&self.index.external_inventories, target, selector)
            .map_or(AnyResolution::NotFound, AnyResolution::External)
    }

    /// Every document claiming a contested label, glossary term or equation
    /// named `target` — the families `:any:` searches by name alone.
    fn contested_claimants(&self, target: &str) -> std::collections::BTreeSet<String> {
        let name = TargetName::new(target);
        let contested = &self.index.ambiguous_definitions;
        [
            contested.targets.get(&name),
            contested.glossary_terms.get(&name),
            contested.equations.get(&name),
        ]
        .into_iter()
        .flatten()
        .flatten()
        .cloned()
        .collect()
    }

    /// Every hit of this site, in the module documentation's order.
    fn local_hits(
        &self,
        scope: &Scope,
        ambient_program: Option<&str>,
        doc_path: &str,
        target: &str,
    ) -> Vec<AnyHit<'a>> {
        let name = TargetName::new(target);
        let mut hits = Vec::new();
        if let Some(label_doc) = self.index.targets.get(&name) {
            hits.push(AnyHit::Label {
                name: name.clone(),
                doc_path: label_doc,
            });
        } else if let Some(page) = self.index.special_page(&name) {
            // One label table in Sphinx, so a document's label shadows the
            // predefined one rather than competing with it.
            hits.push(AnyHit::SpecialPage { page });
        }
        if let Some(OptionResolution::Resolved {
            qualified_name,
            doc_path: option_doc,
        }) = self.options.resolve_local(ambient_program, target)
        {
            hits.push(AnyHit::Option {
                qualified_name,
                doc_path: option_doc,
            });
        }
        if let Some(term_doc) = self.index.glossary_terms.get(&name) {
            hits.push(AnyHit::Term {
                name: name.clone(),
                doc_path: term_doc,
            });
        }
        if let Some(document) = resolve_document(self.index, doc_path, target) {
            hits.push(AnyHit::Document { doc_path: document });
        }
        hits.extend(self.domain_object_hits(scope, Domain::C, target));
        if let Some(location) = self.index.equations.get(&name) {
            hits.push(AnyHit::Equation {
                label: name,
                location,
            });
        }
        hits.extend(self.domain_object_hits(scope, Domain::Py, target));
        hits
    }

    /// The objects of `domain` the target names: every object under the
    /// first scope tier naming any, else — for `py` alone, whose fuzzy search
    /// Sphinx's `:any:` uses — every object whose name ends in the target.
    fn domain_object_hits(&self, scope: &Scope, domain: Domain, target: &str) -> Vec<AnyHit<'a>> {
        let name = target.strip_suffix("()").unwrap_or(target);
        let exact = scope
            .reference_candidates(domain, name, TargetSearchOrder::MostQualifiedFirst)
            .into_iter()
            .map(|candidate| self.objects_named(domain, &candidate))
            .find(|hits| !hits.is_empty());
        match exact {
            Some(hits) => hits,
            None if domain == Domain::Py => self
                .domains
                .names_ending_in(name)
                .into_iter()
                .flat_map(|indexed_name| self.objects_named(domain, indexed_name.as_str()))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Every object of `domain` indexed under exactly `qualified_name`, of
    /// whatever type — a name can carry several.
    fn objects_named(&self, domain: Domain, qualified_name: &str) -> Vec<AnyHit<'a>> {
        self.index
            .domain_objects
            .get(&TargetName::new(qualified_name))
            .into_iter()
            .flatten()
            .filter(|(object_type, _)| object_type.domain() == domain)
            .map(|(object_type, object_doc)| AnyHit::DomainObject {
                object_type: *object_type,
                qualified_name: qualified_name.to_string(),
                doc_path: object_doc,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{CObjectType, PyObjectType, StdObjectType};

    fn py(object_type: PyObjectType) -> ObjectType {
        ObjectType::Py(object_type)
    }

    fn resolve_in<'a>(index: &'a ProjectIndex, scope: &Scope, target: &str) -> AnyResolution<'a> {
        resolve_with(index, scope, target, &InventorySelector::Any)
    }

    fn resolve_with<'a>(
        index: &'a ProjectIndex,
        scope: &Scope,
        target: &str,
        selector: &InventorySelector,
    ) -> AnyResolution<'a> {
        let domains = DomainObjectResolver::new(index);
        let options = OptionResolver::new(index);
        AnyResolver {
            index,
            domains: &domains,
            options: &options,
        }
        .resolve(scope, None, "index.rst", target, selector)
    }

    fn resolve<'a>(index: &'a ProjectIndex, target: &str) -> AnyResolution<'a> {
        resolve_in(index, &Scope::default(), target)
    }

    #[test]
    fn test_resolve_finds_a_label_case_insensitively() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("install"), "guide.rst".to_string());

        // When
        let resolution = resolve(&index, "INSTALL");

        // Then
        assert_eq!(
            resolution,
            AnyResolution::Local(vec![AnyHit::Label {
                name: TargetName::new("install"),
                doc_path: "guide.rst",
            }])
        );
    }

    #[test]
    fn test_resolve_finds_the_general_index_by_its_builtin_label() {
        // Given
        let index = ProjectIndex::default();

        // When
        let resolution = resolve(&index, "genindex");

        // Then
        assert_eq!(
            resolution,
            AnyResolution::Local(vec![AnyHit::SpecialPage {
                page: index
                    .special_page(&TargetName::new("genindex"))
                    .expect("always written"),
            }])
        );
    }

    #[test]
    fn test_resolve_prefers_a_documents_label_over_a_special_page() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("genindex"), "guide.rst".to_string());

        // When
        let resolution = resolve(&index, "genindex");

        // Then — one hit, not an ambiguity
        assert_eq!(
            resolution,
            AnyResolution::Local(vec![AnyHit::Label {
                name: TargetName::new("genindex"),
                doc_path: "guide.rst",
            }])
        );
    }

    #[test]
    fn test_resolve_finds_a_glossary_term() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("Widget"), "glossary.rst".to_string());

        // When / Then
        assert_eq!(
            resolve(&index, "widget"),
            AnyResolution::Local(vec![AnyHit::Term {
                name: TargetName::new("widget"),
                doc_path: "glossary.rst",
            }])
        );
    }

    #[test]
    fn test_resolve_finds_an_option_under_its_embedded_program() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Std(StdObjectType::Cmdoption),
            "prog.--verbose",
            "cli.rst",
        );

        // When / Then — the option is never found again as a domain object
        assert_eq!(
            resolve(&index, "prog --verbose"),
            AnyResolution::Local(vec![AnyHit::Option {
                qualified_name: "prog.--verbose".to_string(),
                doc_path: "cli.rst",
            }])
        );
    }

    #[test]
    fn test_resolve_finds_a_document_relative_to_the_referencing_one() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("guide.rst".to_string(), "The guide".to_string());

        // When / Then
        assert_eq!(
            resolve(&index, "guide"),
            AnyResolution::Local(vec![AnyHit::Document {
                doc_path: "guide.rst"
            }])
        );
    }

    #[test]
    fn test_resolve_finds_an_equation() {
        // Given
        let mut index = ProjectIndex::default();
        index.equations.insert(
            TargetName::new("euler"),
            EquationLocation::new("math.rst", 1),
        );

        // When
        let resolution = resolve(&index, "euler");

        // Then
        let AnyResolution::Local(hits) = resolution else {
            panic!("expected a local hit, got {resolution:?}");
        };
        assert!(matches!(&hits[..], [AnyHit::Equation { .. }]));
    }

    #[test]
    fn test_resolve_finds_a_python_object_through_the_module_scope() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(py(PyObjectType::Function), "pkg.run", "api.rst");
        let mut scope = Scope::default();
        scope.python.set_module("pkg");

        // When / Then — with the call parens an author may write
        assert_eq!(
            resolve_in(&index, &scope, "run()"),
            AnyResolution::Local(vec![AnyHit::DomainObject {
                object_type: py(PyObjectType::Function),
                qualified_name: "pkg.run".to_string(),
                doc_path: "api.rst",
            }])
        );
    }

    #[test]
    fn test_resolve_prefers_an_exact_python_name_over_suffix_matches() {
        // Given — `close` both as a module function and as two methods
        let mut index = ProjectIndex::default();
        index.insert_domain_object(py(PyObjectType::Function), "pkg.close", "api.rst");
        index.insert_domain_object(py(PyObjectType::Method), "pkg.Box.close", "api.rst");
        let mut scope = Scope::default();
        scope.python.set_module("pkg");

        // When
        let resolution = resolve_in(&index, &scope, "close");

        // Then
        let AnyResolution::Local(hits) = resolution else {
            panic!("expected a local hit, got {resolution:?}");
        };
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn test_resolve_collects_every_python_suffix_match() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(py(PyObjectType::Method), "a.Box.close", "a.rst");
        index.insert_domain_object(py(PyObjectType::Method), "b.Other.close", "b.rst");

        // When
        let resolution = resolve(&index, "close");

        // Then
        let AnyResolution::Local(hits) = resolution else {
            panic!("expected local hits, got {resolution:?}");
        };
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn test_resolve_does_not_suffix_match_a_c_name() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::C(CObjectType::Member), "Box.size", "c.rst");

        // When / Then
        assert_eq!(resolve(&index, "size"), AnyResolution::NotFound);
    }

    #[test]
    fn test_resolve_lists_every_kind_a_shared_name_has_in_search_order() {
        // Given — Sphinx's own example of an ambiguous `:any:`: a label, a C
        // function and a Python function all called `shared`
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("shared"), "other.rst".to_string());
        index.insert_domain_object(py(PyObjectType::Function), "shared", "other.rst");
        index.insert_domain_object(ObjectType::C(CObjectType::Function), "shared", "other.rst");

        // When
        let resolution = resolve(&index, "shared");

        // Then
        let AnyResolution::Local(hits) = resolution else {
            panic!("expected local hits, got {resolution:?}");
        };
        let roles: Vec<String> = hits
            .iter()
            .map(|hit| hit.disambiguating_role("shared"))
            .collect();
        assert_eq!(
            roles,
            vec![":std:ref:`shared`", ":c:func:`shared`", ":py:func:`shared`"]
        );
    }

    #[test]
    fn test_resolve_falls_back_to_another_sites_inventory() {
        // Given — nothing here is called `dict`, but Python's inventory is
        let index = crate::test_support::index_linking_into_python();

        // When
        let resolution = resolve(&index, "dict");

        // Then
        let AnyResolution::External(hit) = resolution else {
            panic!("expected an external hit, got {resolution:?}");
        };
        assert_eq!(hit.target.name, "dict");
    }

    #[test]
    fn test_resolve_searches_no_local_target_for_an_external_selector() {
        // Given — a local label `dict`, and Python's inventory
        let mut index = crate::test_support::index_linking_into_python();
        index
            .targets
            .insert(TargetName::new("dict"), "local.rst".to_string());

        // When
        let resolution = resolve_with(
            &index,
            &Scope::default(),
            "dict",
            &InventorySelector::ExternalOnly,
        );

        // Then
        assert!(matches!(resolution, AnyResolution::External(_)));
    }

    #[test]
    fn test_resolve_reports_an_unknown_target_as_not_found() {
        // Given / When / Then
        assert_eq!(
            resolve(&ProjectIndex::default(), "nothing"),
            AnyResolution::NotFound
        );
    }
}
