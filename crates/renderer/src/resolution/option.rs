//! Resolving a `:option:` cross-reference against the project index.
//!
//! Kept apart from [`super::domain_object`] even though both ultimately
//! read `ProjectIndex::domain_objects`: that resolver's whole design is built
//! around `rinx_scope::Scope`'s per-domain tiered candidate lists
//! (`Scope::reference_candidates`) and `ObjectType::role_alias_candidates`,
//! neither of which `:option:` uses. `:option:` resolution is instead a
//! distinct ambient-program/global-fallback/embedded-program search over
//! `(program, optname)` pairs, so it gets its own small resolver here.
//!
//! # Relationship to real Sphinx
//!
//! This mirrors `StandardDomain.resolve_xref`'s `typ == "option"` branch
//! (`sphinx/domains/std.py`), confirmed against real corpus behaviour (the
//! cached `CPython` docs used for benchmarking): `library/dis.rst`, itself
//! written under an ambient `.. program:: dis`, references the *global*
//! (no-program) option `-X` via `` :option:`-X faulthandler <-X>` `` — which
//! only resolves via step 2's fallback below — and both
//! `` :option:`-O <dis --show-offsets>` `` and
//! `` :option:`--feature-version <ast --feature-version>` `` need step 3's
//! embedded-program peel, since their targets name a *different* program
//! than the one ambient where the role is written.
//!
//! Real Sphinx's algorithm, reproduced here in full:
//!
//! 1. Try `(ambient_program, target)`, where `ambient_program` is whatever
//!    `.. program::` is currently in effect at the point the role is
//!    written (may be `None`).
//! 2. If that misses and `ambient_program` was `Some`, try `(None, target)`
//!    — the global/no-program fallback.
//! 3. If that misses and `target` contains whitespace, peel words off the
//!    *left* one at a time, accumulating them hyphen-joined into a
//!    candidate progname (mirroring `.. program::`'s own hyphen-joining of a
//!    multi-word name), trying `(accumulated_progname, remainder)` after
//!    each peel — stopping at the first hit.
//! 4. Otherwise: unresolved.

use rinx_ast::{InventorySelector, ObjectType, StdObjectType, TargetName};
use rinx_index::{ExternalInventory, ProjectIndex};

use super::{ExternalHit, resolve_external};

/// The outcome of resolving one `:option:` reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OptionResolution<'a> {
    Resolved {
        qualified_name: String,
        doc_path: &'a str,
    },
    /// No document of this site defines the option, but another site's
    /// inventory lists it.
    External(ExternalHit<'a>),
    NotFound,
}

/// Resolves `:option:` references against one [`ProjectIndex`].
pub(crate) struct OptionResolver<'a> {
    index: &'a ProjectIndex,
}

impl<'a> OptionResolver<'a> {
    pub(crate) fn new(index: &'a ProjectIndex) -> Self {
        Self { index }
    }

