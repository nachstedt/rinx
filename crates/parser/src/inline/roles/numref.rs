//! The `:numref:` role, which shows the number of a figure, table, code block
//! or section.
//!
//! Flat under `roles/` for the reason `doc.rs` is: it belongs to the `std`
//! domain alone and takes no `default_domain`. Which element the label names,
//! and what number the project gave it, are decided while rendering; this
//! handler reads the markup and the explicit title's format, which is the one
//! part of the role that can be wrong on its own.

use rinx_ast::{InlineNode, NumberFormat, NumberReferenceRefusal};

use crate::explicit_title::split_optional_title;
use crate::inline::escapes::unescape;
use crate::inline::regexes::NUMREF_ROLE_REGEX;

/// Builds the `InlineNode` for a matched `:numref:` (or `:std:numref:`) role.
///
/// A leading `!` turns the reference into text that is never looked up, and —
/// as Sphinx's `XRefRole` does it — that text is everything after the `!`, an
/// angle-bracketed target included.
///
/// An explicit title is parsed into the [`NumberFormat`] it will be applied
/// as. One Sphinx could not apply — `see this <fig>`, with nowhere for the
/// number to go — and an `:external:` prefix both become a
/// [`InlineNode::RefusedNumberReference`], which the whole-document pass
/// reports at this role's span. The title is unescaped here, before it is
/// parsed, because Sphinx applies the format to the text docutils already
/// unescaped — `\%s` still marks the slot. Every other field is unescaped on
/// the way out, as for every other role.
pub(crate) fn handle_numref_match(m_str: &str) -> InlineNode {
    let caps = NUMREF_ROLE_REGEX.captures(m_str).unwrap();
    let content = &caps["target"];
    if !caps["external"].is_empty() {
        return refused(content, NumberReferenceRefusal::External);
    }
    if let Some(text) = content.strip_prefix('!') {
        return unlinked(text);
    }
    let (display, target) = split_optional_title(content);
    let Some(display) = display else {
        return InlineNode::NumberReference {
            title: None,
            target,
            link: true,
            span: None,
        };
    };
    match NumberFormat::parse(&unescape(&display)) {
        Ok(title) => InlineNode::NumberReference {
            title: Some(title),
            target,
            link: true,
            span: None,
        },
        Err(_) => refused(&display, NumberReferenceRefusal::InvalidTitle),
    }
}

/// A reference shown as `text` and never looked up.
fn unlinked(text: &str) -> InlineNode {
    InlineNode::NumberReference {
        title: None,
        target: text.to_string(),
        link: false,
        span: None,
    }
}

/// A reference the whole-document pass will report, then show as `text`.
fn refused(text: &str, refusal: NumberReferenceRefusal) -> InlineNode {
    InlineNode::RefusedNumberReference {
        text: text.to_string(),
        refusal,
        span: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numref(title: Option<&str>, target: &str) -> InlineNode {
        InlineNode::NumberReference {
            title: title.map(|text| NumberFormat::parse(text).expect("valid")),
            target: target.to_string(),
            link: true,
            span: None,
        }
    }

    #[test]
    fn test_handle_numref_match_leaves_a_bare_target_untitled() {
        // Given / When
        let node = handle_numref_match(":numref:`fig-root`");

        // Then
        assert_eq!(node, numref(None, "fig-root"));
    }

    #[test]
    fn test_handle_numref_match_accepts_the_std_spelling() {
        // Given / When
        let node = handle_numref_match(":std:numref:`fig-root`");

        // Then
        assert_eq!(node, numref(None, "fig-root"));
    }

    #[test]
    fn test_handle_numref_match_parses_an_explicit_title_as_a_format() {
        // Given / When
        let new_style = handle_numref_match(":numref:`Figure {number} ({name}) <fig-root>`");
        let old_style = handle_numref_match(":numref:`Fig %s here <fig-root>`");

        // Then
        assert_eq!(
            new_style,
            numref(Some("Figure {number} ({name})"), "fig-root")
        );
        assert_eq!(old_style, numref(Some("Fig %s here"), "fig-root"));
    }

    #[test]
    fn test_handle_numref_match_refuses_a_title_sphinx_could_not_apply() {
        // Given / When
        let node = handle_numref_match(":numref:`see this <fig-root>`");

        // Then — shown as the title alone, as Sphinx shows it
        assert_eq!(
            node,
            InlineNode::RefusedNumberReference {
                text: "see this".to_string(),
                refusal: NumberReferenceRefusal::InvalidTitle,
                span: None,
            }
        );
    }

    #[test]
    fn test_handle_numref_match_turns_a_bang_prefix_into_unlinked_text() {
        // Given / When
        let bare = handle_numref_match(":numref:`!fig-root`");
        let titled = handle_numref_match(":numref:`!Tit <fig-root>`");

        // Then
        assert_eq!(bare, unlinked("fig-root"));
        assert_eq!(titled, unlinked("Tit <fig-root>"));
    }

    #[test]
    fn test_handle_numref_match_refuses_an_external_prefix() {
        // Given / When
        let node = handle_numref_match(":external:numref:`fig-root`");

        // Then
        assert_eq!(
            node,
            InlineNode::RefusedNumberReference {
                text: "fig-root".to_string(),
                refusal: NumberReferenceRefusal::External,
                span: None,
            }
        );
    }
}
