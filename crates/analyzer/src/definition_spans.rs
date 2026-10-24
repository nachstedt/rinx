//! The spans of one document's definitions, collected by the indexing walk
//! as it records each name, and resolved into the [`DefinitionSites`] a
//! [`crate::DocumentAnalysis`] keeps beside its index fragment.
//!
//! Recorded at the very place the walk inserts a name into the index, so a
//! site exists under exactly the key the index names the definition by —
//! scope qualification, an option's several names and `:no-index:` included
//! — without a second copy of those rules (`docs/decisions/045-definition-spans.md`).

use std::collections::BTreeMap;

use rinx_ast::{Document, Span};
use rinx_index::{DefinitionKey, DefinitionSite, DefinitionSites};

/// The spans met so far, by what the index names each definition.
#[derive(Debug, Default)]
pub(crate) struct DefinitionSpans(BTreeMap<DefinitionKey, Span>);

impl DefinitionSpans {
    /// Records where `key` was defined. A definition the parser gave no span
    /// records nothing, and a later one under the same key wins, as a later
    /// insertion into the index's own maps does.
    pub(crate) fn record(&mut self, key: DefinitionKey, span: Option<Span>) {
        if let Some(span) = span {
            self.0.insert(key, span);
        }
    }

    /// The sites of `doc`'s definitions, each naming the fragment it was
    /// written in by its path rather than by `doc`'s file id, which means
    /// nothing outside `doc`. A span naming a file `doc` does not list is
    /// dropped: there would be nowhere to lead.
    pub(crate) fn into_sites(self, doc: &Document) -> DefinitionSites {
        let mut sites = DefinitionSites::default();
        for (key, span) in self.0 {
            let file = match span.file {
                None => None,
                Some(_) => match doc.span_path(Some(span)) {
                    Some(path) => Some(path.to_string()),
                    None => continue,
                },
            };
            sites.insert(
                key,
                DefinitionSite {
                    file,
                    start: span.start,
                    end: span.end,
                },
            );
        }
        sites
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{FileId, Position, TargetName};

    fn span(line: u32) -> Span {
        Span::new(Position::new(line, 1), Position::new(line, 8))
    }

    fn label(name: &str) -> DefinitionKey {
        DefinitionKey::Label(TargetName::new(name))
    }

    #[test]
    fn test_record_ignores_a_definition_without_a_span() {
        // Given
        let mut spans = DefinitionSpans::default();

        // When
        spans.record(label("install"), None);

        // Then
        let doc = Document::new("guide.rst".to_string(), vec![]);
        assert!(spans.into_sites(&doc).is_empty());
    }

    #[test]
    fn test_record_keeps_the_later_span_of_a_key_recorded_twice() {
        // Given
        let mut spans = DefinitionSpans::default();

        // When
        spans.record(label("install"), Some(span(2)));
        spans.record(label("install"), Some(span(9)));

        // Then
        let doc = Document::new("guide.rst".to_string(), vec![]);
        let sites = spans.into_sites(&doc);
        assert_eq!(
            sites.get(&label("install")).map(|site| site.start.line),
            Some(9)
        );
    }

    #[test]
    fn test_into_sites_names_a_fragment_by_its_path_and_the_document_by_none() {
        // Given
        let mut doc = Document::new("guide.rst".to_string(), vec![]);
        let fragment = doc.intern_source_file("shared/install.rst");
        let mut spans = DefinitionSpans::default();
        spans.record(label("own"), Some(span(1)));
        spans.record(label("included"), Some(span(4).with_file(Some(fragment))));

        // When
        let sites = spans.into_sites(&doc);

        // Then
        assert_eq!(
            sites.get(&label("own")),
            Some(&DefinitionSite {
                file: None,
                start: Position::new(1, 1),
                end: Position::new(1, 8),
            })
        );
        assert_eq!(
            sites
                .get(&label("included"))
                .and_then(|site| site.file.as_deref()),
            Some("shared/install.rst")
        );
    }

    #[test]
    fn test_into_sites_drops_a_span_naming_a_file_the_document_does_not_list() {
        // Given
        let doc = Document::new("guide.rst".to_string(), vec![]);
        let mut spans = DefinitionSpans::default();
        spans.record(label("lost"), Some(span(1).with_file(Some(FileId::new(3)))));

        // When / Then
        assert!(spans.into_sites(&doc).is_empty());
    }
}
