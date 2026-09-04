//! The project's reading order: a depth-first walk of the toctree graph.
//!
//! This is what prev/next links are derived from, so it must match the order a
//! reader would take clicking "next" from the root. Sphinx computes it the
//! same way, in `TocTree._traverse_toctree`.
//!
//! Three rules are worth stating because each is a decision:
//!
//! - **First visit wins.** A document referenced from two places appears once,
//!   at its first position, so "next" never revisits a page.
//! - **Hidden toctrees count.** `:hidden:` suppresses *display*, not
//!   structure; the documents it lists are still part of the reading order.
//! - **Unreachable documents are absent.** A document no toctree names gets no
//!   neighbours at all, rather than being appended somewhere arbitrary.

use std::collections::BTreeSet;

use rusty_sphinx_index::ProjectIndex;

use rusty_sphinx_toctree::{TocTarget, expand_toctree};

/// Every document in reading order, starting from `index.root_documents`.
#[must_use]
pub(super) fn collect_page_order(index: &ProjectIndex, universe: &BTreeSet<String>) -> Vec<String> {
    let mut order = Vec::new();
    let mut seen = BTreeSet::new();

    for root in &index.root_documents {
        visit(root, index, universe, &mut order, &mut seen);
    }
    order
}

/// Appends `docname` and everything its toctrees reach, depth-first.
fn visit(
    docname: &str,
    index: &ProjectIndex,
    universe: &BTreeSet<String>,
    order: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
) {
    if !seen.insert(docname.to_string()) {
        return;
    }
    order.push(docname.to_string());

    let Some(toctrees) = index.toctrees.get(docname) else {
        return;
    };
    for placed in toctrees {
        let (targets, _) = expand_toctree(&placed.toctree, docname, universe);
        for target in targets {
            // Sections are not pages, and an external link is not part of this
            // project's reading order.
            if let TocTarget::Document { docname: child, .. } = target {
                visit(&child, index, universe, order, seen);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{TocEntry, Toctree, ToctreeFlag, ToctreeOptions};
    use rusty_sphinx_index::DocumentToctree;

    fn toctree(docnames: &[&str]) -> Toctree {
        Toctree {
            entries: docnames
                .iter()
                .map(|docname| TocEntry::Document {
                    title: None,
                    docname: (*docname).to_string(),
                    span: None,
                })
                .collect(),
            options: ToctreeOptions::default(),
        }
    }

    fn placed(toctree: Toctree) -> DocumentToctree {
        DocumentToctree {
            toctree,
            section: None,
        }
    }

    /// An index whose roots are `roots` and whose graph is `edges`.
    fn index_of(roots: &[&str], edges: &[(&str, Vec<DocumentToctree>)]) -> ProjectIndex {
        let mut index = ProjectIndex {
            root_documents: roots.iter().map(|r| format!("{r}.rst")).collect(),
            ..ProjectIndex::default()
        };
        for (owner, toctrees) in edges {
            index
                .toctrees
                .insert(format!("{owner}.rst"), toctrees.clone());
        }
        index
    }

    fn universe(docnames: &[&str]) -> BTreeSet<String> {
        docnames.iter().map(|d| format!("{d}.rst")).collect()
    }

    #[test]
    fn test_collect_page_order_walks_depth_first() {
        // Given — index → [a, b]; a → [a1].
        let index = index_of(
            &["index"],
            &[
                ("index", vec![placed(toctree(&["a", "b"]))]),
                ("a", vec![placed(toctree(&["a1"]))]),
            ],
        );
        let universe = universe(&["index", "a", "b", "a1"]);

        // When
        let order = collect_page_order(&index, &universe);

        // Then — a's child comes before b, as a reader clicking "next" would.
        assert_eq!(order, vec!["index.rst", "a.rst", "a1.rst", "b.rst"]);
    }

    #[test]
    fn test_collect_page_order_visits_a_shared_document_once() {
        // Given — both a and b reference shared.
        let index = index_of(
            &["index"],
            &[
                ("index", vec![placed(toctree(&["a", "b"]))]),
                ("a", vec![placed(toctree(&["shared"]))]),
                ("b", vec![placed(toctree(&["shared"]))]),
            ],
        );
        let universe = universe(&["index", "a", "b", "shared"]);

        // When
        let order = collect_page_order(&index, &universe);

        // Then — at its first position, so "next" never revisits it.
        assert_eq!(order, vec!["index.rst", "a.rst", "shared.rst", "b.rst"]);
    }

    #[test]
    fn test_collect_page_order_includes_documents_from_a_hidden_toctree() {
        // Given — `:hidden:` suppresses display, not structure.
        let hidden = {
            let mut toctree = toctree(&["a"]);
            toctree.options.set(ToctreeFlag::Hidden);
            toctree
        };
        let index = index_of(&["index"], &[("index", vec![placed(hidden)])]);
        let universe = universe(&["index", "a"]);

        // When
        let order = collect_page_order(&index, &universe);

        // Then
        assert_eq!(order, vec!["index.rst", "a.rst"]);
    }

    #[test]
    fn test_collect_page_order_omits_an_unreachable_document() {
        // Given — `orphan` exists but nothing references it.
        let index = index_of(&["index"], &[("index", vec![placed(toctree(&["a"]))])]);
        let universe = universe(&["index", "a", "orphan"]);

        // When
        let order = collect_page_order(&index, &universe);

        // Then — it gets no neighbours rather than an arbitrary position.
        assert_eq!(order, vec!["index.rst", "a.rst"]);
    }

    #[test]
    fn test_collect_page_order_terminates_on_a_cycle() {
        // Given — a references back to index.
        let index = index_of(
            &["index"],
            &[
                ("index", vec![placed(toctree(&["a"]))]),
                ("a", vec![placed(toctree(&["index"]))]),
            ],
        );
        let universe = universe(&["index", "a"]);

        // When
        let order = collect_page_order(&index, &universe);

        // Then
        assert_eq!(order, vec!["index.rst", "a.rst"]);
    }

    #[test]
    fn test_collect_page_order_skips_an_external_entry() {
        // Given
        let mut toctree = toctree(&["a"]);
        toctree.entries.push(TocEntry::External {
            title: None,
            url: "https://example.org".to_string(),
            span: None,
        });
        let index = index_of(&["index"], &[("index", vec![placed(toctree)])]);
        let universe = universe(&["index", "a"]);

        // When
        let order = collect_page_order(&index, &universe);

        // Then
        assert_eq!(order, vec!["index.rst", "a.rst"]);
    }

    #[test]
    fn test_collect_page_order_covers_every_root() {
        // Given — two roots, which is what inference yields with no configured
        // root document.
        let index = index_of(&["one", "two"], &[]);
        let universe = universe(&["one", "two"]);

        // When
        let order = collect_page_order(&index, &universe);

        // Then
        assert_eq!(order, vec!["one.rst", "two.rst"]);
    }
}
