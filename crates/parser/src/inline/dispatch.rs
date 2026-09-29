//! Turning one matched role or link into its [`InlineNode`]: the `kind`
//! dispatch.
//!
//! The explicit-title (`Display text <target>`) split every role that supports
//! one shares used to live here; it moved to [`crate::explicit_title`] once
//! `.. toctree::` needed the same syntax on its entry lines.

use rinx_ast::{Domain, InlineNode, InventoryName, InventorySelector};

use crate::context::ParseCtx;

use crate::explicit_title::{split_display_and_target, split_optional_title};

use super::regexes::{
    ANONYMOUS_PHRASED_REGEX, ANONYMOUS_SIMPLE_REGEX, EMBEDDED_URI_REGEX, EXTERNAL_PREFIX_REGEX,
    PHRASED_LINK_REGEX, PROGRAM_ROLE_REGEX, REF_REGEX, SIMPLE_LINK_REGEX, TERM_ROLE_REGEX,
};
use super::roles::any::handle_any_match;
use super::roles::c::macro_::handle_macro_match;
use super::roles::c::struct_::handle_struct_match;
use super::roles::c::type_::handle_type_match;
use super::roles::c::union::handle_union_match;
use super::roles::code::{handle_code_match, handle_custom_role_match};
use super::roles::doc::handle_doc_match;
use super::roles::download::handle_download_match;
use super::roles::math::{handle_eq_match, handle_math_match};
use super::roles::numref::handle_numref_match;
use super::roles::pep::handle_pep_match;
use super::roles::py::attr::handle_attr_match;
use super::roles::py::class::handle_class_match;
use super::roles::py::data::handle_data_match;
use super::roles::py::exc::handle_exc_match;
use super::roles::py::func::handle_func_match;
use super::roles::py::meth::handle_meth_match;
use super::roles::py::mod_::handle_mod_match;
use super::roles::std_::option::handle_option_match;

pub(super) fn handle_inline_match(
    kind: &str,
    m_str: &str,
    node_opt: Option<InlineNode>,
    default_domain: Domain,
    ctx: &ParseCtx<'_>,
) -> InlineNode {
    let node = build_inline_node(kind, m_str, node_opt, default_domain, ctx);
    apply_inventory_selector(node, inventory_selector(m_str))
}

/// What a role's `external:`/`external+name:` prefix asks for — every role
/// that may carry one shares the one prefix pattern, so it is read here once
/// rather than by each role's handler.
fn inventory_selector(m_str: &str) -> InventorySelector {
    match EXTERNAL_PREFIX_REGEX.captures(m_str) {
        None => InventorySelector::Any,
        Some(caps) => match caps.name("inventory") {
            // The prefix pattern admits only what `InventoryName` accepts, so
            // the fallback is unreachable; degrading to "any inventory" is
            // still better than a panic in a parser that must not have one.
            Some(name) => InventoryName::new(name.as_str())
                .map_or(InventorySelector::ExternalOnly, InventorySelector::Named),
            None => InventorySelector::ExternalOnly,
        },
    }
}

/// Records `selector` on a cross-reference node. Any other node — a role
/// that fell back to plain text, say — is returned unchanged, since it has
/// nothing to resolve.
fn apply_inventory_selector(node: InlineNode, selector: InventorySelector) -> InlineNode {
    match node {
        InlineNode::Reference {
            display,
            target,
            span,
            ..
        } => InlineNode::Reference {
            display,
            target,
            span,
            inventory: selector,
        },
        InlineNode::AnyReference {
            display,
            target,
            link,
            span,
            ..
        } => InlineNode::AnyReference {
            display,
            target,
            link,
            span,
            inventory: selector,
        },
        InlineNode::DocReference {
            display,
            target,
            link,
            span,
            ..
        } => InlineNode::DocReference {
            display,
            target,
            link,
            span,
            inventory: selector,
        },
        InlineNode::TermReference {
            display,
            term,
            span,
            ..
        } => InlineNode::TermReference {
            display,
            term,
            span,
            inventory: selector,
        },
        InlineNode::OptionReference {
            display,
            target,
            span,
            ..
        } => InlineNode::OptionReference {
            display,
            target,
            span,
            inventory: selector,
        },
        InlineNode::DomainObjectReference {
            object_type,
            name,
            display,
            link,
            search_order,
            span,
            ..
        } => InlineNode::DomainObjectReference {
            object_type,
            name,
            display,
            link,
            search_order,
            span,
            inventory: selector,
        },
        other => other,
    }
}

