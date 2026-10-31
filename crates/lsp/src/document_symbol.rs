//! The outline of an open document, as `textDocument/documentSymbol` answers
//! it: its sections nested by level, and in them the domain objects and
//! entities it defines — a class's methods under the class.
//!
//! Read from the document's latest parse, so the outline never waits for the
//! render tier, and placed by the spans the parser records on each definition
//! (`docs/decisions/045-definition-spans.md`). Only a top-level heading is a
//! section, as in the analyzer's outline. A definition an `.. include::`
//! brought in is placed on that `.. include::` line, since every range of an
//! outline lies in its own document.
//!
//! The symbols are this module's own types rather than `lsp_types`'
//! `DocumentSymbol` and `SymbolInformation`, which cannot be built without
//! naming their deprecated `deprecated` field. They serialize to the same
//! JSON.

use lsp_types::{Location, Range, SymbolKind, Uri};
use rinx_ast::{
    BlockKind, CObjectType, Child, Directive, Document, DomainObjectBody, Node, ObjectType,
    Position, PyObjectType, Span, StdObjectType, for_each_child, inline_plain_text,
};
use rinx_scope::{DefinitionNames, DocumentScopes};
use serde::{Deserialize, Serialize};

use crate::position::{PositionEncoding, to_lsp_position, to_lsp_range};

/// One entry of a nested outline: `lsp_types::DocumentSymbol`'s shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineSymbol {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub kind: SymbolKind,
    /// Everything the symbol spans: a section up to the next one, a
    /// directive with its body. Holds `selection_range` and every child's.
    pub range: Range,
    /// What names the symbol: a section's title, a directive's marker line.
    pub selection_range: Range,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<OutlineSymbol>,
}

/// One entry of a flat outline, for a client that cannot take a nested one:
/// `lsp_types::SymbolInformation`'s shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub location: Location,
    /// The name of the symbol holding it, when one does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
}

/// How a client takes an outline: nested, when it announces
/// `textDocument.documentSymbol.hierarchicalDocumentSymbolSupport`, else flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineShape {
    Nested,
    Flat,
}

/// The answer to `textDocument/documentSymbol`, in whichever shape the client
/// takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum DocumentSymbols {
    Nested(Vec<OutlineSymbol>),
    Flat(Vec<FlatSymbol>),
}

/// The outline of `document`, parsed from `text`, with positions in
/// `encoding`.
#[must_use]
pub fn document_outline(
    document: &Document,
    text: &str,
    encoding: PositionEncoding,
) -> Vec<OutlineSymbol> {
    let lines: Vec<&str> = text.lines().collect();
    let outline = Outline {
        document,
        text,
        lines: &lines,
        encoding,
        scopes: DocumentScopes::of(&document.nodes),
    };
    let mut roots = outline.sections();
    for symbol in &mut roots {
        contain(symbol);
    }
    roots
}

/// The outline of `document`, parsed from `text` and opened at `uri`, in
/// `shape` and with positions in `encoding`.
#[must_use]
pub fn document_symbols(
    document: &Document,
    text: &str,
    uri: &Uri,
    shape: OutlineShape,
    encoding: PositionEncoding,
) -> DocumentSymbols {
    let outline = document_outline(document, text, encoding);
    match shape {
        OutlineShape::Nested => DocumentSymbols::Nested(outline),
        OutlineShape::Flat => DocumentSymbols::Flat(flatten(outline, uri)),
    }
}

/// `symbols`, each followed by what it holds, each held one naming its
/// holder — the flat outline a client without nested ones takes, its
/// locations in the document at `uri`.
#[must_use]
pub fn flatten(symbols: Vec<OutlineSymbol>, uri: &Uri) -> Vec<FlatSymbol> {
    let mut flat = Vec::new();
    push_flat(symbols, None, uri, &mut flat);
    flat
}

fn push_flat(
    symbols: Vec<OutlineSymbol>,
    container: Option<&str>,
    uri: &Uri,
    flat: &mut Vec<FlatSymbol>,
) {
    for symbol in symbols {
        flat.push(FlatSymbol {
            name: symbol.name.clone(),
            kind: symbol.kind,
            location: Location::new(uri.clone(), symbol.range),
            container_name: container.map(str::to_string),
        });
        push_flat(symbol.children, Some(&symbol.name), uri, flat);
    }
}

