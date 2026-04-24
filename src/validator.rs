use crate::ast::{Directive, Document, Node};
use anyhow::{Result, anyhow};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

/// Normalizes a path, resolving `.` and `..` components.
fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            _ => normalized.push(component),
        }
    }
    normalized
}

/// Resolves a `toctree` entry relative to the current document's path.
fn resolve_relative_path(doc_path: &str, toctree_entry: &str) -> String {
    let doc_parent = Path::new(doc_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let combined = doc_parent.join(toctree_entry);
    let normalized = normalize_path(&combined);
    // Convert to string and replace backslashes (Windows) with forward slashes
    normalized.to_string_lossy().replace('\\', "/")
}

/// Validates that all toctree entries in the document are present in the allowed set.
///
/// # Errors
///
/// Returns an error if any toctree entry points to a path that is not in `allowed_paths`.
pub fn validate_toctree<S: ::std::hash::BuildHasher>(
    doc: &Document,
    allowed_paths: &HashSet<String, S>,
) -> Result<()> {
    let mut errors = Vec::new();

    for node in &doc.nodes {
        if let Node::Directive(Directive::Toctree { paths }) = node {
            for path in paths {
                let resolved = resolve_relative_path(&doc.path, path);
                if !allowed_paths.contains(&resolved) {
                    errors.push(format!(
                        "Toctree entry '{}' (resolved to '{}') in document '{}' is not explicitly declared as a dependency.",
                        path, resolved, doc.path
                    ));
                }
            }
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

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path(Path::new("a/b/../c")), PathBuf::from("a/c"));
        assert_eq!(normalize_path(Path::new("a/./b")), PathBuf::from("a/b"));
        assert_eq!(normalize_path(Path::new("a/b/c")), PathBuf::from("a/b/c"));
    }

    #[test]
    fn test_resolve_relative_path() {
        assert_eq!(
            resolve_relative_path("docs/index.rst", "team_a/index"),
            "docs/team_a/index"
        );
        assert_eq!(
            resolve_relative_path("docs/team_a/index.rst", "../team_b/index"),
            "docs/team_b/index"
        );
        assert_eq!(
            resolve_relative_path("index.rst", "team_a/index"),
            "team_a/index"
        );
    }

    #[test]
    fn test_validate_toctree_success() {
        let doc = Document::new(
            "docs/index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string()],
            })],
        );
        let mut allowed = HashSet::new();
        allowed.insert("docs/team_a/index".to_string());

        assert!(validate_toctree(&doc, &allowed).is_ok());
    }

    #[test]
    fn test_validate_toctree_failure() {
        let doc = Document::new(
            "docs/index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string()],
            })],
        );
        let allowed = HashSet::new(); // empty

        let result = validate_toctree(&doc, &allowed);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not explicitly declared as a dependency"));
        assert!(err_msg.contains("team_a/index"));
    }
}
