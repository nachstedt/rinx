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

fn sites_of(nodes: Vec<Node>) -> DefinitionSites {
    analyze_with_sites(&Document::new("guide.rst".to_string(), nodes)).1
}

fn heading(level: u8, title: &str, span: Option<Span>) -> Node {
    Node::Heading {
        level,
        text: vec![InlineNode::Text(title.to_string())],
        span,
    }
}

fn label(name: &str, span: Option<Span>) -> Node {
    Node::Target {
        name: TargetName::new(name).into(),
        destination: None,
        span,
    }
}

#[test]
fn test_analyze_with_sites_records_every_section_by_its_anchor() {
    // When
    let sites = sites_of(vec![
        heading(1, "Guide", Some(span_at(1, 5))),
        heading(2, "Getting Started", Some(span_at(4, 15))),
    ]);

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
    // When
    let sites = sites_of(vec![
        label("install", Some(span_at(3, 13))),
        Node::Target {
            name: TargetName::new("home").into(),
            destination: Some(LinkDestination::Uri("https://example.org".to_string())),
            span: Some(span_at(5, 30)),
        },
    ]);

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
    // When
    let sites = sites_of(vec![Node::Directive(Directive::Admonition {
        kind: AdmonitionKind::Note,
        title: None,
        collapsible: None,
        body: vec![label("inside", Some(span_at(3, 14)))],
    })]);

    // Then
    assert_eq!(
        sites.get(&DefinitionKey::Label(TargetName::new("inside"))),
        Some(&site_at(3, 14))
    );
}

#[test]
fn test_analyze_with_sites_records_each_glossary_term() {
    // When
    let sites = sites_of(vec![Node::Directive(Directive::Glossary {
        entries: vec![GlossaryEntry {
            terms: vec![
                GlossaryTerm {
                    text: "API".to_string(),
                    span: Some(span_at(3, 6)),
                },
                GlossaryTerm {
                    text: "Interface".to_string(),
                    span: Some(span_at(4, 12)),
                },
            ],
            definition: vec![],
        }],
        sorted: false,
    })]);

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
    let sites = sites_of(vec![
        Node::Directive(Directive::DomainObject {
            object: module,
            span: Some(span_at(1, 20)),
        }),
        Node::Directive(Directive::DomainObject {
            object: function,
            span: Some(span_at(3, 25)),
        }),
    ]);

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
    let function = DomainObjectBody::PyFunction {
        signatures: NonEmptyVector::new("area()".to_string(), vec![]),
        is_decorator: false,
        is_async: false,
        module: None,
        flags: DescriptionFlags::of([rinx_ast::DescriptionFlag::NoIndex]),
        body: vec![],
    };

    // When
    let sites = sites_of(vec![Node::Directive(Directive::DomainObject {
        object: function,
        span: Some(span_at(1, 25)),
    })]);

    // Then
    assert!(sites.is_empty(), "{sites:?}");
}

#[test]
fn test_analyze_with_sites_records_a_tables_name_as_a_label() {
    // When
    let sites = sites_of(vec![Node::Directive(Directive::DataTable {
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
        span: Some(span_at(2, 18)),
    })]);

    // Then
    assert_eq!(
        sites.get(&DefinitionKey::Label(TargetName::new("sizes"))),
        Some(&site_at(2, 18))
    );
}

#[test]
fn test_analyze_with_sites_records_an_entity_by_its_id_and_as_a_label() {
    // Given
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
    let sites = sites_of(vec![Node::Directive(Directive::Entity(Box::new(entity)))]);

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
    // When
    let sites = sites_of(vec![Node::Directive(Directive::Math {
        parts: vec!["e = mc^2".to_string()],
        label: Some(TargetName::new("energy").into()),
        nowrap: false,
        classes: vec![],
        span: Some(span_at(4, 9)),
    })]);

    // Then
    assert_eq!(
        sites.get(&DefinitionKey::Equation(TargetName::new("energy"))),
        Some(&site_at(4, 9))
    );
}

#[test]
fn test_analyze_with_sites_names_the_fragment_a_definition_was_included_from() {
    // Given — a label whose span counts in an included fragment
    let mut doc = Document::new("guide.rst".to_string(), vec![]);
    let fragment = doc.intern_source_file("shared/install.rst");
    doc.nodes.push(label(
        "install",
        Some(Span::new(Position::new(2, 1), Position::new(2, 13)).with_file(Some(fragment))),
    ));

    // When
    let (_, sites) = analyze_with_sites(&doc);

    // Then
    assert_eq!(
        sites.get(&DefinitionKey::Label(TargetName::new("install"))),
        Some(&DefinitionSite {
            file: Some("shared/install.rst".to_string()),
            ..site_at(2, 12)
        })
    );
}

/// `name` written at columns `start` to `end` of `line`.
fn written_at(name: &str, line: u32, start: u32, end: u32) -> Written<TargetName> {
    Written::new(
        TargetName::new(name),
        Some(Span::new(
            Position::new(line, start),
            Position::new(line, end),
        )),
    )
}

#[test]
fn test_analyze_with_sites_records_where_a_labels_name_was_written() {
    // Given — `.. _install:`, the name in columns 5 to 11
    let target = Node::Target {
        name: written_at("install", 3, 5, 12),
        destination: None,
        span: Some(span_at(3, 12)),
    };

    // When
    let sites = sites_of(vec![target]);

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
    let mut toctree = rinx_ast::Toctree::default();
    toctree.options.name = Some(written_at("tree", 2, 11, 15));
    let mut contents = rinx_ast::Contents::default();
    contents.options.name = Some(written_at("toc", 6, 11, 14));

    // When
    let sites = sites_of(vec![
        Node::Directive(Directive::Toctree(toctree)),
        Node::Directive(Directive::Contents(contents)),
    ]);

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
    let entity = EntityBody {
        type_name: "req".to_string(),
        id: Written::new(
            EntityId::new("REQ_001").unwrap(),
            Some(Span::new(Position::new(8, 9), Position::new(8, 16))),
        ),
        attributes: BTreeMap::new(),
        relations: BTreeMap::new(),
        sections: vec![],
        span: Some(span_at(7, 20)),
        relation_spans: BTreeMap::new(),
    };

    // When
    let sites = sites_of(vec![Node::Directive(Directive::Entity(Box::new(entity)))]);

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
    // When
    let sites = sites_of(vec![label("install", None)]);

    // Then
    assert!(sites.is_empty());
}

#[test]
fn test_analyze_with_sites_returns_the_index_analyze_returns() {
    // Given
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![
            heading(1, "Guide", Some(span_at(1, 5))),
            label("install", Some(span_at(3, 13))),
        ],
    );

    // When
    let (index, _) = analyze_with_sites(&doc);

    // Then
    assert_eq!(index, analyze(&doc));
}