    /// The other sites' inventories this resolver searches after its own
    /// index — what a renderer needs to say why a reference failed.
    pub(crate) fn external_inventories(&self) -> &'a [ExternalInventory] {
        &self.index.external_inventories
    }

    /// Resolves `target` (the role's text, minus any explicit-title
    /// override, which is handled at parse time) written under
    /// `ambient_program` — see the module doc comment for the search order.
    /// This site's own options come first unless `selector` is an
    /// `:external:` one; the other sites' inventories are searched after.
    pub(crate) fn resolve(
        &self,
        ambient_program: Option<&str>,
        target: &str,
        selector: &InventorySelector,
    ) -> OptionResolution<'a> {
        if selector.allows_local()
            && let Some(resolution) = self.resolve_local(ambient_program, target)
        {
            return resolution;
        }
        self.resolve_external(ambient_program, target, selector)
            .map_or(OptionResolution::NotFound, OptionResolution::External)
    }

    /// The search against this site's own `.. option::` definitions.
    fn resolve_local(
        &self,
        ambient_program: Option<&str>,
        target: &str,
    ) -> Option<OptionResolution<'a>> {
        if let Some(resolution) = self.lookup(ambient_program, target) {
            return Some(resolution);
        }
        if ambient_program.is_some()
            && let Some(resolution) = self.lookup(None, target)
        {
            return Some(resolution);
        }
        if target.contains(char::is_whitespace) {
            let words: Vec<&str> = target.split_whitespace().collect();
            let mut accumulated = String::new();
            for (i, word) in words.iter().enumerate().take(words.len() - 1) {
                if i > 0 {
                    accumulated.push('-');
                }
                accumulated.push_str(word);
                let remainder = words[i + 1..].join(" ");
                if let Some(resolution) = self.lookup(Some(&accumulated), &remainder) {
                    return Some(resolution);
                }
            }
        }
        None
    }

    /// Searches the other sites' inventories, which list a program's option
    /// as `program.option` exactly as this index keys it — qualified by the
    /// ambient program first, then bare, the same two steps a local lookup
    /// takes.
    fn resolve_external(
        &self,
        ambient_program: Option<&str>,
        target: &str,
        selector: &InventorySelector,
    ) -> Option<ExternalHit<'a>> {
        let entry_types = ["std:cmdoption".to_string()];
        let inventories = &self.index.external_inventories;
        ambient_program
            .and_then(|program| {
                resolve_external(
                    inventories,
                    &entry_types,
                    &format!("{program}.{target}"),
                    selector,
                )
            })
            .or_else(|| resolve_external(inventories, &entry_types, target, selector))
    }

    /// Looks up one exact `(program, optname)` pair, qualified exactly like
    /// [`rinx_scope::ProgramScope::qualify`] so a lookup here and a
    /// definition's index key can never disagree about the key shape.
    fn lookup(&self, program: Option<&str>, optname: &str) -> Option<OptionResolution<'a>> {
        let qualified_name = match program {
            Some(program) => format!("{program}.{optname}"),
            None => optname.to_string(),
        };
        let object_type = ObjectType::Std(StdObjectType::Cmdoption);
        let doc_path = self
            .index
            .domain_objects
            .get(&TargetName::new(&qualified_name))
            .and_then(|entries| entries.get(&object_type))?;
        Some(OptionResolution::Resolved {
            qualified_name,
            doc_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_with_option(qualified_name: &str, doc_path: &str) -> ProjectIndex {
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Std(StdObjectType::Cmdoption),
            qualified_name,
            doc_path,
        );
        index
    }

    #[test]
    fn test_resolve_falls_back_to_another_sites_inventory() {
        // Given — no document here defines `-O`, but Python's inventory does
        let index = crate::test_support::index_linking_into_python();
        let resolver = OptionResolver::new(&index);

        // When
        let resolution = resolver.resolve(Some("python"), "-O", &rinx_ast::InventorySelector::Any);

        // Then
        let OptionResolution::External(hit) = resolution else {
            panic!("expected an external resolution, got {resolution:?}");
        };
        assert_eq!(hit.target.uri, "using/cmdline.html#cmdoption-O");
    }

    #[test]
    fn test_resolve_prefers_a_local_option_over_an_external_one() {
        // Given
        let mut index = crate::test_support::index_linking_into_python();
        index.insert_domain_object(ObjectType::Std(StdObjectType::Cmdoption), "-O", "cli.rst");
        let resolver = OptionResolver::new(&index);

        // When
        let resolution = resolver.resolve(None, "-O", &rinx_ast::InventorySelector::Any);

        // Then
        assert!(matches!(resolution, OptionResolution::Resolved { .. }));
    }

    #[test]
    fn test_resolve_finds_bare_option_with_no_ambient_program() {
        // Given
        let index = index_with_option("-x", "using/cmdline.rst");
        let resolver = OptionResolver::new(&index);

        // When
        let resolution = resolver.resolve(None, "-x", &rinx_ast::InventorySelector::Any);

        // Then
        assert_eq!(
            resolution,
            OptionResolution::Resolved {
                qualified_name: "-x".to_string(),
                doc_path: "using/cmdline.rst",
            }
        );
    }

    #[test]
    fn test_resolve_finds_option_qualified_by_ambient_program() {
        // Given
        let index = index_with_option("dis.-o", "library/dis.rst");
        let resolver = OptionResolver::new(&index);

        // When
        let resolution = resolver.resolve(Some("dis"), "-O", &rinx_ast::InventorySelector::Any);

        // Then — the returned `qualified_name` preserves the case the
        // reference was written with (`"dis.-O"`, not `"dis.-o"`); the index
        // lookup itself is case-insensitive via `TargetName`, and whatever
        // case survives here is re-normalized again when the anchor `id` is
        // built (`build_domain_object_key`), so this doesn't affect the
        // resolved link.
        assert_eq!(
            resolution,
            OptionResolution::Resolved {
                qualified_name: "dis.-O".to_string(),
                doc_path: "library/dis.rst",
            }
        );
    }

    #[test]
    fn test_resolve_falls_back_to_global_option_when_ambient_program_misses() {
        // Given — the confirmed `dis.rst` shape: `-X` is a *global* Python
        // option (registered with no program), referenced from a page whose
        // ambient program is `dis`.
        let index = index_with_option("-x", "using/cmdline.rst");
        let resolver = OptionResolver::new(&index);

        // When
        let resolution = resolver.resolve(Some("dis"), "-X", &rinx_ast::InventorySelector::Any);

        // Then — case preserved, same rationale as the test above.
        assert_eq!(
            resolution,
            OptionResolution::Resolved {
                qualified_name: "-X".to_string(),
                doc_path: "using/cmdline.rst",
            }
        );
    }

    #[test]
    fn test_resolve_peels_an_embedded_program_from_the_target() {
        // Given — the confirmed `ast.rst`/`dis.rst` cross-program shape:
        // `:option:`-O <dis --show-offsets>`` written from a page whose
        // ambient program is not `dis`.
        let index = index_with_option("dis.--show-offsets", "library/dis.rst");
        let resolver = OptionResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            None,
            "dis --show-offsets",
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(
            resolution,
            OptionResolution::Resolved {
                qualified_name: "dis.--show-offsets".to_string(),
                doc_path: "library/dis.rst",
            }
        );
    }

    #[test]
    fn test_resolve_peels_progressively_for_a_multi_word_program_name() {
        // Given — mirrors `.. program:: python -m py_compile`'s hyphen-joined
        // key, reached only after peeling both leading words.
        let index = index_with_option("python--m-py_compile.--quiet", "library/py_compile.rst");
        let resolver = OptionResolver::new(&index);

        // When
        let resolution = resolver.resolve(
            None,
            "python -m py_compile --quiet",
            &rinx_ast::InventorySelector::Any,
        );

        // Then
        assert_eq!(
            resolution,
            OptionResolution::Resolved {
                qualified_name: "python--m-py_compile.--quiet".to_string(),
                doc_path: "library/py_compile.rst",
            }
        );
    }

    #[test]
    fn test_resolve_returns_not_found_when_nothing_matches() {
        // Given
        let index = ProjectIndex::default();
        let resolver = OptionResolver::new(&index);

        // When
        let resolution = resolver.resolve(Some("dis"), "-Z", &rinx_ast::InventorySelector::Any);

        // Then
        assert_eq!(resolution, OptionResolution::NotFound);
    }

    #[test]
    fn test_resolve_does_not_peel_a_single_word_target() {
        // Given — no whitespace, so there is nothing to peel; a miss here
        // must not panic on an empty word list.
        let index = ProjectIndex::default();
        let resolver = OptionResolver::new(&index);

        // When
        let resolution = resolver.resolve(None, "-nonexistent", &rinx_ast::InventorySelector::Any);

        // Then
        assert_eq!(resolution, OptionResolution::NotFound);
    }
}

