//! Resolving a `:option:` cross-reference against the project index.
//!
//! Kept apart from [`crate::domain_resolution`] even though both ultimately
//! read `ProjectIndex::domain_objects`: that resolver's whole design is built
//! around `rusty_sphinx_scope::Scope`'s per-domain tiered candidate lists
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

use rusty_sphinx_ast::{ObjectType, StdObjectType, TargetName};
use rusty_sphinx_index::ProjectIndex;

/// The outcome of resolving one `:option:` reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OptionResolution<'a> {
    Resolved {
        qualified_name: String,
        doc_path: &'a str,
    },
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

    /// Resolves `target` (the role's text, minus any explicit-title
    /// override, which is handled at parse time) written under
    /// `ambient_program` — see the module doc comment for the search order.
    pub(crate) fn resolve(
        &self,
        ambient_program: Option<&str>,
        target: &str,
    ) -> OptionResolution<'a> {
        if let Some(resolution) = self.lookup(ambient_program, target) {
            return resolution;
        }
        if ambient_program.is_some()
            && let Some(resolution) = self.lookup(None, target)
        {
            return resolution;
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
                    return resolution;
                }
            }
        }
        OptionResolution::NotFound
    }

    /// Looks up one exact `(program, optname)` pair, qualified exactly like
    /// [`rusty_sphinx_scope::ProgramScope::qualify`] so a lookup here and a
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
    fn test_resolve_finds_bare_option_with_no_ambient_program() {
        // Given
        let index = index_with_option("-x", "using/cmdline.rst");
        let resolver = OptionResolver::new(&index);

        // When
        let resolution = resolver.resolve(None, "-x");

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
        let resolution = resolver.resolve(Some("dis"), "-O");

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
        let resolution = resolver.resolve(Some("dis"), "-X");

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
        let resolution = resolver.resolve(None, "dis --show-offsets");

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
        let resolution = resolver.resolve(None, "python -m py_compile --quiet");

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
        let resolution = resolver.resolve(Some("dis"), "-Z");

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
        let resolution = resolver.resolve(None, "-nonexistent");

        // Then
        assert_eq!(resolution, OptionResolution::NotFound);
    }
}
