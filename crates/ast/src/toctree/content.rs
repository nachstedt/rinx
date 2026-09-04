use serde::{Deserialize, Serialize};

use super::{TocEntry, ToctreeOptions};

/// One `.. toctree::` directive: its entries, in the order written, and the
/// options that apply to all of them.
///
/// Entries are stored **unexpanded** — a [`TocEntry::Glob`] survives into the
/// serialized index rather than being resolved when it is parsed. The parser
/// has no project-wide document list to expand it against, and, more
/// importantly, the three phases that later need the expansion each have a
/// *different* list to expand against: the Bazel strict-deps validator uses
/// the declared dependencies, the analyzer uses every document in the project,
/// and the renderer uses the documents the index knows a title for. Expanding
/// once, early, would force the strict-deps check to reimplement the matcher
/// against its own narrower list, which is exactly the check that must not
/// drift.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toctree {
    pub entries: Vec<TocEntry>,
    #[serde(default)]
    pub options: ToctreeOptions,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToctreeFlag;

    #[test]
    fn test_toctree_round_trips_through_json() {
        // Given
        let toctree = Toctree {
            entries: vec![
                TocEntry::Document {
                    title: None,
                    docname: "intro".to_string(),
                    span: None,
                },
                TocEntry::Glob {
                    pattern: "api/*".to_string(),
                    span: None,
                },
            ],
            options: {
                let mut options = ToctreeOptions::default();
                options.set(ToctreeFlag::Glob);
                options
            },
        };

        // When
        let json = serde_json::to_string(&toctree).expect("serializes");
        let restored: Toctree = serde_json::from_str(&json).expect("deserializes");

        // Then — the glob is still a glob, not a resolved document list.
        assert_eq!(restored, toctree);
    }

    #[test]
    fn test_default_toctree_has_no_entries() {
        // Given / When
        let toctree = Toctree::default();

        // Then
        assert!(toctree.entries.is_empty());
        assert_eq!(toctree.options, ToctreeOptions::default());
    }
}
