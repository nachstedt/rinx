//! Reading a document's or section's `:numbered:` number out of the index.
//!
//! Both places that show a number go through here: the navigation entries and
//! the headings on the target page itself. They must agree — a toctree saying
//! "2.1. Install" that links to a heading rendered as "3.4. Install" would be
//! worse than no numbering at all — so neither computes anything, they only
//! look it up.

use rusty_sphinx_ast::SectionId;
use rusty_sphinx_index::ProjectIndex;

/// The section number for `docname`, or for one of its sections when `anchor`
/// is given. `None` when nothing numbers it.
#[must_use]
pub(crate) fn secnumber_for(
    index: &ProjectIndex,
    docname: &str,
    anchor: Option<&SectionId>,
) -> Option<Vec<usize>> {
    let numbers = index.section_numbers.get(docname)?;
    let found = match anchor {
        Some(anchor) => numbers.section(anchor)?,
        None => numbers.document()?,
    };
    Some(found.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_index::DocumentNumbers;

    fn index_with_numbers() -> ProjectIndex {
        let mut numbers = DocumentNumbers::default();
        numbers.set_document(vec![2]);
        numbers.set_section(&SectionId::from_title("Install"), vec![2, 1]);

        let mut index = ProjectIndex::default();
        index
            .section_numbers
            .insert("guide.rst".to_string(), numbers);
        index
    }

    #[test]
    fn test_secnumber_for_reads_a_documents_own_number() {
        // Given
        let index = index_with_numbers();

        // When / Then
        assert_eq!(secnumber_for(&index, "guide.rst", None), Some(vec![2]));
    }

    #[test]
    fn test_secnumber_for_reads_a_sections_number() {
        // Given
        let index = index_with_numbers();

        // When / Then
        assert_eq!(
            secnumber_for(&index, "guide.rst", Some(&SectionId::from_title("Install"))),
            Some(vec![2, 1])
        );
    }

    #[test]
    fn test_secnumber_for_is_none_for_an_unnumbered_document() {
        // Given
        let index = index_with_numbers();

        // When / Then
        assert_eq!(secnumber_for(&index, "other.rst", None), None);
    }

    #[test]
    fn test_secnumber_for_is_none_for_an_unnumbered_section() {
        // Given
        let index = index_with_numbers();

        // When / Then
        assert_eq!(
            secnumber_for(
                &index,
                "guide.rst",
                Some(&SectionId::from_title("Unnumbered"))
            ),
            None
        );
    }
}
