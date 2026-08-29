//! Broken-link / object-type-mismatch warning formatting and the
//! `--strict-links` enforcement, shared by the `render` and `preview`
//! subcommands.

use anyhow::{Result, anyhow};
use rusty_sphinx_renderer::{self as renderer};

/// Formats a single broken-link diagnostic as a human-readable warning line.
///
/// For a broken domain-object reference the role's requested object type (the
/// "missed type", e.g. `py:function`) is included — it's known at the point
/// resolution failed and pinpoints what kind of object couldn't be found. An
/// ambiguous reference additionally lists the qualified names it matched:
/// unlike a plain miss, the fix is to pick one of them, so they are the
/// actionable part of the message.
pub(super) fn format_broken_link_warning(doc_path: &str, link: &renderer::BrokenLink) -> String {
    let requested = match &link.kind {
        renderer::BrokenLinkKind::DomainObjectReference(object_type) => {
            format!(" (referenced as {})", object_type.domain_qualified_str())
        }
        renderer::BrokenLinkKind::AmbiguousDomainObjectReference {
            object_type,
            candidates,
        } => format!(
            " (referenced as {}, matches {})",
            object_type.domain_qualified_str(),
            candidates.join(", ")
        ),
        _ => String::new(),
    };
    format!(
        "warning: broken {} '{}'{requested} in {doc_path}",
        link.kind.as_str(),
        link.target
    )
}

/// Formats a single object-type-mismatch diagnostic as a human-readable
/// warning line. Both the requested and resolved object types are shown
/// domain-qualified (e.g. `"py:class"`, not just `"class"`) via
/// [`rusty_sphinx_ast::ObjectType::domain_qualified_str`] — the alias
/// fallback is domain-scoped today (`py`'s `class`/`exception`, and `c`'s
/// `macro`/`member` and `function`/`macro`), so the two domains always match
/// in practice, but spelling both out avoids the reader having to assume
/// that rather than see it.
/// Unlike [`format_broken_link_warning`], this never feeds into
/// [`check_broken_links_strict`] — the reference did resolve, so `--strict-links`
/// never fails the build for it; the warning only flags that the reference's
/// role (e.g. `:exc:`) and the definition's actual object type (e.g. `class`)
/// are inconsistent.
pub(super) fn format_object_type_mismatch_warning(
    doc_path: &str,
    mismatch: &renderer::ObjectTypeMismatch,
) -> String {
    format!(
        "warning: domain object '{}' referenced as '{}' but defined as '{}' in {doc_path}",
        mismatch.name,
        mismatch.requested_type.domain_qualified_str(),
        mismatch.resolved_type.domain_qualified_str(),
    )
}

/// Returns an error listing every broken link when `strict` is true and
/// `broken_links` is non-empty. Diagnostics are always reported to stderr by
/// the caller regardless of `strict` — this only controls whether they also
/// fail the render.
pub(super) fn check_broken_links_strict(
    strict: bool,
    doc_path: &str,
    broken_links: &[renderer::BrokenLink],
) -> Result<()> {
    if !strict || broken_links.is_empty() {
        return Ok(());
    }
    let messages: Vec<String> = broken_links
        .iter()
        .map(|link| format_broken_link_warning(doc_path, link))
        .collect();
    Err(anyhow!(
        "Broken link validation failed:\n{}",
        messages.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_broken_link_warning_includes_kind_target_and_doc_path() {
        // Given
        let link = renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing-section".to_string(),
        };

        // When
        let message = format_broken_link_warning("guide/intro.rst", &link);

        // Then
        assert_eq!(
            message,
            "warning: broken ref 'missing-section' in guide/intro.rst"
        );
    }

    #[test]
    fn test_check_broken_links_strict_passes_when_not_strict() {
        // Given
        let broken_links = vec![renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing".to_string(),
        }];

        // When
        let result = check_broken_links_strict(false, "doc.rst", &broken_links);

        // Then
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_broken_links_strict_passes_when_no_broken_links() {
        // Given
        let broken_links: Vec<renderer::BrokenLink> = vec![];

        // When
        let result = check_broken_links_strict(true, "doc.rst", &broken_links);

        // Then
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_broken_links_strict_fails_when_strict_and_broken_links_present() {
        // Given
        let broken_links = vec![renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing".to_string(),
        }];

        // When
        let result = check_broken_links_strict(true, "doc.rst", &broken_links);

        // Then
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Broken link validation failed"));
        assert!(msg.contains("missing"));
    }

    #[test]
    fn test_format_object_type_mismatch_warning_includes_name_and_types() {
        // Given
        let mismatch = renderer::ObjectTypeMismatch {
            name: "fault".to_string(),
            requested_type: rusty_sphinx_ast::ObjectType::Py(
                rusty_sphinx_ast::PyObjectType::Exception,
            ),
            resolved_type: rusty_sphinx_ast::ObjectType::Py(rusty_sphinx_ast::PyObjectType::Class),
        };

        // When
        let message = format_object_type_mismatch_warning("xmlrpc.client.rst", &mismatch);

        // Then
        assert_eq!(
            message,
            "warning: domain object 'fault' referenced as 'py:exception' but defined as 'py:class' in xmlrpc.client.rst"
        );
    }
}
