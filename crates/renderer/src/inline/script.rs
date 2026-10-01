//! `:sub:`/`:subscript:`, `:sup:`/`:superscript:` and the roles derived from
//! them: text in a `<sub>` or `<sup>`, as docutils' HTML writer emits it.

use std::fmt::Write as _;

use rinx_ast::ScriptPosition;

/// Writes `text` in the element `position` names, carrying `classes` — a
/// derived role's — when there are any.
pub(super) fn render_inline_script(
    html: &mut String,
    position: ScriptPosition,
    text: &str,
    classes: &[String],
) {
    let tag = position.html_tag();
    let _ = write!(html, "<{tag}");
    if !classes.is_empty() {
        let _ = write!(
            html,
            " class=\"{}\"",
            html_escape::encode_double_quoted_attribute(&classes.join(" "))
        );
    }
    let _ = write!(html, ">{}</{tag}>", html_escape::encode_text(text));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(position: ScriptPosition, text: &str, classes: &[&str]) -> String {
        let classes: Vec<String> = classes.iter().map(ToString::to_string).collect();
        let mut html = String::new();
        render_inline_script(&mut html, position, text, &classes);
        html
    }

    #[test]
    fn test_render_inline_script_writes_a_subscript() {
        // Given / When / Then
        assert_eq!(render(ScriptPosition::Subscript, "2", &[]), "<sub>2</sub>");
    }

    #[test]
    fn test_render_inline_script_writes_a_superscript() {
        // Given / When / Then
        assert_eq!(
            render(ScriptPosition::Superscript, "th", &[]),
            "<sup>th</sup>"
        );
    }

    #[test]
    fn test_render_inline_script_writes_a_derived_role_s_classes() {
        // Given / When / Then
        assert_eq!(
            render(ScriptPosition::Subscript, "2", &["chem", "formula"]),
            "<sub class=\"chem formula\">2</sub>"
        );
    }

    #[test]
    fn test_render_inline_script_escapes_the_text() {
        // Given / When / Then
        assert_eq!(
            render(ScriptPosition::Superscript, "a <b> & c", &[]),
            "<sup>a &lt;b&gt; &amp; c</sup>"
        );
    }
}
