//! Registry role rendering — `:pep:`, `:rfc:`, `:cve:` and `:cwe:`: the
//! anchor the role's general-index entry links to, then a link to the
//! document's page below its registry's address.
//!
//! Nothing here consults the project index: the link leaves the site by
//! construction, so it cannot be broken while rendering.

use std::fmt::Write as _;

use rinx_ast::{Registry, RegistryTarget};

use crate::config::{PepBaseUrl, RfcBaseUrl};

/// Sphinx's fixed address of the CVE record lookup, below which a `:cve:`
/// target's `CVE-…` page sits.
const CVE_RECORD_URL: &str = "https://www.cve.org/CVERecord?id=";

/// Sphinx's fixed address of the CWE definitions, below which a `:cwe:`
/// target's `….html` page sits.
const CWE_DEFINITIONS_URL: &str = "https://cwe.mitre.org/data/definitions/";

/// A registry role as the author wrote it, with the anchor the parser gave
/// it.
#[derive(Debug, Clone, Copy)]
pub(super) struct RegistryRef<'a> {
    /// The explicit title of the `Title <8>` form, if one was written.
    pub title: Option<&'a str>,
    pub target: &'a RegistryTarget,
    /// The `id` the general index links back to.
    pub index_id: &'a str,
}

impl<'a> RegistryRef<'a> {
    /// Gathers what a registry node carries for rendering, the title first as
    /// it is written first in the `Title <8>` form.
    pub(super) const fn new(
        title: Option<&'a str>,
        target: &'a RegistryTarget,
        index_id: &'a str,
    ) -> Self {
        Self {
            title,
            target,
            index_id,
        }
    }
}

/// The address `registry`'s pages are appended to: the site's configured
/// PEP or RFC index, or the address Sphinx hard-codes for CVE and CWE.
pub(super) fn registry_base_url<'a>(
    registry: Registry,
    pep_base_url: &'a PepBaseUrl,
    rfc_base_url: &'a RfcBaseUrl,
) -> &'a str {
    match registry {
        Registry::Pep => pep_base_url.as_str(),
        Registry::Rfc => rfc_base_url.as_str(),
        Registry::Cve => CVE_RECORD_URL,
        Registry::Cwe => CWE_DEFINITIONS_URL,
    }
}

