//! The `:index:` role: the anchor its general-index entries link to, then
//! its text, unlinked — the `target` and `Text` nodes of Sphinx's
//! `IndexRole`, whose third node (the entries) the general index renders.

use std::fmt::Write as _;

/// Renders an `:index:` role as Sphinx 9.1's HTML builder does: an empty
/// `<span class="target">` carrying the anchor, then the title as plain text.
pub(super) fn render_inline_index_reference(html: &mut String, title: &str, index_id: &str) {
    let _ = write!(
        html,
        "<span class=\"target\" id=\"{}\"></span>{}",
        html_escape::encode_double_quoted_attribute(index_id),
        html_escape::encode_text(title),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_inline_index_reference_writes_the_anchor_then_the_text() {
        // Given
        let mut html = String::new();

        // When
        render_inline_index_reference(&mut html, "loop", "index-3");

        // Then
        assert_eq!(html, "<span class=\"target\" id=\"index-3\"></span>loop");
    }

    #[test]
    fn test_render_inline_index_reference_escapes_the_text_and_the_anchor() {
        // Given
        let mut html = String::new();

        // When
        render_inline_index_reference(&mut html, "a < b", "\"x\"");

        // Then
        assert_eq!(
            html,
            "<span class=\"target\" id=\"&quot;x&quot;\"></span>a &lt; b"
        );
    }
}
