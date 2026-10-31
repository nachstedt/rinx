//! Tests for the definition sites [`super::analyze_with_sites`] records
//! beside the index: one per definition the index names, at the span the
//! parser gave it, and never one for what the index leaves out.

use std::collections::BTreeMap;

use super::*;
use rinx_ast::{
    AdmonitionKind, AttributeValue, DescriptionFlags, DomainObjectBody, EntityBody, EntityId,
    GlossaryEntry, GlossaryTerm, InlineNode, LinkDestination, ModuleOptions, NonEmptyVector,
    ObjectType, Position, PyObjectType, SectionId, Span, TableSource,
};
use rinx_index::{DefinitionKey, DefinitionSite, DefinitionSites, NameRange};

/// A span on `line` of the document itself, `columns` characters wide.
fn span_at(line: u32, columns: u32) -> Span {
    Span::new(Position::new(line, 1), Position::new(line, columns + 1))
}

/// The site [`line`] records.
fn site_at(line: u32, columns: u32) -> DefinitionSite {
    DefinitionSite {
        file: None,
        start: Position::new(line, 1),
        end: Position::new(line, columns + 1),
        name: None,
    }
}

/// The sites of `nodes`, finding their names' characters and blocks' lines
/// in `source_map`.
fn sites_of(nodes: Vec<Node>, source_map: &SourceMap) -> DefinitionSites {
    analyze_with_sites(
        &Document::new("guide.rst".to_string(), nodes),
        Some(source_map),
    )
    .1
}

/// A heading titled `title`, its line `span` recorded in `map`.
fn heading(map: &mut SourceMap, level: u8, title: &str, span: Option<Span>) -> Node {
    Node::Heading {
        level,
        text: vec![InlineNode::Text(title.to_string())],
        block: span.map(|span| map.record_block_lines(span)),
    }
}

/// The internal target `name`, its line `span` recorded in `map`.
fn label(map: &mut SourceMap, name: &str, span: Option<Span>) -> Node {
    Node::Target {
        name: TargetName::new(name).into(),
        destination: None,
        block: span.map(|span| map.record_block_lines(span)),
    }
}

#[test]
fn test_analyze_with_sites_records_every_section_by_its_anchor() {
    // Given
    let mut map = SourceMap::default();

    // When
    let sites = sites_of(
        vec![
            heading(&mut map, 1, "Guide", Some(span_at(1, 5))),
            heading(&mut map, 2, "Getting Started", Some(span_at(4, 15))),
        ],
        &map,
    );

    // Then
    assert_eq!(
        sites.get(&DefinitionKey::Section(SectionId::from_title("Guide"))),
        Some(&site_at(1, 5))
    );
    assert_eq!(
        sites.get(&DefinitionKey::Section(SectionId::from_title(
            "Getting Started"
        ))),
        Some(&site_at(4, 15))
    );
}

#[test]
fn test_analyze_with_sites_records_an_internal_target_but_not_an_external_one() {
    // Given
    let mut map = SourceMap::default();

    // When
    let sites = sites_of(
        vec![
            label(&mut map, "install", Some(span_at(3, 13))),
            Node::Target {
                name: TargetName::new("home").into(),
                destination: Some(LinkDestination::Uri("https://example.org".to_string())),
                block: Some(map.record_block_lines(span_at(5, 30))),
            },
        ],
        &map,
    );

    // Then — the external target names a URL, which the index leaves out
    assert_eq!(
        sites.get(&DefinitionKey::Label(TargetName::new("install"))),
        Some(&site_at(3, 13))
    );
    assert_eq!(
        sites.get(&DefinitionKey::Label(TargetName::new("home"))),
        None
    );
}

#[test]
fn test_analyze_with_sites_records_a_target_nested_in_a_directive_body() {
    // Given
    let mut map = SourceMap::default();

    // When
    let sites = sites_of(
        vec![Node::directive(Directive::Admonition {
            kind: AdmonitionKind::Note,
            title: None,
            collapsible: None,
            body: vec![label(&mut map, "inside", Some(span_at(3, 14)))],
        })],
        &map,
    );

    // Then
    assert_eq!(
        sites.get(&DefinitionKey::Label(TargetName::new("inside"))),
        Some(&site_at(3, 14))
    );
}

#[test]
fn test_analyze_with_sites_records_each_glossary_term() {
    // Given
    let mut map = SourceMap::default();

    // When
    let sites = sites_of(
        vec![Node::directive(Directive::Glossary {
            entries: vec![GlossaryEntry {
                terms: vec![
                    GlossaryTerm {
                        text: "API".to_string(),
                        block: Some(map.record_block_lines(span_at(3, 6))),
                    },
                    GlossaryTerm {
                        text: "Interface".to_string(),
                        block: Some(map.record_block_lines(span_at(4, 12))),
                    },
                ],
                definition: vec![],
            }],
            sorted: false,
        })],
        &map,
    );

    // Then — by the normalized name the index keys a term by
    assert_eq!(
        sites.get(&DefinitionKey::Term(TargetName::new("api"))),
        Some(&site_at(3, 6))
    );
    assert_eq!(
        sites.get(&DefinitionKey::Term(TargetName::new("interface"))),
        Some(&site_at(4, 12))
    );
}

