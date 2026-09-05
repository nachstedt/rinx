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
//!
//! # `.. sectnum::` / `.. section-numbering::`
//!
//! Plain docutils' own numbering directive feeds the same [`DocumentNumbers`]
//! this module builds for `:numbered:` toctrees, so the two must agree on
//! precedence for a document that both could cover:
//!
//! - A document already numbered by an ancestor `:numbered:` toctree ignores
//!   its own `.. sectnum::` entirely — the same rule as a nested `:numbered:`
//!   toctree above, just extended to this second source of numbers.
//! - Otherwise `.. sectnum::` numbers that one document's own top-level
//!   outline sections (and, recursively, their children) independently of
//!   every other document — plain docutils has no toctree, so `:start:`,
//!   `:depth:`, `:prefix:` and `:suffix:` never cross a document boundary.
//! - Unlike a `:numbered:` toctree, `.. sectnum::` never gives the document
//!   itself a number: plain docutils has no notion of "this whole document is
//!   2", only of numbered sections within it, so a sectnum-numbered
//!   document's title heading stays unnumbered.

use std::collections::{BTreeMap, BTreeSet};

use rusty_sphinx_ast::{NumberedDepth, SectnumOptions};
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

    // A document's own `.. sectnum::` numbers it independently, unless an
    // ancestor `:numbered:` toctree already claimed it above.
    for (docname, options) in &index.sectnum {
        if numbered.contains(docname) {
            continue;
        }
        number_sectnum_document(docname, options, index, &mut numbers);
    }

    numbers
}

/// Numbers `docname`'s own top-level outline sections (and their children)
/// per its `.. sectnum::` options, independently of every other document.
///
/// Reuses [`number_section`] verbatim for the recursion — `:start:` is simply
/// the counter [`number_section`] is first called with, and `:depth:` maps
/// onto the same [`NumberedDepth`] `:numbered:` already uses to stop appending
/// past a limit.
fn number_sectnum_document(
    docname: &str,
    options: &SectnumOptions,
    index: &ProjectIndex,
    numbers: &mut BTreeMap<String, DocumentNumbers>,
) {
    numbers
        .entry(docname.to_string())
        .or_default()
        .set_format(options.prefix.clone(), options.suffix.clone());

    let Some(outline) = index.document_outlines.get(docname) else {
        return;
    };
    let depth = options
        .depth
        .map_or(NumberedDepth::Unlimited, NumberedDepth::Levels);
    let start = options.start.map_or(1, |value| value.get() as usize);

    for (offset, section) in outline.sections.iter().enumerate() {
        number_section(docname, section, &[], start + offset, depth, numbers);
    }
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

        fn with_sectnum(mut self, docname: &str, options: SectnumOptions) -> Self {
            self.index.sectnum.insert(format!("{docname}.rst"), options);
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

    #[test]
    fn test_sectnum_numbers_a_standalone_documents_sections() {
        // Given — no toctree at all, just a document with `.. sectnum::`.
        let fixture = Fixture::new(&["a"], &["a"])
            .with_sections(
                "a",
                vec![section("Install", vec![]), section("Use", vec![])],
            )
            .with_sectnum("a", SectnumOptions::default());

        // When
        let numbers = fixture.numbers();

        // Then — sections are numbered, but the document itself is not: plain
        // docutils has no notion of a document's own number.
        assert_eq!(numbers["a.rst"].document(), None);
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("Install")),
            Some([1].as_slice())
        );
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("Use")),
            Some([2].as_slice())
        );
    }

    #[test]
    fn test_sectnum_numbers_subsections_recursively() {
        // Given
        let fixture = Fixture::new(&["a"], &["a"])
            .with_sections(
                "a",
                vec![section("Install", vec![section("From Source", vec![])])],
            )
            .with_sectnum("a", SectnumOptions::default());

        // When
        let numbers = fixture.numbers();

        // Then
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("From Source")),
            Some([1, 1].as_slice())
        );
    }

    #[test]
    fn test_sectnum_start_offsets_the_first_top_level_number() {
        // Given — `:start: 5`.
        let options = SectnumOptions {
            start: std::num::NonZeroU32::new(5),
            ..SectnumOptions::default()
        };
        let fixture = Fixture::new(&["a"], &["a"])
            .with_sections(
                "a",
                vec![section("Install", vec![]), section("Use", vec![])],
            )
            .with_sectnum("a", options);

        // When
        let numbers = fixture.numbers();

        // Then
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("Install")),
            Some([5].as_slice())
        );
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("Use")),
            Some([6].as_slice())
        );
    }

    #[test]
    fn test_sectnum_depth_stops_appending_past_its_limit() {
        // Given — `:depth: 1`.
        let options = SectnumOptions {
            depth: NonZeroUsize::new(1),
            ..SectnumOptions::default()
        };
        let fixture = Fixture::new(&["a"], &["a"])
            .with_sections(
                "a",
                vec![section("Install", vec![section("From Source", vec![])])],
            )
            .with_sectnum("a", options);

        // When
        let numbers = fixture.numbers();

        // Then
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("Install")),
            Some([1].as_slice())
        );
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("From Source")),
            None
        );
    }

    #[test]
    fn test_sectnum_zero_depth_is_unlimited() {
        // Given — `:depth: 0` mirrors `.. contents::`'s own convention.
        let options = SectnumOptions {
            depth: None,
            ..SectnumOptions::default()
        };
        let fixture = Fixture::new(&["a"], &["a"])
            .with_sections(
                "a",
                vec![section("Install", vec![section("From Source", vec![])])],
            )
            .with_sectnum("a", options);

        // When
        let numbers = fixture.numbers();

        // Then
        assert_eq!(
            numbers["a.rst"].section(&SectionId::from_title("From Source")),
            Some([1, 1].as_slice())
        );
    }

    #[test]
    fn test_sectnum_records_prefix_and_suffix() {
        // Given
        let options = SectnumOptions {
            prefix: "Appendix ".to_string(),
            suffix: ".".to_string(),
            ..SectnumOptions::default()
        };
        let fixture = Fixture::new(&["a"], &["a"]).with_sectnum("a", options);

        // When
        let numbers = fixture.numbers();

        // Then
        assert_eq!(numbers["a.rst"].prefix(), "Appendix ");
        assert_eq!(numbers["a.rst"].suffix(), ".");
    }

    #[test]
    fn test_an_ancestor_numbered_toctree_wins_over_a_documents_own_sectnum() {
        // Given — `a` is reached by a `:numbered:` toctree *and* writes its
        // own `.. sectnum::`.
        let fixture = Fixture::new(&["index"], &["index", "a"])
            .with_toctree("index", toctree(&["a"], Some(NumberedDepth::Unlimited)))
            .with_sections("a", vec![section("Install", vec![])])
            .with_sectnum(
                "a",
                SectnumOptions {
                    prefix: "Ignored ".to_string(),
                    ..SectnumOptions::default()
                },
            );

        // When
        let numbers = fixture.numbers();

        // Then — the toctree's numbering wins: `a` gets a document number
        // (which plain `.. sectnum::` alone never assigns) and no prefix.
        assert_eq!(numbers["a.rst"].document(), Some([1].as_slice()));
        assert_eq!(numbers["a.rst"].prefix(), "");
    }
}
