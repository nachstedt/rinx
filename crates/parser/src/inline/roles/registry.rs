//! The registry roles — `:pep:`, `:rfc:`, `:cve:` and `:cwe:` — which link a
//! numbered document outside the site and index the place it was mentioned.
//!
//! Flat under `roles/` for the reason `download.rs` is: they name something
//! outside every domain — here a page outside the site altogether. The one
//! part of such a role that can be wrong on its own is the target, so that
//! is what this handler checks, by its registry's rules; the anchor its index
//! entry links to is minted once the whole document is parsed, and the URL
//! only while rendering.

use rinx_ast::{InlineNode, Registry, RegistryTarget, RoleRefusal};

use crate::explicit_title::split_optional_title;
use crate::inline::escapes::unescape;
use crate::inline::regexes::REGISTRY_ROLE_REGEX;

/// Builds the `InlineNode` for a matched registry role.
///
/// The target is unescaped before it is read, since Sphinx hands `int()` the
/// text docutils already unescaped. A target its registry cannot link
/// becomes an [`InlineNode::RefusedRole`] showing the role's source, which
/// the whole-document pass reports at this role's span. There is no `!`
/// form: Sphinx's roles have none, and `!8` is not a number.
pub(crate) fn handle_registry_match(m_str: &str) -> InlineNode {
    let caps = REGISTRY_ROLE_REGEX.captures(m_str).unwrap();
    let registry = registry_named(&caps["registry"]);
    let (display, target) = split_optional_title(&caps["target"]);
    let target = unescape(&target);
    match RegistryTarget::parse(registry, &target) {
        Ok(target) => InlineNode::RegistryReference {
            target,
            display,
            index_id: String::new(),
            span: None,
        },
        Err(_) => InlineNode::RefusedRole {
            text: m_str.to_string(),
            refusal: RoleRefusal::RegistryTarget { registry, target },
            span: None,
        },
    }
}

/// The registry whose role is named `name`, as [`REGISTRY_ROLE_REGEX`]
/// captured it.
fn registry_named(name: &str) -> Registry {
    Registry::ALL
        .into_iter()
        .find(|registry| registry.role_name() == name)
        .unwrap_or_else(|| unreachable!("the registry regex matched an unknown name {name:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(registry: Registry, target: &str, display: Option<&str>) -> InlineNode {
        InlineNode::RegistryReference {
            target: RegistryTarget::parse(registry, target).unwrap(),
            display: display.map(str::to_string),
            index_id: String::new(),
            span: None,
        }
    }

    fn refused(text: &str, registry: Registry, target: &str) -> InlineNode {
        InlineNode::RefusedRole {
            text: text.to_string(),
            refusal: RoleRefusal::RegistryTarget {
                registry,
                target: target.to_string(),
            },
            span: None,
        }
    }

    #[test]
    fn test_handle_registry_match_reads_a_bare_number() {
        // Given / When
        let node = handle_registry_match(":pep:`8`");

        // Then
        assert_eq!(node, reference(Registry::Pep, "8", None));
    }

    #[test]
    fn test_handle_registry_match_keeps_a_fragment_in_the_target() {
        // Given / When
        let node = handle_registry_match(":rfc:`2324#section-2.3`");

        // Then
        assert_eq!(node, reference(Registry::Rfc, "2324#section-2.3", None));
    }

    #[test]
    fn test_handle_registry_match_splits_an_explicit_title() {
        // Given / When
        let node = handle_registry_match(":cwe:`Out-of-bounds Write <787>`");

        // Then
        assert_eq!(
            node,
            reference(Registry::Cwe, "787", Some("Out-of-bounds Write"))
        );
    }

    #[test]
    fn test_handle_registry_match_reads_a_cve_identifier() {
        // Given / When
        let node = handle_registry_match(":cve:`2024-3094`");

        // Then
        assert_eq!(node, reference(Registry::Cve, "2024-3094", None));
    }

    #[test]
    fn test_handle_registry_match_refuses_by_the_registry_rules() {
        // Given / When / Then — the target kept for the diagnostic
        assert_eq!(
            handle_registry_match(":pep:`Style <abc>`"),
            refused(":pep:`Style <abc>`", Registry::Pep, "abc")
        );
        assert_eq!(
            handle_registry_match(":cve:`CVE-2024-3094`"),
            refused(":cve:`CVE-2024-3094`", Registry::Cve, "CVE-2024-3094")
        );
        assert_eq!(
            handle_registry_match(":rfc:`HTTP`"),
            refused(":rfc:`HTTP`", Registry::Rfc, "HTTP")
        );
    }

    #[test]
    fn test_handle_registry_match_has_no_bang_form() {
        // Given / When
        let node = handle_registry_match(":pep:`!8`");

        // Then
        assert!(matches!(node, InlineNode::RefusedRole { .. }), "{node:?}");
    }

    #[test]
    fn test_registry_named_finds_each_registry() {
        // Given / When / Then
        for registry in Registry::ALL {
            assert_eq!(registry_named(registry.role_name()), registry);
        }
    }
}
