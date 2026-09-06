//! Turning one matched role or link into its [`InlineNode`]: the `kind`
//! dispatch.
//!
//! The explicit-title (`Display text <target>`) split every role that supports
//! one shares used to live here; it moved to [`crate::explicit_title`] once
//! `.. toctree::` needed the same syntax on its entry lines.

use rusty_sphinx_ast::{Domain, InlineNode};

use crate::explicit_title::split_display_and_target;

use super::regexes::{
    ANONYMOUS_PHRASED_REGEX, ANONYMOUS_SIMPLE_REGEX, EMBEDDED_URI_REGEX, PHRASED_LINK_REGEX,
    PROGRAM_ROLE_REGEX, REF_REGEX, SIMPLE_LINK_REGEX, TERM_ROLE_REGEX,
};
use super::roles::c::macro_::handle_macro_match;
use super::roles::c::struct_::handle_struct_match;
use super::roles::c::type_::handle_type_match;
use super::roles::c::union::handle_union_match;
use super::roles::math::{handle_eq_match, handle_math_match};
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
    schema: &rusty_sphinx_entity::EntitySchema,
) -> InlineNode {
    match kind {
        "entity_role" => super::roles::entity::handle_entity_role_match(m_str, schema),
        "inline" => node_opt.expect("inline node should be present"),
        "ref" => {
            let caps = REF_REGEX.captures(m_str).unwrap();
            let (display, target) = split_display_and_target(&caps["target"]);
            InlineNode::Reference {
                display,
                target,
                span: None,
            }
        }
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
            }
        }
        "option" => handle_option_match(m_str),
        "math" => handle_math_match(m_str),
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
    use rusty_sphinx_entity::EntitySchema;

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
            &EntitySchema::empty(),
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
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::Reference {
                display: "target".to_string(),
                target: "target".to_string(),
                span: None
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
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::Reference {
                display: "GenericAlias".to_string(),
                target: "types-genericalias".to_string(),
                span: None
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
            &EntitySchema::empty(),
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
            &EntitySchema::empty(),
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
            &EntitySchema::empty(),
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
        let result =
            handle_inline_match("simple", "name_", None, Domain::Py, &EntitySchema::empty());
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
            &EntitySchema::empty(),
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
            &EntitySchema::empty(),
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
            &EntitySchema::empty(),
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
