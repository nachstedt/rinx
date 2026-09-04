//! Walking a toctree's entries into the tree of pages and sections it shows.
//!
//! This is where `:maxdepth:`, `:titlesonly:` and `:includehidden:` actually
//! take effect, and where cycles are broken. It works from the *graph* stored
//! in the index — each document's toctrees plus each document's section
//! outline — rather than from a pre-flattened tree, which is what lets two
//! toctrees in one document, with different options, render differently.
//!
//! # Depth
//!
//! Documents and sections share one counter, which is Sphinx's rule and the
//! reason `:maxdepth: 2` over a set of chapter files shows each chapter's own
//! sections but not the contents of any toctree inside them. A toctree's own
//! entries are depth 1.

use std::collections::BTreeSet;

use rusty_sphinx_ast::{SectionId, Toctree, ToctreeFlag};
use rusty_sphinx_index::{OutlineSection, ProjectIndex};

use super::entry::ResolvedNavEntry;
use super::resolve::{document_href, document_title};
use crate::nav::secnumber::secnumber_for;

/// Everything the walk needs that does not change as it descends.
struct Walk<'a> {
    index: &'a ProjectIndex,
    /// The site-relative logical path of the page being rendered, which every
    /// href is computed relative to.
    from_doc: &'a str,
    /// The `.rst` path of the page being rendered, so the current entry can be
    /// marked.
    current_doc: &'a str,
    /// Every `.rst` path the index knows a document for.
    universe: BTreeSet<String>,
    /// `:maxdepth:` — `None` is unlimited.
    maxdepth: Option<usize>,
    /// `:titlesonly:` — drop section entries entirely.
    titles_only: bool,
    /// `:includehidden:` — descend into a referenced document's own `:hidden:`
    /// toctrees as well.
    include_hidden: bool,
}

impl Walk<'_> {
    /// Whether an entry at `depth` may still be emitted.
    const fn within_depth(&self, depth: usize) -> bool {
        match self.maxdepth {
            Some(maxdepth) => depth <= maxdepth,
            None => true,
        }
    }
}

/// Expands one toctree into the entries it displays.
///
/// `owner` is the `.rst` path of the document containing the directive.
#[must_use]
pub(crate) fn expand_toctree_entries(
    toctree: &Toctree,
    owner: &str,
    index: &ProjectIndex,
    from_doc: &str,
    current_doc: &str,
) -> Vec<ResolvedNavEntry> {
    let walk = Walk {
        index,
        from_doc,
        current_doc,
        universe: index.document_titles.keys().cloned().collect(),
        maxdepth: toctree.options.maxdepth.map(std::num::NonZeroUsize::get),
        titles_only: toctree.options.has(ToctreeFlag::TitlesOnly),
        include_hidden: toctree.options.has(ToctreeFlag::IncludeHidden),
    };

    // The ancestor chain, so a document cannot be nested inside itself. It is
    // an ancestor set rather than a global seen-set, so a document legitimately
    // referenced from two branches still appears in both.
    let mut ancestors = BTreeSet::new();
    ancestors.insert(owner.to_string());

    expand_one(&walk, toctree, owner, 1, &mut ancestors)
}

/// The entries of `toctree`, whose directive lives in `owner`, at `depth`.
fn expand_one(
    walk: &Walk<'_>,
    toctree: &Toctree,
    owner: &str,
    depth: usize,
    ancestors: &mut BTreeSet<String>,
) -> Vec<ResolvedNavEntry> {
    if !walk.within_depth(depth) {
        return Vec::new();
    }

    let (targets, _) = rusty_sphinx_toctree::expand_toctree(toctree, owner, &walk.universe);
    targets
        .into_iter()
        .map(|target| match target {
            rusty_sphinx_toctree::TocTarget::External { url, title, .. } => {
                ResolvedNavEntry::External {
                    title: title.unwrap_or_else(|| url.clone()),
                    href: minijinja::Value::from_safe_string(url),
                }
            }
            rusty_sphinx_toctree::TocTarget::Document {
                docname,
                title,
                span: _,
            } => document_entry(walk, &docname, title.as_deref(), depth, ancestors),
        })
        .collect()
}

