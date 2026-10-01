//! docutils' `:pep-reference:` rendering: a plain link to the PEP's page
//! below the site's PEP index, with none of `:pep:`'s anchor or emphasis.
//!
//! Nothing here consults the project index, for the reason
//! [`super::pep_reference`] gives.

use std::fmt::Write as _;

use rinx_ast::DocutilsPepNumber;

use crate::config::PepBaseUrl;

/// Renders a `:pep-reference:` as docutils' `pep_reference_role` does under
/// Sphinx 9.1's HTML builder: an `<a class="reference external">` showing
/// `PEP ` and the number as written, linking the page `pep-%04d` — no
/// trailing slash, unlike `:pep:` — below `base_url`.
pub(super) fn render_inline_docutils_pep_reference(
    html: &mut String,
    number: &DocutilsPepNumber,
    base_url: &PepBaseUrl,
) {
    let href = format!("{}{}", base_url.as_str(), number.page_path());
    let _ = write!(
        html,
        "<a class=\"reference external\" href=\"{}\">PEP {}</a>",
        html_escape::encode_double_quoted_attribute(&href),
        html_escape::encode_text(number.as_written()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(number: &str, base_url: &PepBaseUrl) -> String {
        let number = DocutilsPepNumber::parse(number).unwrap();
        let mut html = String::new();
        render_inline_docutils_pep_reference(&mut html, &number, base_url);
        html
    }

    #[test]
    fn test_renders_docutils_markup() {
        // Given / When
        let html = render("8", &PepBaseUrl::default());

        // Then
        assert_eq!(
            html,
            "<a class=\"reference external\" href=\"https://peps.python.org/pep-0008\">PEP 8</a>"
        );
    }

    #[test]
    fn test_shows_the_number_as_written() {
        // Given / When
        let html = render("0020", &PepBaseUrl::default());

        // Then
        assert!(html.contains("pep-0020\">PEP 0020</a>"), "{html}");
    }

    #[test]
    fn test_links_below_the_configured_base_url() {
        // Given
        let base_url = PepBaseUrl::parse("/mirror/peps/").unwrap();

        // When
        let html = render("484", &base_url);

        // Then
        assert!(html.contains("href=\"/mirror/peps/pep-0484\""), "{html}");
    }
}
