use anyhow::{Result, anyhow};
use rusty_sphinx_ast::{Directive, Document, Node};
use rusty_sphinx_toctree::{UnmatchedKind, expand_toctree};
use std::collections::{BTreeSet, HashSet};

/// Validates that every toctree entry names a document the build declared as a
/// dependency.
///
/// The allowed names arrive from Bazel without their `.rst` extension (see
/// `rules/library.bzl`'s `local_doc_names`), so they are given one here: entry
/// resolution works in `.rst` paths throughout, and having one convention is
/// what lets this check share [`expand_toctree`] with the analyzer and the
/// renderer instead of resolving entries its own way.
///
/// Sharing that function is what makes `:glob:` enforceable. A pattern is
/// expanded against the *declared dependencies* here and against the whole
/// project later, so a glob that reaches an undeclared document still fails
/// the build — it simply matches nothing in this narrower universe.
///
/// `self` and external links name no dependency and are skipped, which
/// [`expand_toctree`] already does.
///
/// # Errors
///
/// Returns an error naming every entry that is not covered by `allowed_paths`.
pub fn validate_toctree<S: ::std::hash::BuildHasher>(
    doc: &Document,
    allowed_paths: &HashSet<String, S>,
) -> Result<()> {
    let universe: BTreeSet<String> = allowed_paths
        .iter()
        .map(|name| {
            if std::path::Path::new(name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rst"))
            {
                name.clone()
            } else {
                format!("{name}.rst")
            }
        })
        .collect();

    let mut errors = Vec::new();
    for node in &doc.nodes {
        let Node::Directive(Directive::Toctree(toctree)) = node else {
            continue;
        };
        let (_, unmatched) = expand_toctree(toctree, &doc.path, &universe);
        for entry in unmatched {
            let reason = match entry.kind {
                UnmatchedKind::MissingDocument => "is not explicitly declared as a dependency",
                UnmatchedKind::GlobMatchedNothing => "matches no explicitly declared dependency",
            };
            errors.push(format!(
                "Toctree entry '{}' in document '{}' {reason}.",
                entry.text, doc.path
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("Toctree validation failed:\n{}", errors.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A toctree of plain document entries, the shape almost every test here
    /// needs. Entry spans are irrelevant to these tests, so they are left
    /// unset rather than invented.
    fn toctree_of(docnames: &[&str]) -> rusty_sphinx_ast::Toctree {
        rusty_sphinx_ast::Toctree {
            entries: docnames
                .iter()
                .map(|docname| rusty_sphinx_ast::TocEntry::Document {
                    title: None,
                    docname: (*docname).to_string(),
                    span: None,
                })
                .collect(),
            options: rusty_sphinx_ast::ToctreeOptions::default(),
        }
    }

    /// A toctree of one `:glob:` pattern.
    fn globbing_toctree(pattern: &str) -> rusty_sphinx_ast::Toctree {
        let mut options = rusty_sphinx_ast::ToctreeOptions::default();
        options.set(rusty_sphinx_ast::ToctreeFlag::Glob);
        rusty_sphinx_ast::Toctree {
            entries: vec![rusty_sphinx_ast::TocEntry::Glob {
                pattern: pattern.to_string(),
                span: None,
            }],
            options,
        }
    }

    // The path-resolution cases this module used to test moved with the
    // function itself: `rusty_sphinx_toctree::resolve_docname` is now the one
    // implementation, and its own tests cover them.

    #[test]
    fn test_validate_toctree_success() {
        let doc = Document::new(
            "docs/index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree(toctree_of(&[
                "team_a/index",
            ])))],
        );
        let mut allowed = HashSet::new();
        allowed.insert("docs/team_a/index".to_string());

        assert!(validate_toctree(&doc, &allowed).is_ok());
    }

    #[test]
    fn test_validate_toctree_failure() {
        let doc = Document::new(
            "docs/index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree(toctree_of(&[
                "team_a/index",
            ])))],
        );
        let allowed = HashSet::new(); // empty

        let result = validate_toctree(&doc, &allowed);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not explicitly declared as a dependency"));
        assert!(err_msg.contains("team_a/index"));
    }

    #[test]
    fn test_validate_toctree_success_with_rst_suffix() {
        let doc = Document::new(
            "docs/index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree(toctree_of(&[
                "team_a/index.rst",
            ])))],
        );
        let mut allowed = HashSet::new();
        // The allowed paths are stripped of .rst by the bazel rule, so we test with the stripped path.
        allowed.insert("docs/team_a/index".to_string());

        assert!(validate_toctree(&doc, &allowed).is_ok());
    }

    #[test]
    fn test_validate_toctree_empty_paths() {
        let doc = Document::new(
            "docs/index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree(toctree_of(&[])))],
        );
        let allowed = HashSet::new(); // empty

        assert!(validate_toctree(&doc, &allowed).is_ok());
    }

    #[test]
    fn test_validate_toctree_accepts_a_glob_matching_a_declared_dependency() {
        // Given — a pattern covering a document the build declared.
        let doc = Document::new(
            "docs/index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree(globbing_toctree(
                "team_a/*",
            )))],
        );
        let mut allowed = HashSet::new();
        allowed.insert("docs/team_a/index".to_string());

        // When / Then
        assert!(validate_toctree(&doc, &allowed).is_ok());
    }

    #[test]
    fn test_validate_toctree_rejects_a_glob_matching_no_declared_dependency() {
        // Given — the pattern is legal, but nothing declared satisfies it.
        // This is the case that would go unchecked if globs were expanded
        // against the whole project instead of against the declared deps.
        let doc = Document::new(
            "docs/index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree(globbing_toctree(
                "team_b/*",
            )))],
        );
        let mut allowed = HashSet::new();
        allowed.insert("docs/team_a/index".to_string());

        // When
        let result = validate_toctree(&doc, &allowed);

        // Then
        let err = result.expect_err("an undeclared glob must fail the build");
        assert!(err.to_string().contains("team_b/*"), "{err}");
    }

    #[test]
    fn test_validate_toctree_ignores_self_and_external_entries() {
        // Given — neither names a document, so neither needs declaring.
        let doc = Document::new(
            "docs/index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree(
                rusty_sphinx_ast::Toctree {
                    entries: vec![
                        rusty_sphinx_ast::TocEntry::SelfRef {
                            title: None,
                            span: None,
                        },
                        rusty_sphinx_ast::TocEntry::External {
                            title: None,
                            url: "https://example.org".to_string(),
                            span: None,
                        },
                    ],
                    options: rusty_sphinx_ast::ToctreeOptions::default(),
                },
            ))],
        );
        let allowed = HashSet::new(); // empty

        // When / Then
        assert!(validate_toctree(&doc, &allowed).is_ok());
    }
}
