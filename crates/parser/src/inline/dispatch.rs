//! Turning one matched role or link into its [`InlineNode`]: the `kind`
//! dispatch, and the explicit-title (`Display text <target>`) split every
//! role that supports one shares.

use rusty_sphinx_ast::{Domain, InlineNode};

use super::regexes::{
    ANONYMOUS_PHRASED_REGEX, ANONYMOUS_SIMPLE_REGEX, EMBEDDED_URI_REGEX, PHRASED_LINK_REGEX,
    PROGRAM_ROLE_REGEX, REF_REGEX, SIMPLE_LINK_REGEX, TERM_ROLE_REGEX,
};
use super::roles::c::macro_::handle_macro_match;
use super::roles::c::struct_::handle_struct_match;
use super::roles::c::type_::handle_type_match;
use super::roles::c::union::handle_union_match;
use super::roles::py::attr::handle_attr_match;
use super::roles::py::class::handle_class_match;
use super::roles::py::data::handle_data_match;
use super::roles::py::exc::handle_exc_match;
use super::roles::py::func::handle_func_match;
use super::roles::py::meth::handle_meth_match;
use super::roles::py::mod_::handle_mod_match;
use super::roles::std_::option::handle_option_match;

/// Splits a role's backtick content on Sphinx's optional explicit-title
/// syntax (`Display text <target>`), shared by every role that supports it
/// (`:term:`, `:ref:`, and the domain-object roles via
/// [`parse_domain_object_target`]). Returns `None` when there is no explicit
/// title.
pub(super) fn split_explicit_title(content: &str) -> Option<(String, String)> {
    let angle_start = content.rfind('<')?;
    let angle_end = content[angle_start..].find('>')?;
    let display = content[..angle_start].trim().to_string();
    let target = content[angle_start + 1..angle_start + angle_end]
        .trim()
        .to_string();
    Some((display, target))
}

/// Splits a role's backtick content on Sphinx's optional explicit-title
/// syntax, shared by `:term:`/`:ref:`. Returns `(display, target)`, both
/// equal to `content` when there is no explicit title.
pub(super) fn split_display_and_target(content: &str) -> (String, String) {
    split_explicit_title(content).unwrap_or_else(|| (content.to_string(), content.to_string()))
}

pub(super) fn handle_inline_match(
    kind: &str,
    m_str: &str,
    node_opt: Option<InlineNode>,
    default_domain: Domain,
) -> InlineNode {
    match kind {
        "inline" => node_opt.expect("inline node should be present"),
        "ref" => {
            let caps = REF_REGEX.captures(m_str).unwrap();
            let (display, target) = split_display_and_target(&caps["target"]);
            InlineNode::Reference { display, target }
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
            InlineNode::TermReference { display, term }
        }
        "option" => handle_option_match(m_str),
        "phrased" => {
            let caps = PHRASED_LINK_REGEX.captures(m_str).unwrap();
            let text_full = &caps["text"];
            if let Some(embedded) = EMBEDDED_URI_REGEX.captures(text_full) {
                InlineNode::Hyperlink {
                    text: embedded["text"].trim().to_string(),
                    target: embedded["uri"].to_string(),
                }
            } else {
                InlineNode::Hyperlink {
                    text: text_full.to_string(),
                    target: text_full.to_string(),
                }
            }
        }
        "simple" => {
            let caps = SIMPLE_LINK_REGEX.captures(m_str).unwrap();
            let name = &caps["name"];
            InlineNode::Hyperlink {
                text: name.to_string(),
                target: name.to_string(),
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
                InlineNode::AnonymousReference(text_full.to_string())
            }
        }
        "anon_simple" => {
            let caps = ANONYMOUS_SIMPLE_REGEX.captures(m_str).unwrap();
            let name = &caps["name"];
            InlineNode::AnonymousReference(name.to_string())
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_inline_match_inline_variant() {
        // Given
        let node = InlineNode::Emphasis("text".to_string());
        // When
        let result = handle_inline_match("inline", "", Some(node.clone()), Domain::Py);
        // Then
        assert_eq!(result, node);
    }
    #[test]
    fn test_handle_inline_match_ref_variant() {
        let result = handle_inline_match("ref", ":ref:`target`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::Reference {
                display: "target".to_string(),
                target: "target".to_string(),
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
        );
        assert_eq!(
            result,
            InlineNode::Reference {
                display: "GenericAlias".to_string(),
                target: "types-genericalias".to_string(),
            }
        );
    }
    #[test]
    fn test_handle_inline_match_program_variant() {
        let result = handle_inline_match("program", ":program:`curl`", None, Domain::Py);
        assert_eq!(result, InlineNode::Program("curl".to_string()));
    }
    #[test]
    fn test_handle_inline_match_phrased_with_embedded_uri() {
        let result = handle_inline_match("phrased", "`text <http://uri>`_", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "text".to_string(),
                target: "http://uri".to_string(),
            }
        );
    }
    #[test]
    fn test_handle_inline_match_phrased_without_uri() {
        let result = handle_inline_match("phrased", "`just text`_", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "just text".to_string(),
                target: "just text".to_string(),
            }
        );
    }
    #[test]
    fn test_handle_inline_match_simple_variant() {
        let result = handle_inline_match("simple", "name_", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "name".to_string(),
                target: "name".to_string(),
            }
        );
    }
    #[test]
    fn test_handle_inline_match_anon_phrased_with_embedded_uri() {
        let result = handle_inline_match("anon_phrased", "`text <http://uri>`__", None, Domain::Py);
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
        let result = handle_inline_match("anon_phrased", "`anon text`__", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::AnonymousReference("anon text".to_string())
        );
    }
    #[test]
    fn test_handle_inline_match_anon_simple_variant() {
        let result = handle_inline_match("anon_simple", "anon_name__", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::AnonymousReference("anon_name".to_string())
        );
    }
}
