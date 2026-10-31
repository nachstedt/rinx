//! What each of a project's documents defines, named as a search across the
//! workspace lists it (`workspace/symbol`): its sections by their titles, its
//! labels, terms, equations and entities by their names, and its domain
//! objects as their definitions spelled them.
//!
//! Read off the document's own analysis — where each definition was written
//! and the document's own index fragment — rather than the project's fold, so
//! a name two documents define is listed under both, where the fold keeps it
//! in neither, and listing needs no fold at all. Built once per analysis,
//! beside it, so a search only matches names.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lsp_types::SymbolKind;
use rinx_analyzer::DocumentAnalysis;
use rinx_ast::{
    Directive, Document, Node, Position, SectionId, TargetName, allocate_section_ids,
    inline_plain_text, walk_nodes,
};
use rinx_index::{DefinitionKey, DefinitionSite};

use super::index::site_path;
use crate::diagnostics::{RST_EPILOG, RST_PROLOG};
use crate::document_symbol::{non_empty, symbol_kind};

/// One definition of a document, as a workspace search lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolEntry {
    pub name: String,
    pub kind: SymbolKind,
    /// The file it was written in, by its source-root path, when that is an
    /// `.. include::`d fragment rather than the document itself.
    pub file: Option<String>,
    /// Its first character, in that file.
    pub start: Position,
    /// Just past its last character, in that file.
    pub end: Position,
}

/// Every definition one document wrote with a known position, as a
/// workspace search lists them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentSymbols(Vec<SymbolEntry>);

impl DocumentSymbols {
    /// What `document`, analysed as `analysis`, defines.
    #[must_use]
    pub fn of(document: &Document, analysis: &DocumentAnalysis) -> Self {
        let sites = analysis.definition_sites();
        let titles = section_titles(document);
        let terms = term_spellings(document);
        let fragment = analysis.index_fragment();
        // An entity is a label too, recorded at the same site; every
        // reference to it names the entity, so it is listed once, as one.
        let entity_labels: BTreeMap<TargetName, Position> = sites
            .iter()
            .filter_map(|(key, site)| match key {
                DefinitionKey::Entity(id) => Some((TargetName::new(id.as_str()), site.start)),
                _ => None,
            })
            .collect();
        let entries = sites
            .iter()
            .filter(|(_, site)| !in_no_file(site))
            .filter_map(|(key, site)| {
                let (name, kind) = match key {
                    DefinitionKey::Section(id) => (section_title(&titles, id), SymbolKind::STRING),
                    DefinitionKey::Label(name) => {
                        if entity_labels.get(name) == Some(&site.start) {
                            return None;
                        }
                        (name.as_str().to_string(), SymbolKind::KEY)
                    }
                    DefinitionKey::Term(name) => (
                        terms
                            .get(name)
                            .map_or(name.as_str(), String::as_str)
                            .to_string(),
                        SymbolKind::ENUM_MEMBER,
                    ),
                    DefinitionKey::Object(object_type, name) => (
                        fragment.domain_object_spelling(name).to_string(),
                        symbol_kind(*object_type),
                    ),
                    DefinitionKey::Equation(name) => {
                        (name.as_str().to_string(), SymbolKind::NUMBER)
                    }
                    DefinitionKey::Entity(id) => (id.as_str().to_string(), SymbolKind::OBJECT),
                };
                Some(SymbolEntry {
                    name,
                    kind,
                    file: site.file.clone(),
                    start: site.start,
                    end: site.end,
                })
            })
            .collect();
        Self(entries)
    }

    /// Every entry, in the order the analysis keys them.
    pub fn iter(&self) -> impl Iterator<Item = &SymbolEntry> {
        self.0.iter()
    }
}

/// One entry of a project's documents, with what places it.
#[derive(Debug, Clone, Copy)]
pub struct SymbolCandidate<'a> {
    /// The name of the document defining it.
    pub doc_path: &'a str,
    pub entry: &'a SymbolEntry,
    /// The document's file.
    pub document: &'a Path,
    /// The project's source root, which a fragment's name is relative to.
    pub source_root: &'a Path,
}

impl SymbolCandidate<'_> {
    /// The file the entry was written in: the document's, or the
    /// `.. include::`d fragment's.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        site_path(self.document, self.source_root, self.entry.file.as_deref())
    }
}

/// Whether `site` was written in a `conf.py`'s `rst_prolog` or `rst_epilog`,
/// which no file holds, so no search can lead there.
fn in_no_file(site: &DefinitionSite) -> bool {
    site.file
        .as_deref()
        .is_some_and(|file| file == RST_PROLOG || file == RST_EPILOG)
}

/// Each section of `document` by the id its anchor is, with its title — from
/// the same [`allocate_section_ids`] call the analyzer records the sites by.
fn section_titles(document: &Document) -> BTreeMap<SectionId, String> {
    allocate_section_ids(&document.nodes)
        .into_iter()
        .filter_map(|(position, id)| match document.nodes.get(position) {
            Some(Node::Heading { text, .. }) => Some((id, inline_plain_text(text))),
            _ => None,
        })
        .collect()
}

/// Each glossary term of `document` by the name the index keys it under,
/// with the spelling it was written in — the later of two, as the index
/// keeps the later definition.
fn term_spellings(document: &Document) -> BTreeMap<TargetName, String> {
    let mut spellings = BTreeMap::new();
    walk_nodes(&document.nodes, &mut |node| {
        if let Node::Directive(Directive::Glossary { entries, .. }) = node {
            for term in entries.iter().flat_map(|entry| &entry.terms) {
                spellings.insert(TargetName::new(&term.text), term.text.clone());
            }
        }
    });
    spellings
}

