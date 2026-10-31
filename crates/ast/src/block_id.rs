//! A handle to where a block of the document was written.

use serde::{Deserialize, Serialize};

/// Which written block of one parse a node is — a paragraph, a list item, a
/// section's title, a directive's marker, a glossary term: the row of its
/// [`SourceMap`](crate::SourceMap) holding the block's lines, where an
/// editor's outline, a definition and a preview's scroll position lead.
///
/// Only [`SourceMap::record_block_lines`](crate::SourceMap::record_block_lines)
/// makes one, so an id always has a row in the map it was recorded in. It is
/// part of the `.ast` — a build phase reading one back holds the id but no
/// map, and so cannot ask where the block stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlockId(u32);

impl BlockId {
    /// The id of the `index`th block a source map records.
    pub(crate) const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The row of the source map this id names.
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_an_id_names_its_row() {
        // Given / When / Then
        assert_eq!(BlockId::new(7).index(), 7);
    }

    #[test]
    fn test_an_id_reads_back_from_its_number() {
        // Given
        let id = BlockId::new(3);

        // When
        let json = serde_json::to_string(&id).expect("serializes");

        // Then
        assert_eq!(json, "3");
        assert_eq!(serde_json::from_str::<BlockId>(&json).expect("reads"), id);
    }
}