/// Widens `symbol`'s range, and each of its children's, to hold what they
/// must: its selection and every child. Already so for a document's own
/// text; a definition placed on the `.. include::` that brought it in may
/// fall outside the section it closes.
fn contain(symbol: &mut OutlineSymbol) {
    let mut range = Range::new(
        symbol.range.start.min(symbol.selection_range.start),
        symbol.range.end.max(symbol.selection_range.end),
    );
    for child in &mut symbol.children {
        contain(child);
        range.start = range.start.min(child.range.start);
        range.end = range.end.max(child.range.end);
    }
    symbol.range = range;
}

/// What building one document's outline reads.
struct Outline<'a> {
    document: &'a Document,
    text: &'a str,
    lines: &'a [&'a str],
    encoding: PositionEncoding,
    scopes: DocumentScopes<'a>,
}

impl Outline<'_> {
    /// The document's sections, nested by level, each holding the symbols
    /// written under it; the symbols before the first section come first.
    fn sections(&self) -> Vec<OutlineSymbol> {
        // Each open section, with its level: a heading closes every open one
        // at its own level or deeper, as the analyzer's outline nests them.
        let mut open: Vec<(u8, OutlineSymbol)> = Vec::new();
        let mut roots: Vec<OutlineSymbol> = Vec::new();
        for node in &self.document.nodes {
            if let Node::Heading { level, text, span } = node {
                let Some(title) = self.placed(*span) else {
                    continue;
                };
                let selection = self.range_of(title);
                let closed_at = lsp_types::Position::new(selection.start.line, 0);
                while open.last().is_some_and(|(at, _)| *at >= *level) {
                    close(&mut open, &mut roots, closed_at);
                }
                open.push((
                    *level,
                    OutlineSymbol {
                        name: non_empty(inline_plain_text(text), "(untitled)"),
                        detail: None,
                        kind: SymbolKind::STRING,
                        range: selection,
                        selection_range: selection,
                        children: Vec::new(),
                    },
                ));
            } else {
                let held = match open.last_mut() {
                    Some((_, section)) => &mut section.children,
                    None => &mut roots,
                };
                self.symbols_of(node, held);
            }
        }
        let end = self.document_end();
        while !open.is_empty() {
            close(&mut open, &mut roots, end);
        }
        roots
    }

    /// Appends to `out` the symbols `node` defines, nested as they are.
    fn symbols_of(&self, node: &Node, out: &mut Vec<OutlineSymbol>) {
        match node {
            Node::Directive(Directive::DomainObject { object, span }) => {
                let mut children = Vec::new();
                for child in object.body() {
                    self.symbols_of(child, &mut children);
                }
                match self.object_symbol(object, *span, children) {
                    Ok(symbol) => out.push(symbol),
                    // An object with no position still holds what it holds.
                    Err(children) => out.extend(children),
                }
            }
            Node::Directive(Directive::Entity(entity)) => {
                let mut children = Vec::new();
                for section in &entity.sections {
                    for child in &section.body {
                        self.symbols_of(child, &mut children);
                    }
                }
                match self.placed(entity.span) {
                    Some(marker) => out.push(OutlineSymbol {
                        name: entity.id.value().as_str().to_string(),
                        detail: Some(entity.type_name.clone()),
                        kind: SymbolKind::OBJECT,
                        range: self.block_range(marker),
                        selection_range: self.range_of(marker),
                        children,
                    }),
                    None => out.extend(children),
                }
            }
            _ => for_each_child(node, &mut |child| {
                if let Child::Nodes(nodes) = child {
                    for nested in nodes {
                        self.symbols_of(nested, out);
                    }
                }
            }),
        }
    }

    /// The symbol of `object`, written at `span` and holding `children` —
    /// or, when its position is not known, `children` back.
    fn object_symbol(
        &self,
        object: &DomainObjectBody,
        span: Option<Span>,
        children: Vec<OutlineSymbol>,
    ) -> Result<OutlineSymbol, Vec<OutlineSymbol>> {
        let Some(marker) = self.placed(span) else {
            return Err(children);
        };
        let object_type = object.object_type();
        let name = match &*self.scopes.definition(object) {
            DefinitionNames::Object(names) => names.first().clone(),
            DefinitionNames::Options(lines) => lines
                .iter()
                .flatten()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        };
        Ok(OutlineSymbol {
            name: non_empty(name, object_type.as_str()),
            detail: Some(object_type.domain_qualified_str()),
            kind: symbol_kind(object_type),
            range: self.block_range(marker),
            selection_range: self.range_of(marker),
            children,
        })
    }

    /// Where `span` lies in the document: the span itself, or the
    /// `.. include::` the document wrote that brought its file in — `None`
    /// for no span, or a file no such include brought in.
    fn placed(&self, span: Option<Span>) -> Option<Span> {
        let span = span?;
        let Some(file) = span.file else {
            return Some(span);
        };
        let sites = &self.document.include_sites;
        sites
            .iter()
            .filter(|site| {
                site.directive
                    .is_some_and(|directive| directive.file.is_none())
            })
            .find(|site| site.reached_files(sites).contains(&file))
            .and_then(|site| site.directive)
    }

    fn range_of(&self, span: Span) -> Range {
        to_lsp_range(span, self.text, self.encoding)
    }

    /// The range of the directive whose marker is `marker`: its whole block,
    /// as the parser read it, up to its last written line — or the marker
    /// alone when the parser recorded no block there.
    fn block_range(&self, marker: Span) -> Range {
        let block = self.document.syntax.blocks.iter().find(|block| {
            block.kind == BlockKind::Directive
                && block.lines.file == marker.file
                && block.lines.start.line == marker.start.line
        });
        match block {
            Some(block) => Range::new(
                self.range_of(marker).start,
                self.line_end(block.lines.end.line.saturating_sub(1) as usize),
            ),
            None => self.range_of(marker),
        }
    }

    /// Just past the last character of the line at index `at`.
    fn line_end(&self, at: usize) -> lsp_types::Position {
        let text = self.lines.get(at).copied().unwrap_or("");
        let line = u32::try_from(at + 1).unwrap_or(u32::MAX);
        let column = u32::try_from(text.chars().count() + 1).unwrap_or(u32::MAX);
        to_lsp_position(Position::new(line, column), text, self.encoding)
    }

    /// Just past the document's last character.
    fn document_end(&self) -> lsp_types::Position {
        match self.lines.len() {
            0 => lsp_types::Position::new(0, 0),
            count => self.line_end(count - 1),
        }
    }
}