/// Renders a registry role as Sphinx 9.1's HTML builder does: an empty
/// `<span class="target">` carrying the index anchor, then an
/// `<a class="pep reference external">` — the class named for the registry —
/// around a `<strong>` showing the explicit title, else the target's
/// [`RegistryTarget::display_text`].
pub(super) fn render_inline_registry_reference(
    html: &mut String,
    reference: RegistryRef<'_>,
    base_url: &str,
) {
    let RegistryRef {
        title,
        target,
        index_id,
    } = reference;
    let href = format!("{base_url}{}", target.page_path());
    let text = title.map_or_else(|| target.display_text(), str::to_string);
    let _ = write!(
        html,
        "<span class=\"target\" id=\"{}\"></span>\
         <a class=\"{} reference external\" href=\"{}\"><strong>{}</strong></a>",
        html_escape::encode_double_quoted_attribute(index_id),
        target.registry().role_name(),
        html_escape::encode_double_quoted_attribute(&href),
        html_escape::encode_text(&text),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(registry: Registry, title: Option<&str>, target: &str) -> String {
        render_below(
            registry,
            title,
            target,
            &PepBaseUrl::default(),
            &RfcBaseUrl::default(),
        )
    }

    fn render_below(
        registry: Registry,
        title: Option<&str>,
        target: &str,
        pep: &PepBaseUrl,
        rfc: &RfcBaseUrl,
    ) -> String {
        let target = RegistryTarget::parse(registry, target).unwrap();
        let mut html = String::new();
        render_inline_registry_reference(
            &mut html,
            RegistryRef::new(title, &target, "index-0"),
            registry_base_url(registry, pep, rfc),
        );
        html
    }

    #[test]
    fn test_renders_sphinx_markup_for_a_bare_pep_number() {
        // Given / When
        let html = render(Registry::Pep, None, "8");

        // Then
        assert_eq!(
            html,
            "<span class=\"target\" id=\"index-0\"></span>\
             <a class=\"pep reference external\" href=\"https://peps.python.org/pep-0008/\">\
             <strong>PEP 8</strong></a>"
        );
    }

    #[test]
    fn test_renders_sphinx_markup_for_an_rfc_section() {
        // Given / When
        let html = render(Registry::Rfc, None, "2324#section-2.3.2");

        // Then
        assert_eq!(
            html,
            "<span class=\"target\" id=\"index-0\"></span>\
             <a class=\"rfc reference external\" \
             href=\"https://datatracker.ietf.org/doc/html/rfc2324.html#section-2.3.2\">\
             <strong>RFC 2324 Section 2.3.2</strong></a>"
        );
    }

    #[test]
    fn test_renders_sphinx_markup_for_a_cve() {
        // Given / When
        let html = render(Registry::Cve, None, "2024-3094");

        // Then
        assert_eq!(
            html,
            "<span class=\"target\" id=\"index-0\"></span>\
             <a class=\"cve reference external\" \
             href=\"https://www.cve.org/CVERecord?id=CVE-2024-3094\">\
             <strong>CVE 2024-3094</strong></a>"
        );
    }

    #[test]
    fn test_renders_sphinx_markup_for_a_cwe() {
        // Given / When
        let html = render(Registry::Cwe, None, "0787");

        // Then — the text as written, the page unpadded
        assert_eq!(
            html,
            "<span class=\"target\" id=\"index-0\"></span>\
             <a class=\"cwe reference external\" \
             href=\"https://cwe.mitre.org/data/definitions/787.html\">\
             <strong>CWE 0787</strong></a>"
        );
    }

    #[test]
    fn test_links_a_fragment_and_shows_it_in_the_text() {
        // Given / When
        let html = render(Registry::Pep, None, "8#naming");

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
        let html = render(Registry::Cwe, Some("Style <guide> & more"), "8");

        // Then
        assert!(
            html.contains("<strong>Style &lt;guide&gt; &amp; more</strong>"),
            "{html}"
        );
    }

    #[test]
    fn test_links_below_the_configured_base_urls() {
        // Given
        let pep = PepBaseUrl::parse("/mirror/peps/").unwrap();
        let rfc = RfcBaseUrl::parse("https://www.rfc-editor.org/rfc/").unwrap();

        // When
        let pep_html = render_below(Registry::Pep, None, "20", &pep, &rfc);
        let rfc_html = render_below(Registry::Rfc, None, "20", &pep, &rfc);

        // Then
        assert!(
            pep_html.contains("href=\"/mirror/peps/pep-0020/\""),
            "{pep_html}"
        );
        assert!(
            rfc_html.contains("href=\"https://www.rfc-editor.org/rfc/rfc20.html\""),
            "{rfc_html}"
        );
    }

    #[test]
    fn test_escapes_a_fragment_in_the_href() {
        // Given / When
        let html = render(Registry::Pep, None, "8#a\"b");

        // Then
        assert!(html.contains("pep-0008/#a&quot;b\""), "{html}");
    }

    #[test]
    fn test_registry_base_url_uses_sphinx_fixed_addresses_for_cve_and_cwe() {
        // Given
        let pep = PepBaseUrl::default();
        let rfc = RfcBaseUrl::default();

        // When / Then
        assert_eq!(
            registry_base_url(Registry::Cve, &pep, &rfc),
            "https://www.cve.org/CVERecord?id="
        );
        assert_eq!(
            registry_base_url(Registry::Cwe, &pep, &rfc),
            "https://cwe.mitre.org/data/definitions/"
        );
        assert_eq!(
            registry_base_url(Registry::Pep, &pep, &rfc),
            "https://peps.python.org/"
        );
        assert_eq!(
            registry_base_url(Registry::Rfc, &pep, &rfc),
            "https://datatracker.ietf.org/doc/html/"
        );
    }
}