/// One document entry, with whatever the walk is still allowed to nest inside
/// it.
fn document_entry(
    walk: &Walk<'_>,
    docname: &str,
    explicit_title: Option<&str>,
    depth: usize,
    ancestors: &mut BTreeSet<String>,
) -> ResolvedNavEntry {
    let title = document_title(docname, explicit_title, walk.index);
    let href = document_href(docname, None, walk.from_doc);
    let secnumber = secnumber_for(walk.index, docname, None);
    let is_current = docname == walk.current_doc;

    // A document already on the ancestor chain would nest inside itself, so it
    // is emitted as a leaf and the cycle stops there.
    let children = if ancestors.insert(docname.to_string()) {
        let nested = children_of(walk, docname, depth, ancestors);
        ancestors.remove(docname);
        nested
    } else {
        Vec::new()
    };

    let is_ancestor = children.iter().any(ResolvedNavEntry::contains_current);

    ResolvedNavEntry::Page {
        title,
        href,
        anchor: None,
        secnumber,
        is_current,
        is_ancestor,
        children,
    }
}

/// What nests under `docname`: its own sections, with the entries of its own
/// toctrees spliced in wherever those directives were written.
fn children_of(
    walk: &Walk<'_>,
    docname: &str,
    depth: usize,
    ancestors: &mut BTreeSet<String>,
) -> Vec<ResolvedNavEntry> {
    if !walk.within_depth(depth + 1) {
        return Vec::new();
    }

    let mut children = Vec::new();

    if !walk.titles_only
        && let Some(outline) = walk.index.document_outlines.get(docname)
    {
        for section in &outline.sections {
            children.push(section_entry(walk, docname, section, depth + 1, ancestors));
        }
    }

    // Toctrees written before the document's first section belong to the
    // document itself. Ones written inside a section were spliced in there by
    // `section_entry`, so they are not repeated here.
    children.extend(nested_toctree_entries(
        walk, docname, None, depth, ancestors,
    ));

    children
}

/// The entries of every toctree in `docname` that was written inside
/// `section` (or, for `None`, at the document level).
fn nested_toctree_entries(
    walk: &Walk<'_>,
    docname: &str,
    section: Option<&SectionId>,
    depth: usize,
    ancestors: &mut BTreeSet<String>,
) -> Vec<ResolvedNavEntry> {
    let Some(toctrees) = walk.index.toctrees.get(docname) else {
        return Vec::new();
    };

    toctrees
        .iter()
        .filter(|placed| placed.section.as_ref() == section)
        // A `:hidden:` toctree contributes structure but displays nothing, so
        // it is skipped unless the toctree being rendered asked for it.
        .filter(|placed| walk.include_hidden || !placed.toctree.options.has(ToctreeFlag::Hidden))
        .flat_map(|placed| expand_one(walk, &placed.toctree, docname, depth + 1, ancestors))
        .collect()
}

/// One section of `docname`, its subsections, and any toctree written inside
/// it.
fn section_entry(
    walk: &Walk<'_>,
    docname: &str,
    section: &OutlineSection,
    depth: usize,
    ancestors: &mut BTreeSet<String>,
) -> ResolvedNavEntry {
    let mut children = Vec::new();
    if walk.within_depth(depth + 1) {
        for child in &section.children {
            children.push(section_entry(walk, docname, child, depth + 1, ancestors));
        }
        children.extend(nested_toctree_entries(
            walk,
            docname,
            Some(&section.id),
            depth,
            ancestors,
        ));
    }

    let is_ancestor = children.iter().any(ResolvedNavEntry::contains_current);

    ResolvedNavEntry::Page {
        title: section.title.clone(),
        href: document_href(docname, Some(&section.id), walk.from_doc),
        anchor: Some(section.id.clone()),
        secnumber: secnumber_for(walk.index, docname, Some(&section.id)),
        // A section is never "the current page"; the document holding it is.
        is_current: false,
        is_ancestor,
        children,
    }
}