#[cfg(test)]
mod pipeline_tests {
    use crate::{BrokenLinkKind, render};
    use rinx_ast::{Directive, Document, InlineNode, Node};
    use rinx_index::ProjectIndex;

    #[test]
    fn test_render_option_reference_resolves_to_link() {
        // Given — resolution goes through the ambient-program tier, so the
        // reference is preceded by the matching `.. program::`.
        let doc = Document::new(
            "dis.rst".to_string(),
            vec![
                Node::Directive(Directive::StdProgram {
                    name: Some("dis".to_string()),
                }),
                Node::Paragraph(vec![InlineNode::OptionReference {
                    display: "-O".to_string(),
                    target: "-O".to_string(),
                    span: None,
                    inventory: rinx_ast::InventorySelector::Any,
                }]),
            ],
        );
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            rinx_ast::ObjectType::Std(rinx_ast::StdObjectType::Cmdoption),
            "dis.-o",
            "library/dis.rst",
        );

        // When
        let output = render(&doc, &index, &doc.path);

        // Then
        assert!(output.html.contains(
            "<a class=\"reference internal\" href=\"library/dis.html#std:cmdoption:dis.-o\">"
        ));
        assert!(output.broken_links.is_empty());
    }
    #[test]
    fn test_render_option_reference_not_found_produces_broken_link() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::OptionReference {
                display: "-Z".to_string(),
                target: "-Z".to_string(),
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }])],
        );
        let index = ProjectIndex::default();

        // When
        let output = render(&doc, &index, &doc.path);

        // Then
        assert!(output.html.contains("class=\"broken-link\""));
        assert_eq!(output.broken_links.len(), 1);
        assert_eq!(output.broken_links[0].kind, BrokenLinkKind::OptionReference);
        assert_eq!(output.broken_links[0].target, "-Z");
    }
}
