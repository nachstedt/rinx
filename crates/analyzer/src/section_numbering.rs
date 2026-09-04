//! Assigning `:numbered:` section numbers across the whole project.
//!
//! Unlike [`super::equation_numbering`], which restarts in every document and
//! so can run inside `analyze()`, section numbers depend on the entire toctree
//! graph *and* on every document's outline. They can only be computed once
//! both exist, which is why this is a separate phase of
//! [`super::build_project_index`] rather than part of a document's own
//! analysis.
//!
//! # The rules, each of which is Sphinx's
//!
//! - Numbering **starts** at a toctree carrying `:numbered:` and propagates
//!   into everything reachable below it, across document boundaries.
//! - Documents and sections share one counter, exactly as `:maxdepth:` counts
//!   them, so a numbered chapter's first section is `1.1`.
//! - `:numbered: N` stops *appending* past `N` components. A deeper section is
//!   left unnumbered rather than given a truncated number, which would make
//!   two different sections share one number.
//! - A nested `:numbered:` inside an already-numbered subtree is ignored: the
//!   outer numbering already covers it, and restarting would produce two
//!   sections numbered `1`.

use std::collections::{BTreeMap, BTreeSet};

use rusty_sphinx_ast::NumberedDepth;
use rusty_sphinx_index::{DocumentNumbers, OutlineSection, ProjectIndex};

use rusty_sphinx_toctree::{TocTarget, expand_toctree};

/// The section numbers for every document the project's `:numbered:` toctrees
/// reach.
#[must_use]
pub(super) fn assign_section_numbers(
    index: &ProjectIndex,
    universe: &BTreeSet<String>,
) -> BTreeMap<String, DocumentNumbers> {
    let mut numbers: BTreeMap<String, DocumentNumbers> = BTreeMap::new();
    let mut numbered = BTreeSet::new();

    // Walk from the roots so numbering starts wherever the first `:numbered:`
    // toctree is, at whatever depth that turns out to be.
    let mut seeking = BTreeSet::new();
    for root in &index.root_documents {
        seek_numbered_toctrees(
            root,
            index,
            universe,
            &mut numbers,
            &mut numbered,
            &mut seeking,
        );
    }
    numbers
}

/// Descends through `docname` looking for a toctree that starts numbering.
///
/// `numbered` is the set of documents already covered by some numbering, which
/// is what makes a nested `:numbered:` inside a numbered subtree a no-op.
/// `seeking` is every document this search has already descended through, so a
/// cycle of unnumbered toctrees terminates instead of recursing forever.
fn seek_numbered_toctrees(
    docname: &str,
    index: &ProjectIndex,
    universe: &BTreeSet<String>,
    numbers: &mut BTreeMap<String, DocumentNumbers>,
    numbered: &mut BTreeSet<String>,
    seeking: &mut BTreeSet<String>,
) {
    if !seeking.insert(docname.to_string()) {
        return;
    }
    let Some(toctrees) = index.toctrees.get(docname) else {
        return;
    };

    for placed in toctrees {
        let (targets, _) = expand_toctree(&placed.toctree, docname, universe);
        match placed.toctree.options.numbered {
            Some(depth) => {
                let mut counter = 0;
                for target in &targets {
                    let TocTarget::Document { docname: child, .. } = target else {
                        continue;
                    };
                    if numbered.contains(child) {
                        continue;
                    }
                    counter += 1;
                    number_document(child, &[counter], depth, index, universe, numbers, numbered);
                }
            }
            None => {
                for target in &targets {
                    if let TocTarget::Document { docname: child, .. } = target
                        && !numbered.contains(child)
                    {
                        seek_numbered_toctrees(child, index, universe, numbers, numbered, seeking);
                    }
                }
            }
        }
    }
}