/// The sidebar's entries: every root document's toctrees, expanded.
///
/// The sidebar shows the same tree on every page — only `is_current` and
/// `is_ancestor` differ — so it always expands with `:includehidden:` in
/// effect. A `:hidden:` toctree exists precisely to contribute structure the
/// sidebar shows while the page body does not.
#[must_use]
pub(crate) fn sidebar_entries(
    index: &ProjectIndex,
    from_doc: &str,
    current_doc: &str,
) -> Vec<ResolvedNavEntry> {
    let mut entries = Vec::new();
    for root in &index.root_documents {
        let Some(toctrees) = index.toctrees.get(root) else {
            continue;
        };
        for placed in toctrees {
            // The sidebar honours each toctree's own `:maxdepth:` and
            // `:titlesonly:`, but never its `:hidden:` — see above.
            let mut toctree = placed.toctree.clone();
            toctree.options.set(ToctreeFlag::IncludeHidden);
            entries.extend(expand_toctree_entries(
                &toctree,
                root,
                index,
                from_doc,
                current_doc,
            ));
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{TocEntry, ToctreeOptions};
    use rusty_sphinx_index::{DocumentOutline, DocumentToctree, OutlineSection};

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

    fn section(title: &str, children: Vec<OutlineSection>) -> OutlineSection {
        OutlineSection {
            title: title.to_string(),
            id: SectionId::from_title(title),
            children,
        }
    }

    /// An index knowing `docs` (docname without extension → title).
    fn index_of(docs: &[(&str, &str)]) -> ProjectIndex {
        let mut index = ProjectIndex::default();
        for (docname, title) in docs {
            index
                .document_titles
                .insert(format!("{docname}.rst"), (*title).to_string());
        }
        index
    }

    /// A compact shape of the rendered tree: titles, children in parentheses.
    fn shape(entries: &[ResolvedNavEntry]) -> String {
        entries
            .iter()
            .map(|entry| {
                if entry.children().is_empty() {
                    entry.title().to_string()
                } else {
                    format!("{}({})", entry.title(), shape(entry.children()))
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    fn expand(toctree: &Toctree, index: &ProjectIndex) -> Vec<ResolvedNavEntry> {
        expand_toctree_entries(toctree, "index.rst", index, "index", "index.rst")
    }

    #[test]
    fn test_expand_lists_the_documents_the_toctree_names() {
        // Given
        let index = index_of(&[("intro", "Introduction"), ("guide", "Guide")]);

        // When
        let entries = expand(&toctree(&["intro", "guide"]), &index);

        // Then
        assert_eq!(shape(&entries), "Introduction,Guide");
    }

    #[test]
    fn test_expand_nests_a_referenced_documents_sections() {
        // Given — the default behaviour `:titlesonly:` switches off.
        let mut index = index_of(&[("guide", "Guide")]);
        index.document_outlines.insert(
            "guide.rst".to_string(),
            DocumentOutline {
                sections: vec![section("Install", vec![section("From Source", vec![])])],
            },
        );

        // When
        let entries = expand(&toctree(&["guide"]), &index);

        // Then
        assert_eq!(shape(&entries), "Guide(Install(From Source))");
    }

    #[test]
    fn test_titlesonly_drops_section_entries() {
        // Given
        let mut index = index_of(&[("guide", "Guide")]);
        index.document_outlines.insert(
            "guide.rst".to_string(),
            DocumentOutline {
                sections: vec![section("Install", vec![])],
            },
        );
        let mut toctree = toctree(&["guide"]);
        toctree.options.set(ToctreeFlag::TitlesOnly);

        // When
        let entries = expand(&toctree, &index);

        // Then
        assert_eq!(shape(&entries), "Guide");
    }

    #[test]
    fn test_maxdepth_counts_documents_and_sections_on_one_counter() {
        // Given — `:maxdepth: 2` shows the document (depth 1) and its
        // top-level sections (depth 2), but not their subsections.
        let mut index = index_of(&[("guide", "Guide")]);
        index.document_outlines.insert(
            "guide.rst".to_string(),
            DocumentOutline {
                sections: vec![section("Install", vec![section("From Source", vec![])])],
            },
        );
        let mut toctree = toctree(&["guide"]);
        toctree.options.maxdepth = std::num::NonZeroUsize::new(2);

        // When
        let entries = expand(&toctree, &index);

        // Then
        assert_eq!(shape(&entries), "Guide(Install)");
    }

    #[test]
    fn test_maxdepth_of_one_shows_documents_alone() {
        // Given
        let mut index = index_of(&[("guide", "Guide")]);
        index.document_outlines.insert(
            "guide.rst".to_string(),
            DocumentOutline {
                sections: vec![section("Install", vec![])],
            },
        );
        let mut toctree = toctree(&["guide"]);
        toctree.options.maxdepth = std::num::NonZeroUsize::new(1);

        // When
        let entries = expand(&toctree, &index);

        // Then
        assert_eq!(shape(&entries), "Guide");
    }

    #[test]
    fn test_two_toctrees_with_different_maxdepth_render_differently() {
        // Given — the headline regression: before this rewrite both toctrees
        // in one document rendered the same list, because the renderer looked
        // the *document* up in a flattened tree instead of using the
        // directive's own entries and options.
        let mut index = index_of(&[("guide", "Guide")]);
        index.document_outlines.insert(
            "guide.rst".to_string(),
            DocumentOutline {
                sections: vec![section("Install", vec![])],
            },
        );

        let shallow = {
            let mut toctree = toctree(&["guide"]);
            toctree.options.maxdepth = std::num::NonZeroUsize::new(1);
            toctree
        };
        let deep = toctree(&["guide"]);

        // When
        let shallow_entries = expand(&shallow, &index);
        let deep_entries = expand(&deep, &index);

        // Then
        assert_eq!(shape(&shallow_entries), "Guide");
        assert_eq!(shape(&deep_entries), "Guide(Install)");
        assert_ne!(shape(&shallow_entries), shape(&deep_entries));
    }

    #[test]
    fn test_two_toctrees_listing_different_documents_render_differently() {
        // Given
        let index = index_of(&[("a", "Alpha"), ("b", "Beta")]);

        // When
        let first = expand(&toctree(&["a"]), &index);
        let second = expand(&toctree(&["b"]), &index);

        // Then
        assert_eq!(shape(&first), "Alpha");
        assert_eq!(shape(&second), "Beta");
    }

    #[test]
    fn test_expand_descends_into_a_referenced_documents_own_toctree() {
        // Given
        let mut index = index_of(&[("guide", "Guide"), ("setup", "Setup")]);
        index.toctrees.insert(
            "guide.rst".to_string(),
            vec![DocumentToctree {
                toctree: toctree(&["setup"]),
                section: None,
            }],
        );

        // When
        let entries = expand(&toctree(&["guide"]), &index);

        // Then
        assert_eq!(shape(&entries), "Guide(Setup)");
    }

    #[test]
    fn test_expand_splices_a_nested_toctree_under_the_section_it_was_written_in() {
        // Given — Sphinx puts a toctree's entries where the directive sits.
        let mut index = index_of(&[("guide", "Guide"), ("setup", "Setup")]);
        index.document_outlines.insert(
            "guide.rst".to_string(),
            DocumentOutline {
                sections: vec![section("Advanced", vec![])],
            },
        );
        index.toctrees.insert(
            "guide.rst".to_string(),
            vec![DocumentToctree {
                toctree: toctree(&["setup"]),
                section: Some(SectionId::from_title("Advanced")),
            }],
        );

        // When
        let entries = expand(&toctree(&["guide"]), &index);

        // Then — Setup is under Advanced, not beside it.
        assert_eq!(shape(&entries), "Guide(Advanced(Setup))");
    }

    #[test]
    fn test_expand_skips_a_hidden_nested_toctree_by_default() {
        // Given
        let mut index = index_of(&[("guide", "Guide"), ("setup", "Setup")]);
        let hidden = {
            let mut toctree = toctree(&["setup"]);
            toctree.options.set(ToctreeFlag::Hidden);
            toctree
        };
        index.toctrees.insert(
            "guide.rst".to_string(),
            vec![DocumentToctree {
                toctree: hidden,
                section: None,
            }],
        );

        // When
        let entries = expand(&toctree(&["guide"]), &index);

        // Then
        assert_eq!(shape(&entries), "Guide");
    }

    #[test]
    fn test_includehidden_descends_into_a_hidden_nested_toctree() {
        // Given
        let mut index = index_of(&[("guide", "Guide"), ("setup", "Setup")]);
        let hidden = {
            let mut toctree = toctree(&["setup"]);
            toctree.options.set(ToctreeFlag::Hidden);
            toctree
        };
        index.toctrees.insert(
            "guide.rst".to_string(),
            vec![DocumentToctree {
                toctree: hidden,
                section: None,
            }],
        );
        let mut outer = toctree(&["guide"]);
        outer.options.set(ToctreeFlag::IncludeHidden);

        // When
        let entries = expand(&outer, &index);

        // Then
        assert_eq!(shape(&entries), "Guide(Setup)");
    }

    #[test]
    fn test_expand_breaks_a_cycle() {
        // Given — guide references back to index.
        let mut index = index_of(&[("index", "Home"), ("guide", "Guide")]);
        index.toctrees.insert(
            "guide.rst".to_string(),
            vec![DocumentToctree {
                toctree: toctree(&["index"]),
                section: None,
            }],
        );

        // When
        let entries = expand(&toctree(&["guide"]), &index);

        // Then — Home appears once as a leaf, and the walk terminates.
        assert_eq!(shape(&entries), "Guide(Home)");
    }

    #[test]
    fn test_expand_allows_one_document_in_two_branches() {
        // Given — shared is referenced from both left and right, which is not
        // a cycle.
        let mut index = index_of(&[("left", "Left"), ("right", "Right"), ("shared", "Shared")]);
        for owner in ["left", "right"] {
            index.toctrees.insert(
                format!("{owner}.rst"),
                vec![DocumentToctree {
                    toctree: toctree(&["shared"]),
                    section: None,
                }],
            );
        }

        // When
        let entries = expand(&toctree(&["left", "right"]), &index);

        // Then
        assert_eq!(shape(&entries), "Left(Shared),Right(Shared)");
    }

    #[test]
    fn test_expand_renders_an_external_entry_without_children() {
        // Given
        let index = ProjectIndex::default();
        let toctree = Toctree {
            entries: vec![TocEntry::External {
                title: Some("Upstream".to_string()),
                url: "https://example.org".to_string(),
                span: None,
            }],
            options: ToctreeOptions::default(),
        };

        // When
        let entries = expand(&toctree, &index);

        // Then
        assert_eq!(
            entries,
            vec![ResolvedNavEntry::External {
                title: "Upstream".to_string(),
                href: minijinja::Value::from_safe_string("https://example.org".to_string()),
            }]
        );
    }

    #[test]
    fn test_expand_labels_an_untitled_external_entry_with_its_url() {
        // Given
        let index = ProjectIndex::default();
        let toctree = Toctree {
            entries: vec![TocEntry::External {
                title: None,
                url: "https://example.org".to_string(),
                span: None,
            }],
            options: ToctreeOptions::default(),
        };

        // When
        let entries = expand(&toctree, &index);

        // Then
        assert_eq!(entries[0].title(), "https://example.org");
    }

    #[test]
    fn test_expand_marks_the_current_page() {
        // Given
        let index = index_of(&[("intro", "Introduction"), ("guide", "Guide")]);

        // When — rendering from `guide`.
        let entries = expand_toctree_entries(
            &toctree(&["intro", "guide"]),
            "index.rst",
            &index,
            "guide",
            "guide.rst",
        );

        // Then
        assert!(!entries[0].contains_current());
        assert!(entries[1].contains_current());
    }

    #[test]
    fn test_expand_marks_an_ancestor_of_the_current_page() {
        // Given — the current page is nested under `guide`.
        let mut index = index_of(&[("guide", "Guide"), ("setup", "Setup")]);
        index.toctrees.insert(
            "guide.rst".to_string(),
            vec![DocumentToctree {
                toctree: toctree(&["setup"]),
                section: None,
            }],
        );

        // When
        let entries = expand_toctree_entries(
            &toctree(&["guide"]),
            "index.rst",
            &index,
            "setup",
            "setup.rst",
        );

        // Then
        let ResolvedNavEntry::Page { is_ancestor, .. } = &entries[0] else {
            panic!("expected a page entry");
        };
        assert!(is_ancestor);
    }

    #[test]
    fn test_expand_resolves_self_to_the_owning_document() {
        // Given
        let index = index_of(&[("index", "Home")]);
        let toctree = Toctree {
            entries: vec![TocEntry::SelfRef {
                title: None,
                span: None,
            }],
            options: ToctreeOptions::default(),
        };

        // When
        let entries = expand(&toctree, &index);

        // Then — and no infinite recursion, since the owner starts as an
        // ancestor.
        assert_eq!(shape(&entries), "Home");
    }
}
