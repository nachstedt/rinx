//! Where an `.. include::` stood, and which file it brought in.

use std::collections::BTreeSet;

use crate::span::{FileId, Span};

/// One `.. include::` whose file was parsed into the document.
///
/// A diagnostic found inside a fragment names the fragment, so on its own it
/// cannot say which `.. include::` brought it in — a fragment included in two
/// places carries one [`FileId`]. The sites are what an editor reads to point
/// the includer's author at the line that pulled a problem in.
///
/// Recorded by file rather than by the diagnostics found while the fragment was
/// parsed, because several diagnostics in a fragment are found only after the
/// whole document is parsed — an undefined substitution among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncludeSite {
    /// The directive's first line, in the file it is written in — the
    /// document, or another fragment for a nested include.
    pub directive: Option<Span>,
    /// The file the directive brought in.
    pub file: FileId,
}

impl IncludeSite {
    /// The files whose diagnostics this site brought in: its own and,
    /// transitively, every file included from it, according to `sites`.
    ///
    /// Terminates on a cycle of sites, which the parser refuses to produce
    /// but a mangled table could still describe.
    #[must_use]
    pub fn reached_files(&self, sites: &[IncludeSite]) -> BTreeSet<FileId> {
        let mut reached = BTreeSet::new();
        let mut pending = vec![self.file];
        while let Some(file) = pending.pop() {
            if reached.insert(file) {
                pending.extend(
                    sites
                        .iter()
                        .filter(|site| site.directive.and_then(|span| span.file) == Some(file))
                        .map(|site| site.file),
                );
            }
        }
        reached
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Position;

    /// A site on `line` of `within` (`None` for the document) including `file`.
    fn site(within: Option<u32>, line: u32, file: u32) -> IncludeSite {
        IncludeSite {
            directive: Some(
                Span::new(Position::new(line, 1), Position::new(line, 2))
                    .with_file(within.map(FileId::new)),
            ),
            file: FileId::new(file),
        }
    }

    #[test]
    fn test_reached_files_of_a_leaf_site_is_its_own_file() {
        // Given
        let sites = [site(None, 1, 0), site(None, 2, 1)];

        // When
        let reached = sites[0].reached_files(&sites);

        // Then
        assert_eq!(reached, BTreeSet::from([FileId::new(0)]));
    }

    #[test]
    fn test_reached_files_follows_nested_includes() {
        // Given the document includes 0, which includes 1, which includes 2
        let sites = [site(None, 1, 0), site(Some(0), 3, 1), site(Some(1), 5, 2)];

        // When
        let reached = sites[0].reached_files(&sites);

        // Then
        assert_eq!(
            reached,
            BTreeSet::from([FileId::new(0), FileId::new(1), FileId::new(2)])
        );
    }

    #[test]
    fn test_reached_files_terminates_on_a_cycle() {
        // Given a table describing 0 and 1 including each other
        let sites = [site(None, 1, 0), site(Some(0), 1, 1), site(Some(1), 1, 0)];

        // When
        let reached = sites[0].reached_files(&sites);

        // Then
        assert_eq!(reached, BTreeSet::from([FileId::new(0), FileId::new(1)]));
    }
}