/// Numbers `docname` itself as `prefix`, then everything below it.
fn number_document(
    docname: &str,
    prefix: &[usize],
    depth: NumberedDepth,
    index: &ProjectIndex,
    universe: &BTreeSet<String>,
    numbers: &mut BTreeMap<String, DocumentNumbers>,
    numbered: &mut BTreeSet<String>,
) {
    if !numbered.insert(docname.to_string()) {
        return;
    }
    numbers
        .entry(docname.to_string())
        .or_default()
        .set_document(prefix.to_vec());

    // A document's own sections and its nested toctrees continue the same
    // numbering one level deeper, sharing a counter the way `:maxdepth:`
    // counts them together.
    let mut counter = 0;

    if let Some(outline) = index.document_outlines.get(docname) {
        for section in &outline.sections {
            counter += 1;
            number_section(docname, section, prefix, counter, depth, numbers);
        }
    }

    let Some(toctrees) = index.toctrees.get(docname) else {
        return;
    };
    for placed in toctrees {
        let (targets, _) = expand_toctree(&placed.toctree, docname, universe);
        for target in &targets {
            let TocTarget::Document { docname: child, .. } = target else {
                continue;
            };
            if numbered.contains(child) {
                continue;
            }
            counter += 1;
            let mut child_prefix = prefix.to_vec();
            child_prefix.push(counter);
            number_document(
                child,
                &child_prefix,
                depth,
                index,
                universe,
                numbers,
                numbered,
            );
        }
    }
}

