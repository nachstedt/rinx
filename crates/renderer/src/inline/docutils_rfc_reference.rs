//! docutils' `:rfc-reference:` rendering: a plain link to the RFC's page
//! below the site's RFC index, with none of `:rfc:`'s anchor or emphasis.
//!
//! Nothing here consults the project index, for the reason
//! [`super::registry_reference`] gives.

use std::fmt::Write as _;

use rinx_ast::DocutilsRfcNumber;

use crate::config::RfcBaseUrl;

/// Renders an `:rfc-reference:` as docutils' `rfc_reference_role` does under
/// Sphinx 9.1's HTML builder: an `<a class="reference external">` showing
/// `RFC ` and the number as `int()` reads it, linking the page `rfc%d.html`
/// and any section below `base_url`.
pub(super) fn render_inline_docutils_rfc_reference(
    html: &mut String,
    number: &DocutilsRfcNumber,
    base_url: &RfcBaseUrl,
) {
    let href = format!("{}{}", base_url.as_str(), number.page_path());
    let _ = write!(
        html,
        "<a class=\"reference external\" href=\"{}\">{}</a>",
        html_escape::encode_double_quoted_attribute(&href),
        html_escape::encode_text(&number.display_text()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(number: &str, base_url: &RfcBaseUrl) -> String {
        let number = DocutilsRfcNumber::parse(number).unwrap();
        let mut html = String::new();
        render_inline_docutils_rfc_reference(&mut html, &number, base_url);
        html
    }

    #[test]
    fn test_renders_docutils_markup() {
        // Given / When
        let html = render("02822#section-3", &RfcBaseUrl::default());

        // Then — the number normalized, the section only in the href
        assert_eq!(
            html,
            "<a class=\"reference external\" \
             href=\"https://datatracker.ietf.org/doc/html/rfc2822.html#section-3\">RFC 2822</a>"
        );
    }

    #[test]
    fn test_links_below_the_configured_base_url() {
        // Given
        let base_url = RfcBaseUrl::parse("/rfcs/").unwrap();

        // When
        let html = render("1", &base_url);

        // Then
        assert!(html.contains("href=\"/rfcs/rfc1.html\""), "{html}");
    }

    #[test]
    fn test_escapes_a_section_in_the_href() {
        // Given / When
        let html = render("1#a\"b", &RfcBaseUrl::default());

        // Then
        assert!(html.contains("rfc1.html#a&quot;b\""), "{html}");
    }
}