#[test]
fn test_analyze_with_sites_records_every_signature_of_a_domain_object_qualified_by_its_scope() {
    // Given — a module, then a function with two signatures inside it
    let mut map = SourceMap::default();
    let module = DomainObjectBody::PyModule {
        name: "shapes".to_string(),
        options: ModuleOptions::default(),
        body: vec![],
    };
    let function = DomainObjectBody::PyFunction {
        signatures: NonEmptyVector::new("area()".to_string(), vec!["size()".to_string()]),
        is_decorator: false,
        is_async: false,
        module: None,
        flags: DescriptionFlags::default(),
        body: vec![],
    };

    // When
    let sites = sites_of(
        vec![
            Node::Directive {
                directive: Directive::DomainObject { object: module },
                block: Some(map.record_block_lines(span_at(1, 20))),
            },
            Node::Directive {
                directive: Directive::DomainObject { object: function },
                block: Some(map.record_block_lines(span_at(3, 25))),
            },
        ],
        &map,
    );

    // Then
    let function_type = ObjectType::Py(PyObjectType::Function);
    for name in ["shapes.area", "shapes.size"] {
        assert_eq!(
            sites.get(&DefinitionKey::Object(function_type, TargetName::new(name))),
            Some(&site_at(3, 25)),
            "{name}"
        );
    }
    assert_eq!(
        sites.get(&DefinitionKey::Object(
            ObjectType::Py(PyObjectType::Module),
            TargetName::new("shapes")
        )),
        Some(&site_at(1, 20))
    );
}

#[test]
fn test_analyze_with_sites_records_no_domain_object_that_asked_not_to_be_indexed() {
    // Given
    let mut map = SourceMap::default();
    let function = DomainObjectBody::PyFunction {
        signatures: NonEmptyVector::new("area()".to_string(), vec![]),
        is_decorator: false,
        is_async: false,
        module: None,
        flags: DescriptionFlags::of([rinx_ast::DescriptionFlag::NoIndex]),
        body: vec![],
    };

    // When
    let sites = sites_of(
        vec![Node::Directive {
            directive: Directive::DomainObject { object: function },
            block: Some(map.record_block_lines(span_at(1, 25))),
        }],
        &map,
    );

    // Then
    assert!(sites.is_empty(), "{sites:?}");
}

#[test]
fn test_analyze_with_sites_records_a_tables_name_as_a_label() {
    // Given
    let mut map = SourceMap::default();

    // When
    let sites = sites_of(
        vec![Node::Directive {
            directive: Directive::DataTable {
                source: TableSource::List,
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: Some(TargetName::new("sizes").into()),
                rows: vec![],
            },
            block: Some(map.record_block_lines(span_at(2, 18))),
        }],
        &map,
    );

    // Then
    assert_eq!(
        sites.get(&DefinitionKey::Label(TargetName::new("sizes"))),
        Some(&site_at(2, 18))
    );
}

#[test]
fn test_analyze_with_sites_records_an_entity_by_its_id_and_as_a_label() {
    // Given
    let map = SourceMap::default();
    let entity = EntityBody {
        type_name: "req".to_string(),
        id: EntityId::new("REQ_001").unwrap().into(),
        attributes: BTreeMap::from([(
            "title".to_string(),
            AttributeValue::String("Boot quickly".to_string()),
        )]),
        relations: BTreeMap::new(),
        sections: vec![],
        span: Some(span_at(7, 20)),
        relation_spans: BTreeMap::new(),
    };

    // When
    let sites = sites_of(
        vec![Node::directive(Directive::Entity(Box::new(entity)))],
        &map,
    );

    // Then
    let id = EntityId::new("REQ_001").unwrap();
    assert_eq!(sites.get(&DefinitionKey::Entity(id)), Some(&site_at(7, 20)));
    assert_eq!(
        sites.get(&DefinitionKey::Label(TargetName::new("REQ_001"))),
        Some(&site_at(7, 20))
    );
}

#[test]
fn test_analyze_with_sites_records_an_equations_label() {
    // Given
    let map = SourceMap::default();

    // When
    let sites = sites_of(
        vec![Node::directive(Directive::Math {
            parts: vec!["e = mc^2".to_string()],
            label: Some(TargetName::new("energy").into()),
            nowrap: false,
            classes: vec![],
            span: Some(span_at(4, 9)),
        })],
        &map,
    );

    // Then
    assert_eq!(
        sites.get(&DefinitionKey::Equation(TargetName::new("energy"))),
        Some(&site_at(4, 9))
    );
}