/// Numbers one section and its subsections.
fn number_section(
    docname: &str,
    section: &OutlineSection,
    prefix: &[usize],
    counter: usize,
    depth: NumberedDepth,
    numbers: &mut BTreeMap<String, DocumentNumbers>,
) {
    let mut number = prefix.to_vec();
    number.push(counter);

    // Past the configured depth a section is left unnumbered rather than given
    // a number that would duplicate its parent's.
    if !depth.covers(number.len()) {
        return;
    }

    numbers
        .entry(docname.to_string())
        .or_default()
        .set_section(&section.id, number.clone());

    for (child_index, child) in section.children.iter().enumerate() {
        number_section(docname, child, &number, child_index + 1, depth, numbers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{SectionId, TocEntry, Toctree, ToctreeOptions};
    use rusty_sphinx_index::{DocumentOutline, DocumentToctree};
    use std::num::NonZeroUsize;

    fn toctree(docnames: &[&str], numbered: Option<NumberedDepth>) -> Toctree {
        let options = ToctreeOptions {
            numbered,
            ..ToctreeOptions::default()
        };
        Toctree {
            entries: docnames
                .iter()
                .map(|docname| TocEntry::Document {
                    title: None,
                    docname: (*docname).to_string(),
                    span: None,
                })
                .collect(),
            options,
        }
    }

    fn placed(toctree: Toctree) -> DocumentToctree {
        DocumentToctree {
            toctree,
            section: None,
        }
    }

    fn section(title: &str, children: Vec<OutlineSection>) -> OutlineSection {
        OutlineSection {
            title: title.to_string(),
            id: SectionId::from_title(title),
            children,
        }
    }

    struct Fixture {
        index: ProjectIndex,
        universe: BTreeSet<String>,
    }

    impl Fixture {
        fn new(roots: &[&str], docs: &[&str]) -> Self {
            let index = ProjectIndex {
                root_documents: roots.iter().map(|r| format!("{r}.rst")).collect(),
                ..ProjectIndex::default()
            };
            Self {
                index,
                universe: docs.iter().map(|d| format!("{d}.rst")).collect(),
            }
        }

        fn with_toctree(mut self, owner: &str, toctree: Toctree) -> Self {
            self.index
                .toctrees
                .entry(format!("{owner}.rst"))
                .or_default()
                .push(placed(toctree));
            self
        }

        fn with_sections(mut self, docname: &str, sections: Vec<OutlineSection>) -> Self {
            self.index
                .document_outlines
                .insert(format!("{docname}.rst"), DocumentOutline { sections });
            self
        }

        fn numbers(&self) -> BTreeMap<String, DocumentNumbers> {
            assign_section_numbers(&self.index, &self.universe)
        }
    }

    #[test]
    fn test_numbered_toctree_numbers_its_entries_in_order() {
        // Given
        let fixture = Fixture::new(&["index"], &["index", "a", "b"]).with_toctree(
            "index",
            toctree(&["a", "b"], Some(NumberedDepth::Unlimited)),
        );

        // When
        let numbers = fixture.numbers();

        // Then
        assert_eq!(numbers["a.rst"].document(), Some([1].as_slice()));
        assert_eq!(numbers["b.rst"].document(), Some([2].as_slice()));
    }

    #[test]
    fn test_an_unnumbered_toctree_numbers_nothing() {
        // Given
        let fixture =
            Fixture::new(&["index"], &["index", "a"]).with_toctree("index", toctree(&["a"], None));

        // When
        let numbers = fixture.numbers();

        // Then
        assert!(numbers.is_empty());
    }

    #[test]
    fn test_numbering_continues_into_a_documents_sections() {
        // Given — a numbered chapter with two sections.
        let fixture = Fixture::new(&["index"], &["index", "a"])
            .with_toctree("index", toctree(&["a"], Some(NumberedDepth::Unlimited)))
            .with_sections(
                "a",
                vec![section("Install", vec![]), section("Use", vec![])],
            );

        // When
        let numbers = fixture.numbers();

        // Then — documents and sections share the counter.
        assert_eq!(numbers["a.rst"].document(), Some([1].as_slice()));
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("Install")),
            Some([1, 1].as_slice())
        );
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("Use")),
            Some([1, 2].as_slice())
        );
    }

    #[test]
    fn test_numbering_continues_into_subsections() {
        // Given
        let fixture = Fixture::new(&["index"], &["index", "a"])
            .with_toctree("index", toctree(&["a"], Some(NumberedDepth::Unlimited)))
            .with_sections(
                "a",
                vec![section("Install", vec![section("From Source", vec![])])],
            );

        // When
        let numbers = fixture.numbers();

        // Then
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("From Source")),
            Some([1, 1, 1].as_slice())
        );
    }

    #[test]
    fn test_numbering_crosses_document_boundaries() {
        // Given — index → a → a1, all numbered from one toctree.
        let fixture = Fixture::new(&["index"], &["index", "a", "a1"])
            .with_toctree("index", toctree(&["a"], Some(NumberedDepth::Unlimited)))
            .with_toctree("a", toctree(&["a1"], None));

        // When
        let numbers = fixture.numbers();

        // Then
        assert_eq!(numbers["a.rst"].document(), Some([1].as_slice()));
        assert_eq!(numbers["a1.rst"].document(), Some([1, 1].as_slice()));
    }

    #[test]
    fn test_numbered_depth_stops_appending_past_its_limit() {
        // Given — `:numbered: 2`.
        let depth = NumberedDepth::Levels(NonZeroUsize::new(2).unwrap());
        let fixture = Fixture::new(&["index"], &["index", "a"])
            .with_toctree("index", toctree(&["a"], Some(depth)))
            .with_sections(
                "a",
                vec![section("Install", vec![section("From Source", vec![])])],
            );

        // When
        let numbers = fixture.numbers();

        // Then — the third level is left unnumbered, not truncated to `1.1`.
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("Install")),
            Some([1, 1].as_slice())
        );
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("From Source")),
            None
        );
    }

    #[test]
    fn test_a_nested_numbered_toctree_is_ignored_inside_a_numbered_subtree() {
        // Given — a's own toctree also says `:numbered:`.
        let fixture = Fixture::new(&["index"], &["index", "a", "a1"])
            .with_toctree("index", toctree(&["a"], Some(NumberedDepth::Unlimited)))
            .with_toctree("a", toctree(&["a1"], Some(NumberedDepth::Unlimited)));

        // When
        let numbers = fixture.numbers();

        // Then — a1 continues the outer numbering rather than restarting at 1.
        assert_eq!(numbers["a1.rst"].document(), Some([1, 1].as_slice()));
    }

    #[test]
    fn test_numbering_can_start_below_an_unnumbered_toctree() {
        // Given — the root's toctree is unnumbered; a's is numbered.
        let fixture = Fixture::new(&["index"], &["index", "a", "a1"])
            .with_toctree("index", toctree(&["a"], None))
            .with_toctree("a", toctree(&["a1"], Some(NumberedDepth::Unlimited)));

        // When
        let numbers = fixture.numbers();

        // Then — a is unnumbered, its child starts at 1.
        assert!(!numbers.contains_key("a.rst"));
        assert_eq!(numbers["a1.rst"].document(), Some([1].as_slice()));
    }

    #[test]
    fn test_numbering_terminates_on_a_cycle() {
        // Given — a references back to index.
        let fixture = Fixture::new(&["index"], &["index", "a"])
            .with_toctree("index", toctree(&["a"], Some(NumberedDepth::Unlimited)))
            .with_toctree("a", toctree(&["index"], None));

        // When
        let numbers = fixture.numbers();

        // Then
        assert_eq!(numbers["a.rst"].document(), Some([1].as_slice()));
    }
}
