//! Assigning `numfig` numbers to every captioned figure, table and code block
//! in the project.
//!
//! A port of Sphinx's `assign_figure_numbers`, and a project-wide phase for
//! the reason [`super::section_numbering`] is one: a figure's number depends
//! on every document the toctree walk reads before it, and on the section
//! numbers, which must already exist. Each rule below is Sphinx's.
//!
//! - The walk starts at the root documents and descends through each toctree
//!   **where it is written**: a child document's figures are numbered between
//!   the parent's figures before and after the toctree. A document is walked
//!   once, the first time it is reached; one no toctree reaches gets no
//!   numbers at all.
//! - Every kind has its own counter, keyed by the section number the element
//!   sits under, cut to `numfig_secnum_depth` components. With no numbered
//!   sections that key is empty for every element, so the count runs on across
//!   documents; with `:numbered:` it restarts in each chapter (`1.1`, `2.1`).
//! - The section number an element sits under is its innermost enclosing
//!   section's — or, for one too deep to be numbered, the document's own
//!   ([`rinx_index::DocumentNumbers::number_at`]). A document with no
//!   number at all inherits the number of the toctree position it was reached
//!   from.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rinx_ast::{EnumerableKind, SectionId};
use rinx_index::{DocumentNumbers, ElementNumbers, NumberingStep, ProjectIndex};
use rinx_toctree::{TocTarget, expand_toctree};

/// The numbers of every element the toctree walk from `index`'s roots reaches.
///
/// Reads `root_documents`, `toctrees`, `section_numbers` and
/// `numbering_steps`, so it runs after section numbering. `universe` is the
/// set of documents a toctree entry may expand to.
#[must_use]
pub fn assign_element_numbers(
    index: &ProjectIndex,
    universe: &BTreeSet<String>,
    secnum_depth: usize,
) -> BTreeMap<String, ElementNumbers> {
    let mut walk = Walk {
        index,
        universe,
        secnum_depth,
        visited: BTreeSet::new(),
        counters: HashMap::new(),
        numbers: BTreeMap::new(),
    };
    for root in &index.root_documents {
        walk.document(root, &[]);
    }
    walk.numbers
}

/// The state of one project-wide numbering walk.
struct Walk<'a> {
    index: &'a ProjectIndex,
    universe: &'a BTreeSet<String>,
    secnum_depth: usize,
    /// Every document already walked — Sphinx's `assigned`.
    visited: BTreeSet<String>,
    /// The last number handed out per kind and per section-number prefix.
    counters: HashMap<(EnumerableKind, Vec<usize>), usize>,
    numbers: BTreeMap<String, ElementNumbers>,
}

impl Walk<'_> {
    /// Numbers `doc_path`'s elements and, at each of its toctrees, the
    /// documents that toctree lists. `inherited` is the section number of the
    /// toctree position the walk reached it from.
    fn document(&mut self, doc_path: &str, inherited: &[usize]) {
        if !self.visited.insert(doc_path.to_string()) {
            return;
        }
        let Some(steps) = self.index.numbering_steps.get(doc_path) else {
            return;
        };
        let doc_numbers = self.index.section_numbers.get(doc_path);
        let mut ordinal = 0;
        for step in steps {
            match step {
                NumberingStep::Element { kind, sections } => {
                    let under = section_number_under(inherited, sections, doc_numbers);
                    let number = self.next_number(*kind, under);
                    self.numbers
                        .entry(doc_path.to_string())
                        .or_default()
                        .set(ordinal, number);
                    ordinal += 1;
                }
                NumberingStep::Toctree { position, sections } => {
                    let under = section_number_under(inherited, sections, doc_numbers).to_vec();
                    for child in self.toctree_documents(doc_path, *position) {
                        self.document(&child, &under);
                    }
                }
            }
        }
    }

    /// The documents `doc_path`'s toctree at `position` lists, in order.
    fn toctree_documents(&self, doc_path: &str, position: usize) -> Vec<String> {
        let Some(toctree) = self
            .index
            .toctrees
            .get(doc_path)
            .and_then(|toctrees| toctrees.get(position))
        else {
            return Vec::new();
        };
        let (targets, _) = expand_toctree(&toctree.toctree, doc_path, self.universe);
        targets
            .into_iter()
            .filter_map(|target| match target {
                TocTarget::Document { docname, .. } => Some(docname),
                TocTarget::External { .. } => None,
            })
            .collect()
    }

    /// The next number of `kind` under the section number `under`.
    fn next_number(&mut self, kind: EnumerableKind, under: &[usize]) -> Vec<usize> {
        let prefix = under[..under.len().min(self.secnum_depth)].to_vec();
        let counter = self.counters.entry((kind, prefix.clone())).or_insert(0);
        *counter += 1;
        let mut number = prefix;
        number.push(*counter);
        number
    }
}