/// Moves the innermost open section into what holds it, ending its range at
/// `end` unless what it holds reaches further.
fn close(
    open: &mut Vec<(u8, OutlineSymbol)>,
    roots: &mut Vec<OutlineSymbol>,
    end: lsp_types::Position,
) {
    let Some((_, mut section)) = open.pop() else {
        return;
    };
    section.range.end = end.max(section.selection_range.end);
    match open.last_mut() {
        Some((_, parent)) => parent.children.push(section),
        None => roots.push(section),
    }
}

/// `name`, or `fallback` when it is empty: a client refuses a symbol with no
/// name.
pub(crate) fn non_empty(name: String, fallback: &str) -> String {
    if name.trim().is_empty() {
        fallback.to_string()
    } else {
        name
    }
}

/// How the editor marks an object of `object_type`.
pub(crate) fn symbol_kind(object_type: ObjectType) -> SymbolKind {
    match object_type {
        ObjectType::Py(PyObjectType::Function) | ObjectType::C(CObjectType::Function) => {
            SymbolKind::FUNCTION
        }
        ObjectType::Py(PyObjectType::Method) => SymbolKind::METHOD,
        ObjectType::Py(PyObjectType::Class | PyObjectType::Exception) => SymbolKind::CLASS,
        ObjectType::Py(PyObjectType::Module) => SymbolKind::MODULE,
        ObjectType::Py(PyObjectType::Data) => SymbolKind::VARIABLE,
        ObjectType::Py(PyObjectType::Attribute) => SymbolKind::PROPERTY,
        ObjectType::C(CObjectType::Macro) => SymbolKind::CONSTANT,
        ObjectType::C(CObjectType::Struct | CObjectType::Union) => SymbolKind::STRUCT,
        ObjectType::C(CObjectType::Member) => SymbolKind::FIELD,
        ObjectType::C(CObjectType::Type) => SymbolKind::TYPE_PARAMETER,
        ObjectType::Std(StdObjectType::Cmdoption) => SymbolKind::KEY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{EntityBody, EntityId, IncludeSite, InlineNode};
    use std::collections::BTreeMap;
    use std::str::FromStr;

    fn outline_of(text: &str) -> Vec<OutlineSymbol> {
        let document = rinx_parser::parse("guide.rst", text);
        document_outline(&document, text, PositionEncoding::Utf16)
    }

    fn range(start: (u32, u32), end: (u32, u32)) -> Range {
        Range::new(
            lsp_types::Position::new(start.0, start.1),
            lsp_types::Position::new(end.0, end.1),
        )
    }

    /// Each symbol's name, nested as the outline nests them.
    fn tree(symbols: &[OutlineSymbol]) -> Vec<(String, Vec<String>)> {
        symbols
            .iter()
            .map(|symbol| {
                let mut below = Vec::new();
                collect_names(&symbol.children, &mut below);
                (symbol.name.clone(), below)
            })
            .collect()
    }

    fn collect_names(symbols: &[OutlineSymbol], names: &mut Vec<String>) {
        for symbol in symbols {
            names.push(symbol.name.clone());
            collect_names(&symbol.children, names);
        }
    }

    /// Asserts what a client checks of every symbol: its selection and its
    /// children lie in its range.
    fn assert_contained(symbols: &[OutlineSymbol]) {
        for symbol in symbols {
            let holds =
                |inner: &Range| symbol.range.start <= inner.start && inner.end <= symbol.range.end;
            assert!(holds(&symbol.selection_range), "{symbol:?}");
            for child in &symbol.children {
                assert!(holds(&child.range), "{symbol:?} does not hold {child:?}");
            }
            assert_contained(&symbol.children);
        }
    }

    fn heading(level: u8, title: &str, span: Option<Span>) -> Node {
        Node::Heading {
            level,
            text: vec![InlineNode::Text(title.to_string())],
            span,
        }
    }

    fn span_at(line: u32, columns: u32) -> Span {
        Span::new(Position::new(line, 1), Position::new(line, columns + 1))
    }

    #[test]
    fn test_document_outline_nests_sections_by_level_up_to_the_next_section() {
        // Given
        let text = "Guide\n=====\n\nIntro.\n\nInstall\n-------\n\nText.\n\nUsage\n-----\n\nMore.\n";

        // When
        let outline = outline_of(text);

        // Then
        assert_eq!(outline.len(), 1);
        let guide = &outline[0];
        assert_eq!(guide.name, "Guide");
        assert_eq!(guide.kind, SymbolKind::STRING);
        assert_eq!(guide.selection_range, range((0, 0), (0, 5)));
        assert_eq!(guide.range, range((0, 0), (13, 5)));
        let sections: Vec<(&str, Range)> = guide
            .children
            .iter()
            .map(|section| (section.name.as_str(), section.range))
            .collect();
        assert_eq!(
            sections,
            [
                ("Install", range((5, 0), (10, 0))),
                ("Usage", range((10, 0), (13, 5)))
            ]
        );
        assert_contained(&outline);
    }

    #[test]
    fn test_document_outline_nests_objects_under_their_section_and_class() {
        // Given
        let text = "\
API
===

.. py:module:: shapes

.. py:class:: Circle

   .. py:method:: area()

      The area.

.. option:: -v, --verbose
";

        // When
        let outline = outline_of(text);

        // Then — named as the index names them
        assert_eq!(
            tree(&outline),
            [(
                "API".to_string(),
                [
                    "shapes",
                    "shapes.Circle",
                    "shapes.Circle.area",
                    "-v, --verbose"
                ]
                .map(String::from)
                .to_vec()
            )]
        );
        let api = &outline[0];
        let kinds: Vec<(SymbolKind, Option<&str>)> = api
            .children
            .iter()
            .map(|symbol| (symbol.kind, symbol.detail.as_deref()))
            .collect();
        assert_eq!(
            kinds,
            [
                (SymbolKind::MODULE, Some("py:module")),
                (SymbolKind::CLASS, Some("py:class")),
                (SymbolKind::KEY, Some("std:cmdoption")),
            ]
        );
        // A class spans its body, the method in it with its own.
        let circle = &api.children[1];
        assert_eq!(circle.range, range((5, 0), (9, 15)));
        let area = &circle.children[0];
        assert_eq!(area.kind, SymbolKind::METHOD);
        assert_eq!(area.selection_range.start, lsp_types::Position::new(7, 3));
        assert_eq!(area.range.end, lsp_types::Position::new(9, 15));
        assert_contained(&outline);
    }

    #[test]
    fn test_document_outline_lists_an_entity_by_its_id_and_type() {
        // Given
        let text = "Reqs\n====\n\n.. req:: Boot\n   :id: REQ_1\n";
        let entity = EntityBody {
            type_name: "req".to_string(),
            id: EntityId::new("REQ_1").unwrap().into(),
            attributes: BTreeMap::new(),
            relations: BTreeMap::new(),
            sections: vec![],
            span: Some(span_at(4, 13)),
            relation_spans: BTreeMap::new(),
        };
        let mut document = Document::new(
            "reqs.rst".to_string(),
            vec![
                heading(1, "Reqs", Some(span_at(1, 4))),
                Node::Directive(Directive::Entity(Box::new(entity))),
            ],
        );
        // The directive's block, as the parser records it
        document.syntax.blocks.push(rinx_ast::SyntaxBlock {
            kind: BlockKind::Directive,
            lines: Span::new(Position::new(4, 1), Position::new(5, 14)),
        });

        // When
        let outline = document_outline(&document, text, PositionEncoding::Utf16);

        // Then
        let requirement = &outline[0].children[0];
        assert_eq!(requirement.name, "REQ_1");
        assert_eq!(requirement.detail.as_deref(), Some("req"));
        assert_eq!(requirement.kind, SymbolKind::OBJECT);
        assert_eq!(requirement.range, range((3, 0), (4, 13)));
        assert_contained(&outline);
    }

    #[test]
    fn test_document_outline_places_an_included_section_on_its_include_line() {
        // Given — `Part` is a section of `part.rst`, included on line 4
        let text = "Guide\n=====\n\n.. include:: part.rst\n\nAfter\n-----\n";
        let mut document = Document::new("guide.rst".to_string(), Vec::new());
        let part = document.intern_source_file("/work/part.rst");
        document.include_sites.push(IncludeSite {
            directive: Some(span_at(4, 21)),
            file: part,
        });
        document.nodes = vec![
            heading(1, "Guide", Some(span_at(1, 5))),
            heading(2, "Part", Some(span_at(1, 4).with_file(Some(part)))),
            heading(2, "After", Some(span_at(6, 5))),
        ];

        // When
        let outline = document_outline(&document, text, PositionEncoding::Utf16);

        // Then
        let placed: Vec<(&str, Range)> = outline[0]
            .children
            .iter()
            .map(|section| (section.name.as_str(), section.selection_range))
            .collect();
        assert_eq!(
            placed,
            [
                ("Part", range((3, 0), (3, 21))),
                ("After", range((5, 0), (5, 5)))
            ]
        );
        assert_contained(&outline);
    }

    #[test]
    fn test_document_outline_leaves_out_what_no_include_of_the_document_brought_in() {
        // Given — a heading in a file no include site names
        let mut document = Document::new("guide.rst".to_string(), Vec::new());
        let stray = document.intern_source_file("/work/stray.rst");
        document.nodes = vec![heading(
            1,
            "Stray",
            Some(span_at(1, 5).with_file(Some(stray))),
        )];

        // When / Then
        assert!(document_outline(&document, "\n", PositionEncoding::Utf16).is_empty());
    }

    #[test]
    fn test_document_outline_takes_no_heading_in_a_directive_body_for_a_section() {
        // When
        let outline = outline_of(".. note::\n\n   Title\n   =====\n\n   text\n");

        // Then
        assert!(outline.is_empty(), "{outline:?}");
    }

    #[test]
    fn test_document_outline_keeps_what_an_object_without_a_position_holds() {
        // Given — a class the parser gave no span, holding an entity
        let entity = EntityBody {
            type_name: "req".to_string(),
            id: EntityId::new("REQ_1").unwrap().into(),
            attributes: BTreeMap::new(),
            relations: BTreeMap::new(),
            sections: vec![],
            span: Some(span_at(2, 10)),
            relation_spans: BTreeMap::new(),
        };
        let class = DomainObjectBody::PyClass {
            signatures: rinx_ast::NonEmptyVector::new("Circle".to_string(), vec![]),
            is_final: false,
            module: None,
            flags: rinx_ast::DescriptionFlags::default(),
            body: vec![Node::Directive(Directive::Entity(Box::new(entity)))],
        };
        let document = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject {
                object: class,
                span: None,
            })],
        );

        // When
        let outline = document_outline(&document, "x\n.. req:: A\n", PositionEncoding::Utf16);

        // Then
        assert_eq!(tree(&outline), [("REQ_1".to_string(), vec![])]);
    }

    #[test]
    fn test_flatten_lists_each_symbol_naming_its_holder() {
        // Given
        let text = "Guide\n=====\n\nInstall\n-------\n";
        let uri = Uri::from_str("file:///work/guide.rst").unwrap();

        // When
        let flat = flatten(outline_of(text), &uri);

        // Then
        let shown: Vec<(&str, Option<&str>)> = flat
            .iter()
            .map(|symbol| (symbol.name.as_str(), symbol.container_name.as_deref()))
            .collect();
        assert_eq!(shown, [("Guide", None), ("Install", Some("Guide"))]);
        assert_eq!(flat[1].location, Location::new(uri, range((3, 0), (4, 7))));
    }

    #[test]
    fn test_document_symbols_serialize_as_the_protocol_types() {
        // Given
        let outline = outline_of("Guide\n=====\n\nInstall\n-------\n");
        let uri = Uri::from_str("file:///work/guide.rst").unwrap();

        // When
        let nested = serde_json::to_value(DocumentSymbols::Nested(outline.clone())).unwrap();
        let flat = serde_json::to_value(DocumentSymbols::Flat(flatten(outline, &uri))).unwrap();

        // Then
        let nested: Vec<lsp_types::DocumentSymbol> = serde_json::from_value(nested).unwrap();
        assert_eq!(nested[0].name, "Guide");
        assert_eq!(nested[0].children.as_ref().map(Vec::len), Some(1));
        let flat: Vec<lsp_types::SymbolInformation> = serde_json::from_value(flat).unwrap();
        assert_eq!(flat[1].container_name.as_deref(), Some("Guide"));
    }

    #[test]
    fn test_document_symbols_take_the_shape_asked_for() {
        // Given
        let text = "Guide\n=====\n\nInstall\n-------\n";
        let document = rinx_parser::parse("guide.rst", text);
        let uri = Uri::from_str("file:///work/guide.rst").unwrap();
        let symbols =
            |shape| document_symbols(&document, text, &uri, shape, PositionEncoding::Utf16);

        // When / Then
        assert_eq!(
            symbols(OutlineShape::Nested),
            DocumentSymbols::Nested(document_outline(&document, text, PositionEncoding::Utf16))
        );
        assert_eq!(
            symbols(OutlineShape::Flat),
            DocumentSymbols::Flat(flatten(
                document_outline(&document, text, PositionEncoding::Utf16),
                &uri
            ))
        );
    }

    #[test]
    fn test_non_empty_falls_back_for_a_blank_name() {
        // When / Then
        assert_eq!(non_empty("  ".to_string(), "function"), "function");
        assert_eq!(non_empty("area".to_string(), "function"), "area");
    }

    #[test]
    fn test_symbol_kind_marks_each_object_type() {
        // When / Then
        assert_eq!(
            symbol_kind(ObjectType::C(CObjectType::Function)),
            SymbolKind::FUNCTION
        );
        assert_eq!(
            symbol_kind(ObjectType::Py(PyObjectType::Exception)),
            SymbolKind::CLASS
        );
        assert_eq!(
            symbol_kind(ObjectType::C(CObjectType::Macro)),
            SymbolKind::CONSTANT
        );
        assert_eq!(
            symbol_kind(ObjectType::Py(PyObjectType::Data)),
            SymbolKind::VARIABLE
        );
    }
}
