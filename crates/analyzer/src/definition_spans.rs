//! The spans of one document's definitions, collected by the indexing walk
//! as it records each name, and resolved into the [`DefinitionSites`] a
//! [`crate::DocumentAnalysis`] keeps beside its index fragment.
//!
//! Recorded at the very place the walk inserts a name into the index, so a
//! site exists under exactly the key the index names the definition by —
//! scope qualification, an option's several names and `:no-index:` included
//! — without a second copy of those rules (`docs/decisions/045-definition-spans.md`).

use std::collections::BTreeMap;

use rinx_ast::{BlockId, Document, Named, SourceMap, Span};
use rinx_index::{DefinitionKey, DefinitionSite, DefinitionSites, NameRange};

/// The spans met so far, by what the index names each definition: its own,
/// and its name's when it has one — looked up in the source map of the
/// parse, which a build, holding a document read from a `.ast`, does not
/// have.
#[derive(Debug, Default)]
pub(crate) struct DefinitionSpans<'m> {
    source_map: Option<&'m SourceMap>,
    spans: BTreeMap<DefinitionKey, (Span, Option<Span>)>,
}

impl<'m> DefinitionSpans<'m> {
    /// No spans yet, finding names' characters in `source_map` when there is
    /// one.
    pub(crate) fn new(source_map: Option<&'m SourceMap>) -> Self {
        Self {
            source_map,
            spans: BTreeMap::new(),
        }
    }

    /// Exactly the characters `name` was written as, when the parse recorded
    /// them and its source map is at hand.
    pub(crate) fn name_span<T>(&self, name: &Named<T>) -> Option<Span> {
        self.source_map?.name_span(name.id()?)
    }

    /// The lines the block `block` was written over, when the parse recorded
    /// them and its source map is at hand.
    pub(crate) fn block_lines(&self, block: Option<BlockId>) -> Option<Span> {
        self.source_map?.block_lines(block?)
    }

    /// Records where `key` was defined. A definition the parser gave no span
    /// records nothing, and a later one under the same key wins, as a later
    /// insertion into the index's own maps does.
    pub(crate) fn record(&mut self, key: DefinitionKey, span: Option<Span>) {
        if let Some(span) = span {
            self.spans.insert(key, (span, None));
        }
    }

    /// Records where `key` was defined, as [`Self::record`] does, and where
    /// its `name` was written within that.
    pub(crate) fn record_named<T>(
        &mut self,
        key: DefinitionKey,
        span: Option<Span>,
        name: &Named<T>,
    ) {
        if let Some(span) = span {
            let name = self.name_span(name);
            self.spans.insert(key, (span, name));
        }
    }

    /// The sites of `doc`'s definitions, each naming the fragment it was
    /// written in by its path rather than by `doc`'s file id, which means
    /// nothing outside `doc`. A span naming a file `doc` does not list is
    /// dropped: there would be nowhere to lead.
    pub(crate) fn into_sites(self, doc: &Document) -> DefinitionSites {
        let mut sites = DefinitionSites::default();
        for (key, (span, name)) in self.spans {
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
                    // A name is written within its definition, so in its file.
                    name: name
                        .filter(|name| name.file == span.file)
                        .map(|name| NameRange {
                            start: name.start,
                            end: name.end,
                        }),
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
        let mut spans = DefinitionSpans::new(None);

        // When
        spans.record(label("install"), None);

        // Then
        let doc = Document::new("guide.rst".to_string(), vec![]);
        assert!(spans.into_sites(&doc).is_empty());
    }

    #[test]
    fn test_record_keeps_the_later_span_of_a_key_recorded_twice() {
        // Given
        let mut spans = DefinitionSpans::new(None);

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
        let mut spans = DefinitionSpans::new(None);
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
                name: None,
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
    fn test_record_named_keeps_where_the_name_was_written() {
        // Given — `.. _install:` with its name in columns 5 to 12
        let mut map = SourceMap::default();
        let name = map.record_name(Span::new(Position::new(2, 5), Position::new(2, 12)));
        let mut spans = DefinitionSpans::new(Some(&map));

        // When
        spans.record_named(
            label("install"),
            Some(span(2)),
            &Named::new(TargetName::new("install"), Some(name)),
        );

        // Then
        let doc = Document::new("guide.rst".to_string(), vec![]);
        assert_eq!(
            spans
                .into_sites(&doc)
                .get(&label("install"))
                .and_then(|site| site.name),
            Some(NameRange {
                start: Position::new(2, 5),
                end: Position::new(2, 12),
            })
        );
    }

    #[test]
    fn test_record_named_records_no_name_without_a_source_map() {
        // Given — a name the parse recorded, but no map to find it in, as in
        // a build reading its document from a `.ast`
        let mut map = SourceMap::default();
        let name = map.record_name(Span::new(Position::new(2, 5), Position::new(2, 12)));
        let mut spans = DefinitionSpans::new(None);

        // When
        spans.record_named(
            label("install"),
            Some(span(2)),
            &Named::new(TargetName::new("install"), Some(name)),
        );

        // Then
        let doc = Document::new("guide.rst".to_string(), vec![]);
        let sites = spans.into_sites(&doc);
        let site = sites.get(&label("install")).expect("a site");
        assert_eq!((site.start.line, site.name), (2, None));
    }

    #[test]
    fn test_record_named_records_no_name_the_parser_did_not_place() {
        // Given
        let mut spans = DefinitionSpans::new(None);

        // When
        spans.record_named(
            label("install"),
            Some(span(2)),
            &Named::unplaced(TargetName::new("install")),
        );

        // Then
        let doc = Document::new("guide.rst".to_string(), vec![]);
        let sites = spans.into_sites(&doc);
        let site = sites.get(&label("install")).expect("a site");
        assert_eq!((site.start.line, site.name), (2, None));
    }

    #[test]
    fn test_record_named_ignores_a_definition_without_a_span() {
        // Given
        let mut spans = DefinitionSpans::new(None);

        // When
        spans.record_named(
            label("install"),
            None,
            &Named::unplaced(TargetName::new("install")),
        );

        // Then
        let doc = Document::new("guide.rst".to_string(), vec![]);
        assert!(spans.into_sites(&doc).is_empty());
    }

    #[test]
    fn test_into_sites_drops_a_span_naming_a_file_the_document_does_not_list() {
        // Given
        let doc = Document::new("guide.rst".to_string(), vec![]);
        let mut spans = DefinitionSpans::new(None);
        spans.record(label("lost"), Some(span(1).with_file(Some(FileId::new(3)))));

        // When / Then
        assert!(spans.into_sites(&doc).is_empty());
    }
}