/// Builds the node for one matched role or link, dispatching on its `kind`.
fn build_inline_node(
    kind: &str,
    m_str: &str,
    node_opt: Option<InlineNode>,
    default_domain: Domain,
    ctx: &ParseCtx<'_>,
) -> InlineNode {
    match kind {
        // The schema is asked first, so a custom role can never take over an
        // entity role — `.. role::` refuses such a name, but a document's
        // role names are case-insensitive where the schema's are not. Anything
        // neither knows stays the text it was written as.
        "named_role" => super::roles::entity::handle_entity_role_match(m_str, ctx.schema)
            .or_else(|| handle_custom_role_match(m_str, ctx.custom_roles()))
            .unwrap_or_else(|| InlineNode::Text(m_str.to_string())),
        "inline" => node_opt.expect("inline node should be present"),
        "ref" => {
            let caps = REF_REGEX.captures(m_str).unwrap();
            let (display, target) = split_optional_title(&caps["target"]);
            InlineNode::Reference {
                display,
                target,
                span: None,
                inventory: InventorySelector::Any,
            }
        }
        "any" => handle_any_match(m_str),
        "doc" => handle_doc_match(m_str),
        "download" => handle_download_match(m_str),
        "numref" => handle_numref_match(m_str),
        "pep" => handle_pep_match(m_str),
        "program" => {
            let caps = PROGRAM_ROLE_REGEX.captures(m_str).unwrap();
            InlineNode::Program(caps["name"].to_string())
        }
        "func" => handle_func_match(m_str, default_domain),
        "mod" => handle_mod_match(m_str, default_domain),
        "data" => handle_data_match(m_str, default_domain),
        "meth" => handle_meth_match(m_str, default_domain),
        "class" => handle_class_match(m_str, default_domain),
        "attr" => handle_attr_match(m_str, default_domain),
        "exc" => handle_exc_match(m_str, default_domain),
        "macro" => handle_macro_match(m_str, default_domain),
        "struct" => handle_struct_match(m_str, default_domain),
        "union" => handle_union_match(m_str, default_domain),
        "type" => handle_type_match(m_str, default_domain),
        "term" => {
            let caps = TERM_ROLE_REGEX.captures(m_str).unwrap();
            let (display, term) = split_display_and_target(&caps["content"]);
            InlineNode::TermReference {
                display,
                term,
                span: None,
                inventory: InventorySelector::Any,
            }
        }
        "option" => handle_option_match(m_str),
        "math" => handle_math_match(m_str),
        "code" => handle_code_match(m_str),
        "eq" => handle_eq_match(m_str),
        "phrased" => {
            let caps = PHRASED_LINK_REGEX.captures(m_str).unwrap();
            let text_full = &caps["text"];
            if let Some(embedded) = EMBEDDED_URI_REGEX.captures(text_full) {
                InlineNode::Hyperlink {
                    text: embedded["text"].trim().to_string(),
                    target: embedded["uri"].to_string(),
                    span: None,
                }
            } else {
                InlineNode::Hyperlink {
                    text: text_full.to_string(),
                    target: text_full.to_string(),
                    span: None,
                }
            }
        }
        "simple" => {
            let caps = SIMPLE_LINK_REGEX.captures(m_str).unwrap();
            let name = &caps["name"];
            InlineNode::Hyperlink {
                text: name.to_string(),
                target: name.to_string(),
                span: None,
            }
        }
        "anon_phrased" => {
            let caps = ANONYMOUS_PHRASED_REGEX.captures(m_str).unwrap();
            let text_full = &caps["text"];
            if let Some(embedded) = EMBEDDED_URI_REGEX.captures(text_full) {
                InlineNode::AnonymousHyperlink {
                    text: embedded["text"].trim().to_string(),
                    target: embedded["uri"].to_string(),
                }
            } else {
                InlineNode::AnonymousReference {
                    text: text_full.to_string(),
                    span: None,
                }
            }
        }
        "anon_simple" => {
            let caps = ANONYMOUS_SIMPLE_REGEX.captures(m_str).unwrap();
            let name = &caps["name"];
            InlineNode::AnonymousReference {
                text: name.to_string(),
                span: None,
            }
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inventory_selector_is_any_without_a_prefix() {
        // Given / When / Then
        assert_eq!(inventory_selector(":ref:`x`"), InventorySelector::Any);
    }

    #[test]
    fn test_inventory_selector_reads_a_bare_external_prefix() {
        // Given / When / Then
        assert_eq!(
            inventory_selector(":external:ref:`x`"),
            InventorySelector::ExternalOnly
        );
    }

    #[test]
    fn test_inventory_selector_reads_a_named_external_prefix() {
        // Given / When / Then
        assert_eq!(
            inventory_selector(":external+numpy:py:class:`ndarray`"),
            InventorySelector::Named(InventoryName::new("numpy").unwrap())
        );
    }

    #[test]
    fn test_apply_inventory_selector_sets_it_on_a_reference() {
        // Given
        let node = InlineNode::Reference {
            display: None,
            target: "x".to_string(),
            span: None,
            inventory: InventorySelector::Any,
        };

        // When
        let node = apply_inventory_selector(node, InventorySelector::ExternalOnly);

        // Then
        assert!(matches!(
            node,
            InlineNode::Reference {
                inventory: InventorySelector::ExternalOnly,
                ..
            }
        ));
    }

    #[test]
    fn test_apply_inventory_selector_leaves_other_nodes_alone() {
        // Given — a role that fell back to plain text
        let node = InlineNode::Text(":class:`Greeter`".to_string());

        // When
        let result = apply_inventory_selector(node.clone(), InventorySelector::ExternalOnly);

        // Then
        assert_eq!(result, node);
    }

    #[test]
    fn test_handle_inline_match_inline_variant() {
        // Given
        let node = InlineNode::Emphasis("text".to_string());
        // When
        let result = handle_inline_match(
            "inline",
            "",
            Some(node.clone()),
            Domain::Py,
            &ParseCtx::with_domain(Domain::Py),
        );
        // Then
        assert_eq!(result, node);
    }
    #[test]
    fn test_handle_inline_match_ref_variant() {
        let result = handle_inline_match(
            "ref",
            ":ref:`target`",
            None,
            Domain::Py,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert_eq!(
            result,
            InlineNode::Reference {
                display: None,
                target: "target".to_string(),
                span: None,
                inventory: InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_ref_variant_with_display_text() {
        let result = handle_inline_match(
            "ref",
            ":ref:`GenericAlias <types-genericalias>`",
            None,
            Domain::Py,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert_eq!(
            result,
            InlineNode::Reference {
                display: Some("GenericAlias".to_string()),
                target: "types-genericalias".to_string(),
                span: None,
                inventory: InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_program_variant() {
        let result = handle_inline_match(
            "program",
            ":program:`curl`",
            None,
            Domain::Py,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert_eq!(result, InlineNode::Program("curl".to_string()));
    }
    #[test]
    fn test_handle_inline_match_phrased_with_embedded_uri() {
        let result = handle_inline_match(
            "phrased",
            "`text <http://uri>`_",
            None,
            Domain::Py,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "text".to_string(),
                target: "http://uri".to_string(),
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_phrased_without_uri() {
        let result = handle_inline_match(
            "phrased",
            "`just text`_",
            None,
            Domain::Py,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "just text".to_string(),
                target: "just text".to_string(),
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_simple_variant() {
        let result = handle_inline_match(
            "simple",
            "name_",
            None,
            Domain::Py,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "name".to_string(),
                target: "name".to_string(),
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_anon_phrased_with_embedded_uri() {
        let result = handle_inline_match(
            "anon_phrased",
            "`text <http://uri>`__",
            None,
            Domain::Py,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert_eq!(
            result,
            InlineNode::AnonymousHyperlink {
                text: "text".to_string(),
                target: "http://uri".to_string(),
            }
        );
    }
    #[test]
    fn test_handle_inline_match_anon_phrased_without_uri() {
        let result = handle_inline_match(
            "anon_phrased",
            "`anon text`__",
            None,
            Domain::Py,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert_eq!(
            result,
            InlineNode::AnonymousReference {
                text: "anon text".to_string(),
                span: None,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_anon_simple_variant() {
        let result = handle_inline_match(
            "anon_simple",
            "anon_name__",
            None,
            Domain::Py,
            &ParseCtx::with_domain(Domain::Py),
        );
        assert_eq!(
            result,
            InlineNode::AnonymousReference {
                text: "anon_name".to_string(),
                span: None,
            }
        );
    }
}
