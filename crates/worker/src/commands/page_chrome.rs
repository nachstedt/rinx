//! What every page-drawing subcommand (`render`, `preview`, `genindex`,
//! `modindex`) needs beyond the page's own content: the template, and which
//! domain index pages the site writes, so every sidebar links them. The
//! `--domain-index` flag they are read from is parsed by
//! [`read_domain_indices`](super::cli_args::read_domain_indices).

use rinx_renderer::DomainIndex;
use std::collections::BTreeSet;

/// The site-wide inputs a page's chrome is drawn from, bundled so a
/// `process_*` function takes them as one value — they are read together, and
/// only by `render_page`.
pub(super) struct PageChrome<'a> {
    /// The `MiniJinja` page template.
    pub template: &'a str,
    /// The domain index pages the site writes (`rinx_site`'s
    /// `domain_indices`).
    pub domain_indices: &'a BTreeSet<DomainIndex>,
}

impl PageChrome<'_> {
    /// Whether the site writes `py-modindex.html`, so a page may link it.
    pub(super) fn has_modindex(&self) -> bool {
        self.domain_indices.contains(&DomainIndex::PyModindex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_modindex_follows_the_enabled_indices() {
        // Given
        let enabled = BTreeSet::from([DomainIndex::PyModindex]);
        let none = BTreeSet::new();

        // When / Then
        assert!(
            PageChrome {
                template: "",
                domain_indices: &enabled
            }
            .has_modindex()
        );
        assert!(
            !PageChrome {
                template: "",
                domain_indices: &none
            }
            .has_modindex()
        );
    }
}