#[test]
fn test_analyze_with_sites_names_the_fragment_a_definition_was_included_from() {
    // Given — a label whose span counts in an included fragment
    let mut map = SourceMap::default();
    let mut doc = Document::new("guide.rst".to_string(), vec![]);
    let fragment = doc.intern_source_file("shared/install.rst");
    doc.nodes.push(label(
        &mut map,
        "install",
        Some(Span::new(Position::new(2, 1), Position::new(2, 13)).with_file(Some(fragment))),
    ));

    // When
    let (_, sites) = analyze_with_sites(&doc, Some(&map));

    // Then
    assert_eq!(
        sites.get(&DefinitionKey::Label(TargetName::new("install"))),
        Some(&DefinitionSite {
            file: Some("shared/install.rst".to_string()),
            ..site_at(2, 12)
        })
    );
}

/// `name` written at columns `start` to `end` of `line`, as `source_map`
/// records it.
fn written_at(
    source_map: &mut SourceMap,
    name: &str,
    line: u32,
    start: u32,
    end: u32,
) -> Named<TargetName> {
    let id = source_map.record_name(Span::new(
        Position::new(line, start),
        Position::new(line, end),
    ));
    Named::new(TargetName::new(name), Some(id))
}

#[test]
fn test_analyze_with_sites_records_where_a_labels_name_was_written() {
    // Given — `.. _install:`, the name in columns 5 to 11
    let mut map = SourceMap::default();
    let target = Node::Target {
        name: written_at(&mut map, "install", 3, 5, 12),
        destination: None,
        block: Some(map.record_block_lines(span_at(3, 12))),
    };

    // When
    let sites = sites_of(vec![target], &map);

    // Then
    assert_eq!(
        sites.get(&DefinitionKey::Label(TargetName::new("install"))),
        Some(&DefinitionSite {
            name: Some(NameRange {
                start: Position::new(3, 5),
                end: Position::new(3, 12),
            }),
            ..site_at(3, 12)
        })
    );
}

#[test]
fn test_analyze_with_sites_places_a_name_no_directive_span_holds_at_the_name() {
    // Given — a toctree's and a table of contents' `:name:`, the parser
    // giving neither directive a span of its own
    let mut map = SourceMap::default();
    let mut toctree = rinx_ast::Toctree::default();
    toctree.options.name = Some(written_at(&mut map, "tree", 2, 11, 15));
    let mut contents = rinx_ast::Contents::default();
    contents.options.name = Some(written_at(&mut map, "toc", 6, 11, 14));

    // When
    let sites = sites_of(
        vec![
            Node::directive(Directive::Toctree(toctree)),
            Node::directive(Directive::Contents(contents)),
        ],
        &map,
    );

    // Then
    let at_name = |line, start, end| DefinitionSite {
        file: None,
        start: Position::new(line, start),
        end: Position::new(line, end),
        name: Some(NameRange {
            start: Position::new(line, start),
            end: Position::new(line, end),
        }),
    };
    assert_eq!(
        sites.get(&DefinitionKey::Label(TargetName::new("tree"))),
        Some(&at_name(2, 11, 15))
    );
    assert_eq!(
        sites.get(&DefinitionKey::Label(TargetName::new("toc"))),
        Some(&at_name(6, 11, 14))
    );
}

#[test]
fn test_analyze_with_sites_records_where_a_written_entity_id_stands() {
    // Given — `:id: REQ_001` on the entity's second line
    let mut map = SourceMap::default();
    let written = map.record_name(Span::new(Position::new(8, 9), Position::new(8, 16)));
    let entity = EntityBody {
        type_name: "req".to_string(),
        id: Named::new(EntityId::new("REQ_001").unwrap(), Some(written)),
        attributes: BTreeMap::new(),
        relations: BTreeMap::new(),
        sections: vec![],
        span: Some(span_at(7, 20)),
        relation_spans: BTreeMap::new(),
    };

    // When
    let sites = sites_of(
        vec![Node::directive(Directive::Entity(Box::new(entity)))],
        &map,
    );

    // Then — as the entity and as its label
    let name = Some(NameRange {
        start: Position::new(8, 9),
        end: Position::new(8, 16),
    });
    let id = EntityId::new("REQ_001").unwrap();
    assert_eq!(
        sites
            .get(&DefinitionKey::Entity(id))
            .and_then(|site| site.name),
        name
    );
    assert_eq!(
        sites
            .get(&DefinitionKey::Label(TargetName::new("REQ_001")))
            .and_then(|site| site.name),
        name
    );
}

#[test]
fn test_analyze_with_sites_records_nothing_for_a_definition_without_a_span() {
    // Given
    let mut map = SourceMap::default();

    // When
    let sites = sites_of(vec![label(&mut map, "install", None)], &map);

    // Then
    assert!(sites.is_empty());
}

#[test]
fn test_analyze_with_sites_returns_the_index_analyze_returns() {
    // Given
    let mut map = SourceMap::default();
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![
            heading(&mut map, 1, "Guide", Some(span_at(1, 5))),
            label(&mut map, "install", Some(span_at(3, 13))),
        ],
    );

    // When
    let (index, _) = analyze_with_sites(&doc, None);

    // Then
    assert_eq!(index, analyze(&doc));
}
