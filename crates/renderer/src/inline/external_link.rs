//! The link a cross-reference resolved through another site's inventory
//! renders as — one shape for every role, so an intersphinx link reads the
//! same whichever role found it.

use std::fmt::Write as _;

use crate::resolution::ExternalHit;

/// Writes `inner_html` (already escaped) as a link to `hit`, carrying
/// Sphinx's `reference external` classes and its `(in Project vX)` tooltip,
/// so a stylesheet written for Sphinx's intersphinx links applies unchanged.
pub(super) fn write_external_link(
    html: &mut String,
    hit: &ExternalHit<'_>,
    doc_path: &str,
    inner_html: &str,
) {
    let href = html_escape::encode_double_quoted_attribute(&hit.href(doc_path)).into_owned();
    let tooltip = html_escape::encode_double_quoted_attribute(&hit.tooltip()).into_owned();
    let _ = write!(
        html,
        "<a class=\"reference external\" href=\"{href}\" title=\"{tooltip}\">{inner_html}</a>"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolution::resolve_external;

    #[test]
    fn test_write_external_link_carries_href_classes_and_tooltip() {
        // Given
        let index = crate::test_support::index_linking_into_python();
        let hit = resolve_external(
            &index.external_inventories,
            &["py:class".to_string()],
            "dict",
            &rusty_sphinx_ast::InventorySelector::Any,
        )
        .unwrap();
        let mut html = String::new();

        // When
        write_external_link(&mut html, &hit, "index.rst", "<code>dict</code>");

        // Then
        assert_eq!(
            html,
            "<a class=\"reference external\" \
             href=\"https://docs.python.org/3/library/stdtypes.html#dict\" \
             title=\"(in Python v3.12)\"><code>dict</code></a>"
        );
    }
}
