//! `:pep:` rendering: the anchor the role's general-index entry links to,
//! then a link to the PEP's page below the site's PEP index.
//!
//! Nothing here consults the project index: the link leaves the site by
//! construction, so it cannot be broken while rendering.

use std::fmt::Write as _;

use rinx_ast::PepTarget;

use crate::config::PepBaseUrl;

/// A `:pep:` as the author wrote it, with the anchor the parser gave it.
#[derive(Debug, Clone, Copy)]
pub(super) struct PepRef<'a> {
    /// The explicit title of the `Title <8>` form, if one was written.
    pub title: Option<&'a str>,
    pub target: &'a PepTarget,
    /// The `id` the general index links back to.
    pub index_id: &'a str,
}

/// Renders a `:pep:` as Sphinx 9.1's HTML builder does: an empty
/// `<span class="target">` carrying the index anchor, then an
/// `<a class="pep reference external">` around a `<strong>` showing the
/// explicit title, else `PEP ` and the target as written.
pub(super) fn render_inline_pep_reference(
    html: &mut String,
    reference: PepRef<'_>,
    base_url: &PepBaseUrl,
) {
    let PepRef {
        title,
        target,
        index_id,
    } = reference;
    let href = format!("{}{}", base_url.as_str(), target.page_path());
    let text = title.map_or_else(|| format!("PEP {}", target.as_written()), str::to_string);
    let _ = write!(
        html,
        "<span class=\"target\" id=\"{}\"></span>\
         <a class=\"pep reference external\" href=\"{}\"><strong>{}</strong></a>",
        html_escape::encode_double_quoted_attribute(index_id),
        html_escape::encode_double_quoted_attribute(&href),
        html_escape::encode_text(&text),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(title: Option<&str>, target: &str, base_url: &PepBaseUrl) -> String {
        let target = PepTarget::parse(target).unwrap();
        let mut html = String::new();
        render_inline_pep_reference(
            &mut html,
            PepRef {
                title,
                target: &target,
                index_id: "index-0",
            },
            base_url,
        );
        html
    }

    #[test]
    fn test_renders_sphinx_markup_for_a_bare_number() {
        // Given / When
        let html = render(None, "8", &PepBaseUrl::default());

        // Then
        assert_eq!(
            html,
            "<span class=\"target\" id=\"index-0\"></span>\
             <a class=\"pep reference external\" href=\"https://peps.python.org/pep-0008/\">\
             <strong>PEP 8</strong></a>"
        );
    }

    #[test]
    fn test_links_a_fragment_and_shows_it_in_the_text() {
        // Given / When
        let html = render(None, "8#naming", &PepBaseUrl::default());

        // Then — Sphinx keeps the fragment in the text
        assert!(
            html.contains("href=\"https://peps.python.org/pep-0008/#naming\""),
            "{html}"
        );
        assert!(html.contains("<strong>PEP 8#naming</strong>"), "{html}");
    }

    #[test]
    fn test_shows_an_explicit_title_without_the_prefix() {
        // Given / When
        let html = render(Some("Style <guide> & more"), "8", &PepBaseUrl::default());

        // Then
        assert!(
            html.contains("<strong>Style &lt;guide&gt; &amp; more</strong>"),
            "{html}"
        );
    }

    #[test]
    fn test_links_below_the_configured_base_url() {
        // Given
        let base_url = PepBaseUrl::parse("/mirror/peps/").unwrap();

        // When
        let html = render(None, "20", &base_url);

        // Then
        assert!(html.contains("href=\"/mirror/peps/pep-0020/\""), "{html}");
    }

    #[test]
    fn test_escapes_a_fragment_in_the_href() {
        // Given / When
        let html = render(None, "8#a\"b", &PepBaseUrl::default());

        // Then
        assert!(html.contains("pep-0008/#a&quot;b\""), "{html}");
    }
}