/// The title of the section `id`, or its id when it has none to show.
fn section_title(titles: &BTreeMap<SectionId, String>, id: &SectionId) -> String {
    let title = titles.get(id).cloned().unwrap_or_default();
    non_empty(title, id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{EntityBody, EntityId, Span};

    fn symbols_of(text: &str) -> Vec<(String, SymbolKind)> {
        let document =
            rinx_parser::parse("guide.rst", text, &rinx_parser::ParseCtx::default()).document;
        DocumentSymbols::of(&document, &DocumentAnalysis::of(&document, None))
            .iter()
            .map(|entry| (entry.name.clone(), entry.kind))
            .collect()
    }

    #[test]
    fn test_of_names_each_kind_of_definition() {
        // Given
        let text = "\
.. _install:

Guide
=====

.. glossary::

   Build
      What rinx does.

.. py:class:: pkg.Greeter

.. math::
   :label: euler

   e^{i\\pi} = -1
";

        // When
        let mut symbols = symbols_of(text);
        symbols.sort_by(|(a, _), (b, _)| a.cmp(b));

        // Then — a term and a domain object as their definitions spelled them
        assert_eq!(
            symbols,
            [
                ("Build".to_string(), SymbolKind::ENUM_MEMBER),
                ("Guide".to_string(), SymbolKind::STRING),
                ("euler".to_string(), SymbolKind::NUMBER),
                ("install".to_string(), SymbolKind::KEY),
                ("pkg.Greeter".to_string(), SymbolKind::CLASS),
            ]
        );
    }

    #[test]
    fn test_of_places_each_entry_at_its_site() {
        // Given
        let document = rinx_parser::parse(
            "guide.rst",
            "Intro.\n\n.. _install:\n\nText.\n",
            &rinx_parser::ParseCtx::default(),
        )
        .document;

        // When
        let symbols = DocumentSymbols::of(&document, &DocumentAnalysis::of(&document, None));

        // Then
        let entry = symbols.iter().next().expect("the label");
        assert_eq!((entry.start.line, entry.file.as_deref()), (3, None));
    }

    #[test]
    fn test_of_lists_an_entity_once_as_an_entity() {
        // Given
        let entity = EntityBody {
            type_name: "req".to_string(),
            id: EntityId::new("REQ_1").unwrap().into(),
            attributes: BTreeMap::new(),
            relations: BTreeMap::new(),
            sections: vec![],
            span: Some(Span::new(Position::new(1, 1), Position::new(1, 13))),
            relation_spans: BTreeMap::new(),
        };
        let document = Document::new(
            "reqs.rst".to_string(),
            vec![Node::Directive(Directive::Entity(Box::new(entity)))],
        );

        // When
        let symbols = DocumentSymbols::of(&document, &DocumentAnalysis::of(&document, None));

        // Then
        let listed: Vec<(&str, SymbolKind)> = symbols
            .iter()
            .map(|entry| (entry.name.as_str(), entry.kind))
            .collect();
        assert_eq!(listed, [("REQ_1", SymbolKind::OBJECT)]);
    }

    #[test]
    fn test_of_leaves_out_what_a_prolog_defines() {
        // Given — a label in `conf.py`'s `rst_prolog`, which no file holds
        let mut document = Document::new("guide.rst".to_string(), Vec::new());
        let prolog = document.intern_source_file(RST_PROLOG);
        let span = Span::new(Position::new(1, 1), Position::new(1, 12)).with_file(Some(prolog));
        document.nodes = vec![Node::Target {
            name: TargetName::new("top").into(),
            destination: None,
            span: Some(span),
        }];

        // When
        let symbols = DocumentSymbols::of(&document, &DocumentAnalysis::of(&document, None));

        // Then
        assert_eq!(symbols.iter().count(), 0, "{symbols:?}");
    }

    #[test]
    fn test_section_titles_names_each_section_by_its_anchor() {
        // Given
        let document = rinx_parser::parse(
            "guide.rst",
            "Guide\n=====\n\nUsage\n-----\n",
            &rinx_parser::ParseCtx::default(),
        )
        .document;

        // When
        let titles = section_titles(&document);

        // Then
        assert_eq!(
            titles
                .get(&SectionId::from_title("Usage"))
                .map(String::as_str),
            Some("Usage")
        );
        assert_eq!(titles.len(), 2);
    }

    #[test]
    fn test_term_spellings_keeps_each_term_as_written() {
        // Given — a glossary inside a note
        let document = rinx_parser::parse(
            "guide.rst",
            ".. note::\n\n   .. glossary::\n\n      Build Step\n         A step.\n",
            &rinx_parser::ParseCtx::default(),
        )
        .document;

        // When
        let spellings = term_spellings(&document);

        // Then
        assert_eq!(
            spellings
                .get(&TargetName::new("build step"))
                .map(String::as_str),
            Some("Build Step")
        );
    }

    #[test]
    fn test_section_title_falls_back_to_the_id() {
        // Given
        let titles = BTreeMap::from([(SectionId::from_title("Usage"), "  ".to_string())]);

        // When / Then
        assert_eq!(
            section_title(&titles, &SectionId::from_title("Usage")),
            "usage"
        );
        assert_eq!(
            section_title(&titles, &SectionId::from_title("Other")),
            "other"
        );
    }

    #[test]
    fn test_in_no_file_holds_only_for_the_prolog_and_epilog() {
        // Given
        let site = |file: Option<&str>| DefinitionSite {
            file: file.map(str::to_string),
            start: Position::new(1, 1),
            end: Position::new(1, 2),
            name: None,
        };

        // When / Then
        assert!(in_no_file(&site(Some(RST_PROLOG))));
        assert!(in_no_file(&site(Some(RST_EPILOG))));
        assert!(!in_no_file(&site(Some("part.rst"))));
        assert!(!in_no_file(&site(None)));
    }
}
