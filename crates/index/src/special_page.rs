use rinx_ast::TargetName;

use crate::{DomainIndex, ProjectIndex};

/// A page the build writes that no document is — `genindex.html`, or a domain
/// index such as `py-modindex.html` — as the label a `:ref:` or `:any:` names
/// it by.
///
/// Sphinx predefines these labels (`StandardDomain.initial_data['labels']`,
/// plus one per domain index from `Domain.setup`), so a document can write
/// `` :ref:`genindex` `` without defining anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecialPage {
    /// Where the page is written, relative to the site root.
    pub path: &'static str,
    /// What a bare `:ref:` to it shows, as Sphinx spells it.
    pub title: &'static str,
}

/// The general index, which every site writes.
const GENINDEX: SpecialPage = SpecialPage {
    path: "genindex.html",
    title: "Index",
};

impl ProjectIndex {
    /// The special page the label `name` names, if this site writes it.
    ///
    /// `genindex` always exists. `modindex` — Sphinx's generic spelling,
    /// titled "Module Index" — and `py-modindex` exist only for a site whose
    /// `domain_indices` enables the Python Module Index: Sphinx predefines
    /// them unconditionally and links a page it may never write, where this
    /// build reports the link broken instead. A label a document defines is
    /// looked up before this, so it wins.
    #[must_use]
    pub fn special_page(&self, name: &TargetName) -> Option<SpecialPage> {
        let modindex = DomainIndex::PyModindex;
        match name.as_str() {
            "genindex" => Some(GENINDEX),
            "modindex" if self.domain_indices.contains(&modindex) => Some(SpecialPage {
                path: modindex.page_path(),
                title: "Module Index",
            }),
            "py-modindex" if self.domain_indices.contains(&modindex) => Some(SpecialPage {
                path: modindex.page_path(),
                title: modindex.title(),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_with_modindex() -> ProjectIndex {
        ProjectIndex {
            domain_indices: [DomainIndex::PyModindex].into(),
            ..ProjectIndex::default()
        }
    }

    #[test]
    fn test_special_page_always_knows_the_general_index() {
        // Given
        let index = ProjectIndex::default();

        // When / Then
        assert_eq!(
            index.special_page(&TargetName::new("genindex")),
            Some(SpecialPage {
                path: "genindex.html",
                title: "Index",
            })
        );
    }

    #[test]
    fn test_special_page_knows_both_module_index_labels_when_enabled() {
        // Given
        let index = index_with_modindex();

        // When / Then — the titles Sphinx 9.1 gives them
        assert_eq!(
            index.special_page(&TargetName::new("modindex")),
            Some(SpecialPage {
                path: "py-modindex.html",
                title: "Module Index",
            })
        );
        assert_eq!(
            index.special_page(&TargetName::new("py-modindex")),
            Some(SpecialPage {
                path: "py-modindex.html",
                title: "Python Module Index",
            })
        );
    }

    #[test]
    fn test_special_page_knows_no_module_index_the_site_does_not_write() {
        // Given
        let index = ProjectIndex::default();

        // When / Then
        assert_eq!(index.special_page(&TargetName::new("modindex")), None);
        assert_eq!(index.special_page(&TargetName::new("py-modindex")), None);
    }

    #[test]
    fn test_special_page_knows_nothing_else() {
        // Given — this build writes no search page
        let index = index_with_modindex();

        // When / Then
        assert_eq!(index.special_page(&TargetName::new("search")), None);
        assert_eq!(index.special_page(&TargetName::new("install")), None);
    }
}