/// The section number an element or toctree inside `sections` sits under:
/// the innermost section that has one, else what the walk `inherited`.
fn section_number_under<'a>(
    inherited: &'a [usize],
    sections: &[SectionId],
    doc_numbers: Option<&'a DocumentNumbers>,
) -> &'a [usize] {
    let Some(doc_numbers) = doc_numbers else {
        return inherited;
    };
    sections
        .iter()
        .rev()
        .find_map(|id| doc_numbers.number_at(Some(id)))
        .filter(|number| !number.is_empty())
        .unwrap_or(inherited)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{TocEntry, Toctree, ToctreeOptions};
    use rinx_index::DocumentToctree;

    fn element(sections: &[&str]) -> NumberingStep {
        NumberingStep::Element {
            kind: EnumerableKind::Figure,
            sections: sections
                .iter()
                .map(|title| SectionId::from_title(title))
                .collect(),
        }
    }

    fn toctree_step(position: usize) -> NumberingStep {
        NumberingStep::Toctree {
            position,
            sections: Vec::new(),
        }
    }

    fn toctree_of(entries: &[&str]) -> DocumentToctree {
        DocumentToctree {
            toctree: Toctree {
                entries: entries
                    .iter()
                    .map(|docname| TocEntry::Document {
                        title: None,
                        docname: (*docname).to_string(),
                        span: None,
                    })
                    .collect(),
                options: ToctreeOptions::default(),
            },
            section: None,
        }
    }

    /// A root holding a figure, a toctree over `one` and `two`, and another
    /// figure; each child holds one figure.
    fn project() -> ProjectIndex {
        let mut index = ProjectIndex {
            root_documents: vec!["index.rst".to_string()],
            ..ProjectIndex::default()
        };
        index.numbering_steps.insert(
            "index.rst".to_string(),
            vec![element(&[]), toctree_step(0), element(&[])],
        );
        index
            .toctrees
            .insert("index.rst".to_string(), vec![toctree_of(&["one", "two"])]);
        for doc in ["one.rst", "two.rst"] {
            index
                .numbering_steps
                .insert(doc.to_string(), vec![element(&[])]);
        }
        index
    }

    fn universe() -> BTreeSet<String> {
        ["index.rst", "one.rst", "two.rst"]
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn number_of(
        numbers: &BTreeMap<String, ElementNumbers>,
        doc: &str,
        ordinal: usize,
    ) -> Vec<usize> {
        numbers[doc].get(ordinal).expect("numbered").to_vec()
    }

    #[test]
    fn test_unnumbered_sections_count_on_across_documents_in_toctree_order() {
        // Given / When
        let numbers = assign_element_numbers(&project(), &universe(), 1);

        // Then — the children are counted where the toctree stands
        assert_eq!(number_of(&numbers, "index.rst", 0), vec![1]);
        assert_eq!(number_of(&numbers, "one.rst", 0), vec![2]);
        assert_eq!(number_of(&numbers, "two.rst", 0), vec![3]);
        assert_eq!(number_of(&numbers, "index.rst", 1), vec![4]);
    }

    #[test]
    fn test_numbered_chapters_restart_the_count_under_their_number() {
        // Given `:numbered:` chapters one and two, each figure under its
        // document's title
        let mut index = project();
        for (doc, title, chapter) in [("one.rst", "One", 1), ("two.rst", "Two", 2)] {
            index
                .numbering_steps
                .insert(doc.to_string(), vec![element(&[title])]);
            let mut numbers = DocumentNumbers::default();
            numbers.set_document(vec![chapter]);
            index.section_numbers.insert(doc.to_string(), numbers);
        }

        // When
        let numbers = assign_element_numbers(&index, &universe(), 1);

        // Then — the root has no number, so its own figures stay 1 and 2
        assert_eq!(number_of(&numbers, "index.rst", 0), vec![1]);
        assert_eq!(number_of(&numbers, "one.rst", 0), vec![1, 1]);
        assert_eq!(number_of(&numbers, "two.rst", 0), vec![2, 1]);
        assert_eq!(number_of(&numbers, "index.rst", 1), vec![2]);
    }

    #[test]
    fn test_content_outside_every_section_inherits_the_toctree_position() {
        // Given chapter one has a number but its figure has no heading above
        let mut index = project();
        let mut numbers = DocumentNumbers::default();
        numbers.set_document(vec![1]);
        index.section_numbers.insert("one.rst".to_string(), numbers);

        // When
        let numbers = assign_element_numbers(&index, &universe(), 1);

        // Then — as in Sphinx, only a section takes a number from the document
        assert_eq!(number_of(&numbers, "one.rst", 0), vec![2]);
    }

    #[test]
    fn test_secnum_depth_cuts_the_section_number_prefix() {
        // Given a figure in section 1.2.3 of chapter one
        let mut index = project();
        index.numbering_steps.insert(
            "one.rst".to_string(),
            vec![element(&["One", "Deep", "Deeper"])],
        );
        let mut numbers = DocumentNumbers::default();
        numbers.set_document(vec![1]);
        numbers.set_section(&SectionId::from_title("Deep"), vec![1, 2]);
        numbers.set_section(&SectionId::from_title("Deeper"), vec![1, 2, 3]);
        index.section_numbers.insert("one.rst".to_string(), numbers);

        // When
        let depth_0 = assign_element_numbers(&index, &universe(), 0);
        let depth_2 = assign_element_numbers(&index, &universe(), 2);
        let depth_9 = assign_element_numbers(&index, &universe(), 9);

        // Then
        assert_eq!(number_of(&depth_0, "one.rst", 0), vec![2]);
        assert_eq!(number_of(&depth_2, "one.rst", 0), vec![1, 2, 1]);
        assert_eq!(number_of(&depth_9, "one.rst", 0), vec![1, 2, 3, 1]);
    }

    #[test]
    fn test_a_document_no_toctree_reaches_is_not_numbered() {
        // Given an orphan with a figure
        let mut index = project();
        index
            .numbering_steps
            .insert("orphan.rst".to_string(), vec![element(&[])]);

        // When
        let numbers = assign_element_numbers(&index, &universe(), 1);

        // Then
        assert!(!numbers.contains_key("orphan.rst"));
    }

    #[test]
    fn test_a_document_listed_twice_is_numbered_once() {
        // Given `one` listed by both the root and itself (a cycle)
        let mut index = project();
        index
            .numbering_steps
            .insert("one.rst".to_string(), vec![element(&[]), toctree_step(0)]);
        index
            .toctrees
            .insert("one.rst".to_string(), vec![toctree_of(&["index", "one"])]);

        // When
        let numbers = assign_element_numbers(&index, &universe(), 1);

        // Then — walking terminates and nothing is renumbered
        assert_eq!(number_of(&numbers, "one.rst", 0), vec![2]);
        assert_eq!(number_of(&numbers, "index.rst", 1), vec![4]);
    }

    #[test]
    fn test_kinds_count_separately() {
        // Given a figure, a table and a figure
        let mut index = ProjectIndex {
            root_documents: vec!["index.rst".to_string()],
            ..ProjectIndex::default()
        };
        index.numbering_steps.insert(
            "index.rst".to_string(),
            vec![
                element(&[]),
                NumberingStep::Element {
                    kind: EnumerableKind::Table,
                    sections: Vec::new(),
                },
                element(&[]),
            ],
        );

        // When
        let numbers = assign_element_numbers(&index, &universe(), 1);

        // Then
        assert_eq!(number_of(&numbers, "index.rst", 1), vec![1]);
        assert_eq!(number_of(&numbers, "index.rst", 2), vec![2]);
    }

    #[test]
    fn test_section_number_under_prefers_the_innermost_numbered_section() {
        // Given
        let mut numbers = DocumentNumbers::default();
        numbers.set_section(&SectionId::from_title("Outer"), vec![3]);
        let sections = [
            SectionId::from_title("Outer"),
            SectionId::from_title("Inner"),
        ];

        // When
        let under = section_number_under(&[7], &sections, Some(&numbers));
        let unnumbered = section_number_under(&[7], &sections, None);

        // Then — `Inner` has no number and there is no document number
        assert_eq!(under, &[3]);
        assert_eq!(unnumbered, &[7]);
    }
}
