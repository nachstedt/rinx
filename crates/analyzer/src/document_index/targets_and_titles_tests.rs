//! Target-registration, document-title and glossary-term tests for
//! [`super::analyze`] — the parts of a document's index that need no
//! domain or scope handling.

use super::*;
use rinx_ast::{InlineNode, ObjectType, PyObjectType, TableSource, TargetSearchOrder};

#[test]
fn test_analyze_returns_default_index_for_empty_document() {
    // Given
    let doc = Document::new("test.rst".to_string(), vec![]);

    // When
    let index = analyze(&doc);

    // Then
    let _ = format!("{index:?}"); // Ensures it doesn't panic
}

#[test]
fn test_analyze_returns_default_index_for_populated_document() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("Title".to_string())],
        }],
    );

    // When
    let index = analyze(&doc);

    // Then
    // Currently analyze does not populate anything, but it shouldn't panic
    let _ = format!("{index:?}");
}

#[test]
fn test_analyze_populates_targets_for_target_nodes() {
    // Given
    let doc = Document::new(
        "docs/my-file.rst".to_string(),
        vec![
            Node::Target {
                name: TargetName::new("section-1"),
                uri: None,
            },
            Node::Paragraph(vec![InlineNode::Text("some text".to_string())]),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.targets.len(), 1);
    assert_eq!(
        index.targets.get(&TargetName::new("section-1")).unwrap(),
        &TargetLocation::Internal("docs/my-file.rst".to_string())
    );
}

#[test]
fn test_analyze_extracts_h1_title() {
    // Given
    let doc = Document::new(
        "docs/my-file.rst".to_string(),
        vec![
            Node::Paragraph(vec![InlineNode::Text("some text".to_string())]),
            Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("My Title".to_string())],
            },
            Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Ignored Second H1".to_string())],
            },
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.document_titles.len(), 1);
    assert_eq!(
        index.document_titles.get("docs/my-file.rst").unwrap(),
        "My Title"
    );
}

#[test]
fn test_analyze_records_a_document_without_a_title() {
    // Given — a page with no heading at all
    let doc = Document::new(
        "notes/scratch.rst".to_string(),
        vec![Node::Paragraph(vec![InlineNode::Text(
            "jottings".to_string(),
        )])],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.document_titles.is_empty());
    assert!(index.documents.contains("notes/scratch.rst"));
}

#[test]
fn test_analyze_extracts_h1_title_as_plain_text_when_heading_has_domain_object_reference() {
    // Given — a heading containing a `~`-shortened domain-object reference
    let doc = Document::new(
        "docs/greetings.rst".to_string(),
        vec![Node::Heading {
            level: 1,
            text: vec![
                InlineNode::Text("The ".to_string()),
                InlineNode::DomainObjectReference {
                    object_type: ObjectType::Py(PyObjectType::Module),
                    name: "pkg.greetings".to_string(),
                    display: "greetings".to_string(),
                    link: true,
                    search_order: TargetSearchOrder::LeastQualifiedFirst,
                    span: None,
                    inventory: rinx_ast::InventorySelector::Any,
                },
                InlineNode::Text(" Module".to_string()),
            ],
        }],
    );

    // When
    let index = analyze(&doc);

    // Then — the title is flattened to plain text, using the shortened display
    assert_eq!(
        index.document_titles.get("docs/greetings.rst").unwrap(),
        "The greetings Module"
    );
}

#[test]
fn test_analyze_registers_glossary_terms() {
    // Given
    let doc = Document::new(
        "glossary.rst".to_string(),
        vec![Node::Directive(Directive::Glossary {
            entries: vec![rinx_ast::GlossaryEntry {
                terms: vec!["environment".to_string()],
                definition: vec![],
            }],
            sorted: false,
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.glossary_terms.len(), 1);
    assert_eq!(
        index.glossary_terms.get(&TargetName::new("environment")),
        Some(&"glossary.rst".to_string())
    );
}

#[test]
fn test_analyze_registers_all_terms_in_multi_term_entry() {
    // Given
    let doc = Document::new(
        "glossary.rst".to_string(),
        vec![Node::Directive(Directive::Glossary {
            entries: vec![rinx_ast::GlossaryEntry {
                terms: vec!["term 1".to_string(), "term 2".to_string()],
                definition: vec![],
            }],
            sorted: false,
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.glossary_terms.len(), 2);
    assert!(
        index
            .glossary_terms
            .contains_key(&TargetName::new("term 1"))
    );
    assert!(
        index
            .glossary_terms
            .contains_key(&TargetName::new("term 2"))
    );
}

#[test]
fn test_analyze_registers_target_nested_in_table_cell() {
    // Given — a target nested inside a grid-table cell
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Table {
            header_rows: vec![],
            body_rows: vec![rinx_ast::TableRow {
                cells: vec![rinx_ast::TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: vec![Node::Target {
                        name: TargetName::new("nested-target"),
                        uri: None,
                    }],
                }],
            }],
        }],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.targets.get(&TargetName::new("nested-target")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

#[test]
fn test_analyze_registers_list_table_name_as_target() {
    // Given — a `.. list-table::` with a `:name:` option
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DataTable {
            source: TableSource::List,
            title: None,
            header_rows: 0,
            stub_columns: 0,
            widths: None,
            width: None,
            align: None,
            classes: vec![],
            name: Some(TargetName::new("fruit-table")),
            rows: vec![],
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.targets.get(&TargetName::new("fruit-table")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

#[test]
fn test_analyze_list_table_without_name_registers_no_target() {
    // Given — a `.. list-table::` with no `:name:` option
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DataTable {
            source: TableSource::List,
            title: None,
            header_rows: 0,
            stub_columns: 0,
            widths: None,
            width: None,
            align: None,
            classes: vec![],
            name: None,
            rows: vec![],
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.targets.is_empty());
}

#[test]
fn test_analyze_registers_table_directive_name_as_target() {
    // Given — a `.. table::` with a `:name:` option
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::Table {
            title: None,
            widths: None,
            width: None,
            align: None,
            classes: vec![],
            name: Some(TargetName::new("wrapped-table")),
            header_rows: vec![],
            body_rows: vec![],
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.targets.get(&TargetName::new("wrapped-table")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

#[test]
fn test_analyze_descends_into_table_directive_header_and_body_cells() {
    // Given — a nested target in both a header cell and a body cell
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::Table {
            title: None,
            widths: None,
            width: None,
            align: None,
            classes: vec![],
            name: None,
            header_rows: vec![rinx_ast::TableRow {
                cells: vec![rinx_ast::TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: vec![Node::Target {
                        name: TargetName::new("in-header"),
                        uri: None,
                    }],
                }],
            }],
            body_rows: vec![rinx_ast::TableRow {
                cells: vec![rinx_ast::TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: vec![Node::Target {
                        name: TargetName::new("in-body"),
                        uri: None,
                    }],
                }],
            }],
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.targets.get(&TargetName::new("in-header")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
    assert_eq!(
        index.targets.get(&TargetName::new("in-body")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

/// A code block carrying `name`, with every other option left at its default.
fn code_block_named(name: Option<&str>) -> Node {
    Node::Directive(Directive::CodeBlock(rinx_ast::CodeBlock {
        source: rinx_ast::CodeBlockSource::CodeBlock,
        language: rinx_ast::CodeLanguage::parse("python"),
        content: "x = 1".to_string(),
        caption: None,
        name: name.map(TargetName::new),
        classes: vec![],
        linenos: false,
        lineno_start: None,
        emphasize_lines: vec![],
        force: false,
        span: None,
    }))
}

#[test]
fn test_analyze_registers_code_block_name_as_target() {
    // Given — a `.. code-block::` with a `:name:` option
    let doc = Document::new(
        "test.rst".to_string(),
        vec![code_block_named(Some("my-code"))],
    );

    // When
    let index = analyze(&doc);

    // Then — a `:ref:` can reach it, exactly as it can reach a named table
    assert_eq!(
        index.targets.get(&TargetName::new("my-code")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

#[test]
fn test_analyze_code_block_without_name_registers_no_target() {
    // Given — a `.. code-block::` with no `:name:` option
    let doc = Document::new("test.rst".to_string(), vec![code_block_named(None)]);

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.targets.is_empty());
}

/// An `.. image::` carrying `name`, with every other option left at its
/// default.
fn image_named(name: Option<&str>) -> Node {
    let mut options = rinx_ast::ImageOptions::new(rinx_ast::AssetUri::new("logo.png"));
    options.name = name.map(TargetName::new);
    Node::Directive(Directive::Image(Box::new(options)))
}

/// A `.. figure::` carrying `name` on its image and `legend` as its body.
fn figure_named(name: Option<&str>, legend: Vec<Node>) -> Node {
    let mut options = rinx_ast::ImageOptions::new(rinx_ast::AssetUri::new("logo.png"));
    options.name = name.map(TargetName::new);
    let mut figure = rinx_ast::Figure::new(options);
    figure.legend = legend;
    Node::Directive(Directive::Figure(Box::new(figure)))
}

#[test]
fn test_analyze_registers_image_name_as_target() {
    // Given — an `.. image::` with a `:name:` option
    let doc = Document::new("test.rst".to_string(), vec![image_named(Some("the-logo"))]);

    // When
    let index = analyze(&doc);

    // Then — a `:ref:` can reach it, exactly as it can reach a named table
    assert_eq!(
        index.targets.get(&TargetName::new("the-logo")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

#[test]
fn test_analyze_image_without_name_registers_no_target() {
    // Given
    let doc = Document::new("test.rst".to_string(), vec![image_named(None)]);

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.targets.is_empty());
}

#[test]
fn test_analyze_registers_figure_name_as_target() {
    // Given — a `.. figure::` with a `:name:` option
    let doc = Document::new(
        "test.rst".to_string(),
        vec![figure_named(Some("the-figure"), vec![])],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.targets.get(&TargetName::new("the-figure")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

#[test]
fn test_analyze_indexes_targets_inside_a_figure_legend() {
    // Given — a legend is ordinary body content, so a target written in one
    // must be reachable like any other
    let doc = Document::new(
        "test.rst".to_string(),
        vec![figure_named(
            None,
            vec![Node::Target {
                name: TargetName::new("in-legend"),
                uri: None,
            }],
        )],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.targets.get(&TargetName::new("in-legend")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

/// A `.. dropdown::` carrying `name` and `body`.
fn dropdown_named(name: Option<&str>, body: Vec<Node>) -> Node {
    Node::Directive(Directive::Dropdown(Box::new(rinx_ast::Dropdown {
        name: name.map(TargetName::new),
        body,
        ..rinx_ast::Dropdown::new()
    })))
}

/// An `.. entity-table::` carrying `name`.
fn entity_table_named(name: Option<&str>) -> Node {
    let mut table = rinx_ast::EntityTable::new(rinx_ast::EntityTableSource::EntityTable);
    table.name = name.map(TargetName::new);
    Node::Directive(Directive::EntityTable(Box::new(table)))
}

/// A diagram directive carrying `name`.
fn diagram_named(name: Option<&str>) -> Node {
    let mut uml = rinx_ast::Uml::new(rinx_ast::UmlSource::PlantUml, "A -> B".to_string());
    uml.name = name.map(TargetName::new);
    Node::Directive(Directive::Uml(Box::new(uml)))
}

#[test]
fn test_analyze_registers_a_diagram_name_as_target() {
    // Given — a diagram with a `:name:` option, which the renderer turns into
    // the `id` a `:ref:` has to land on
    let doc = Document::new(
        "test.rst".to_string(),
        vec![diagram_named(Some("retry-flow"))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.targets.get(&TargetName::new("retry-flow")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

#[test]
fn test_analyze_registers_nothing_for_an_unnamed_diagram() {
    // Given — by far the common case
    let doc = Document::new("test.rst".to_string(), vec![diagram_named(None)]);

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.targets.is_empty());
}

#[test]
fn test_analyze_registers_a_diagram_name_nested_in_an_admonition() {
    // Given — the walk reaches a diagram wherever it is written, the same way
    // diagram extraction does
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::Admonition {
            kind: rinx_ast::AdmonitionKind::Note,
            title: None,
            collapsible: None,
            body: vec![diagram_named(Some("nested-flow"))],
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.targets.get(&TargetName::new("nested-flow")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

#[test]
fn test_analyze_registers_entity_table_name_as_target() {
    // Given — an `.. entity-table::` with a `:name:` option
    let doc = Document::new(
        "test.rst".to_string(),
        vec![entity_table_named(Some("every-requirement"))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.targets.get(&TargetName::new("every-requirement")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

#[test]
fn test_analyze_registers_nothing_for_an_unnamed_entity_table() {
    // Given — the rows come from the index being built, so an unnamed table
    // contributes nothing at all to it
    let doc = Document::new("test.rst".to_string(), vec![entity_table_named(None)]);

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.targets.is_empty());
}

#[test]
fn test_analyze_registers_dropdown_name_as_target() {
    // Given — a `.. dropdown::` with a `:name:` option
    let doc = Document::new(
        "test.rst".to_string(),
        vec![dropdown_named(Some("the-dropdown"), vec![])],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.targets.get(&TargetName::new("the-dropdown")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

#[test]
fn test_analyze_indexes_targets_inside_a_dropdown_body() {
    // Given — the reason this directive is supported at all: an unparsed
    // container hides everything written inside it from the index
    let doc = Document::new(
        "test.rst".to_string(),
        vec![dropdown_named(
            None,
            vec![Node::Target {
                name: TargetName::new("in-dropdown"),
                uri: None,
            }],
        )],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.targets.get(&TargetName::new("in-dropdown")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

fn contents_named(name: Option<&str>) -> Node {
    let options = rinx_ast::ContentsOptions {
        name: name.map(TargetName::new),
        ..rinx_ast::ContentsOptions::default()
    };
    Node::Directive(Directive::Contents(rinx_ast::Contents {
        title: None,
        options,
    }))
}

#[test]
fn test_analyze_registers_contents_name_as_target() {
    // Given — a `.. contents::` with a `:name:` option
    let doc = Document::new(
        "test.rst".to_string(),
        vec![contents_named(Some("main-toc"))],
    );

    // When
    let index = analyze(&doc);

    // Then — a `:ref:` can reach it, exactly as it can reach a named toctree
    assert_eq!(
        index.targets.get(&TargetName::new("main-toc")),
        Some(&TargetLocation::Internal("test.rst".to_string()))
    );
}

#[test]
fn test_analyze_contents_without_name_registers_no_target() {
    // Given
    let doc = Document::new("test.rst".to_string(), vec![contents_named(None)]);

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.targets.is_empty());
}

#[test]
fn test_analyze_records_a_sectnum_directives_options() {
    // Given
    let options = rinx_ast::SectnumOptions {
        prefix: "Appendix ".to_string(),
        ..rinx_ast::SectnumOptions::default()
    };
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::Sectnum(options.clone()))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.sectnum.get("test.rst"), Some(&options));
}

#[test]
fn test_analyze_finds_a_sectnum_directive_nested_in_an_admonition_body() {
    // Given — docutils treats `.. sectnum::` as document-wide regardless of
    // where it's written.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::Admonition {
            kind: rinx_ast::AdmonitionKind::Note,
            title: None,
            collapsible: None,
            body: vec![Node::Directive(Directive::Sectnum(
                rinx_ast::SectnumOptions::default(),
            ))],
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.sectnum.contains_key("test.rst"));
}

#[test]
fn test_analyze_keeps_the_last_sectnum_directive_when_a_document_writes_two() {
    // Given
    let first = rinx_ast::SectnumOptions {
        prefix: "First ".to_string(),
        ..rinx_ast::SectnumOptions::default()
    };
    let last = rinx_ast::SectnumOptions {
        prefix: "Last ".to_string(),
        ..rinx_ast::SectnumOptions::default()
    };
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::Sectnum(first)),
            Node::Directive(Directive::Sectnum(last.clone())),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.sectnum.get("test.rst"), Some(&last));
}

#[test]
fn test_analyze_registers_a_flowcharts_name_as_a_target() {
    // Given — a `.. entity-flow::` with a `:name:` option
    let doc = Document::new(
        "specs.rst".to_string(),
        vec![Node::Directive(Directive::EntityFlow(Box::new(
            rinx_ast::EntityFlow {
                name: Some(TargetName::new("requirement-flow")),
                ..rinx_ast::EntityFlow::new(rinx_ast::EntityFlowSource::EntityFlow)
            },
        )))],
    );

    // When
    let index = analyze(&doc);

    // Then — a picture is a cross-reference target like any other
    assert!(
        index
            .targets
            .contains_key(&TargetName::new("requirement-flow"))
    );
}

#[test]
fn test_analyze_registers_a_sequence_diagrams_name_as_a_target() {
    // Given — a `.. entity-sequence::` with a `:name:` option
    let doc = Document::new(
        "specs.rst".to_string(),
        vec![Node::Directive(Directive::EntitySequence(Box::new(
            rinx_ast::EntitySequence {
                name: Some(TargetName::new("startup-sequence")),
                ..rinx_ast::EntitySequence::new(
                    rinx_ast::EntitySequenceSource::EntitySequence,
                    rinx_ast::NonEmptyVector::single(rinx_ast::EntityId::new("COMP_UI").unwrap()),
                    rinx_ast::NonEmptyVector::single("sends".to_string()),
                )
            },
        )))],
    );

    // When
    let index = analyze(&doc);

    // Then — a picture is a cross-reference target like any other
    assert!(
        index
            .targets
            .contains_key(&TargetName::new("startup-sequence"))
    );
}

#[test]
fn test_analyze_registers_a_bar_charts_name_as_a_target() {
    // Given — a `.. entity-bar::` with a `:name:` option
    let doc = Document::new(
        "report.rst".to_string(),
        vec![Node::Directive(Directive::EntityBar(Box::new(
            rinx_ast::EntityBar {
                name: Some(TargetName::new("authors-chart")),
                ..rinx_ast::EntityBar::new(
                    rinx_ast::EntityBarSource::NeedBar,
                    rinx_ast::BarGrid::default(),
                )
            },
        )))],
    );

    // When
    let index = analyze(&doc);

    // Then — a chart is a cross-reference target like any other picture
    assert!(
        index
            .targets
            .contains_key(&TargetName::new("authors-chart"))
    );
}

/// A target node with no URI — the `.. _name:` form that labels what follows.
fn label(name: &str) -> Node {
    Node::Target {
        name: TargetName::new(name),
        uri: None,
    }
}

fn heading(title: &str) -> Node {
    Node::Heading {
        level: 2,
        text: vec![InlineNode::Text(title.to_string())],
    }
}

#[test]
fn test_analyze_records_the_title_of_the_heading_a_target_labels() {
    // Given
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![label("install"), heading("Installing")],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.target_titles.get(&TargetName::new("install")),
        Some(&"Installing".to_string())
    );
}

#[test]
fn test_analyze_gives_every_chained_target_the_heading_title() {
    // Given — two labels stacked above one heading, which docutils chains
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![label("install"), label("setup"), heading("Installing")],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.target_titles.get(&TargetName::new("install")),
        Some(&"Installing".to_string())
    );
    assert_eq!(
        index.target_titles.get(&TargetName::new("setup")),
        Some(&"Installing".to_string())
    );
}

#[test]
fn test_analyze_records_no_title_for_a_target_labelling_a_paragraph() {
    // Given
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![
            label("note"),
            Node::Paragraph(vec![InlineNode::Text("text".to_string())]),
            heading("Later"),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.target_titles.is_empty());
}

#[test]
fn test_analyze_records_no_title_across_a_comment() {
    // Given — a comment is invisible in docutils, so it ends the chain
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![label("install"), Node::Comment, heading("Installing")],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.target_titles.is_empty());
}

#[test]
fn test_analyze_records_no_title_for_an_external_hyperlink_target() {
    // Given — a target with a URI names a URL, not the heading after it
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![
            Node::Target {
                name: TargetName::new("python"),
                uri: Some("https://python.org".to_string()),
            },
            heading("Installing"),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.target_titles.is_empty());
}

#[test]
fn test_analyze_records_the_title_of_a_heading_labelled_inside_a_directive_body() {
    // Given — a target and heading inside an admonition body
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![Node::Directive(Directive::SeeAlso {
            body: vec![label("inner"), heading("Inner")],
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.target_titles.get(&TargetName::new("inner")),
        Some(&"Inner".to_string())
    );
}

fn figure(caption: Option<&str>, name: Option<&str>) -> Node {
    let mut image = rinx_ast::ImageOptions::new(rinx_ast::AssetUri::new("logo.png"));
    image.name = name.map(TargetName::new);
    let mut figure = rinx_ast::Figure::new(image);
    figure.caption = caption.map(|text| vec![InlineNode::Text(text.to_string())]);
    Node::Directive(Directive::Figure(Box::new(figure)))
}

#[test]
fn test_analyze_records_a_figure_caption_for_the_label_above_it() {
    // Given — Sphinx shows a figure's caption for a `:ref:` to its label
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![label("logo"), figure(Some("The logo"), None)],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.target_titles.get(&TargetName::new("logo")),
        Some(&"The logo".to_string())
    );
}

#[test]
fn test_analyze_records_a_figure_caption_for_its_own_name() {
    // Given
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![figure(Some("The logo"), Some("logo-figure"))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.target_titles.get(&TargetName::new("logo-figure")),
        Some(&"The logo".to_string())
    );
}

#[test]
fn test_analyze_records_no_title_for_a_figure_without_a_caption() {
    // Given
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![label("logo"), figure(None, Some("logo-figure"))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.target_titles.is_empty());
}

#[test]
fn test_analyze_records_a_table_title_for_its_label_and_name() {
    // Given
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![
            label("sizes"),
            Node::Directive(Directive::Table {
                title: Some("Sizes".to_string()),
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: Some(TargetName::new("sizes-table")),
                header_rows: vec![],
                body_rows: vec![],
            }),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.target_titles.get(&TargetName::new("sizes")),
        Some(&"Sizes".to_string())
    );
    assert_eq!(
        index.target_titles.get(&TargetName::new("sizes-table")),
        Some(&"Sizes".to_string())
    );
}

#[test]
fn test_analyze_records_a_code_block_caption_for_its_name() {
    // Given — Sphinx shows a code block's `:caption:` for a `:ref:` to it
    let mut node = code_block_named(Some("my-code"));
    if let Node::Directive(Directive::CodeBlock(block)) = &mut node {
        block.caption = Some("example.py".to_string());
    }
    let doc = Document::new("test.rst".to_string(), vec![node]);

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.target_titles.get(&TargetName::new("my-code")),
        Some(&"example.py".to_string())
    );
}

#[test]
fn test_analyze_records_an_entity_anchor_for_its_target() {
    // Given — an entity renders at `entity-<id>`, not at its lowercased name
    let body = rinx_ast::EntityBody {
        type_name: "req".to_string(),
        id: rinx_ast::EntityId::new("REQ_001").unwrap(),
        attributes: std::collections::BTreeMap::new(),
        relations: std::collections::BTreeMap::new(),
        sections: vec![],
        span: None,
    };
    let doc = Document::new(
        "reqs.rst".to_string(),
        vec![Node::Directive(Directive::Entity(Box::new(body)))],
    );

    // When
    let index = analyze(&doc);

    // Then
    let name = TargetName::new("REQ_001");
    assert!(index.targets.contains_key(&name));
    assert_eq!(index.target_anchor(&name), "entity-REQ_001");
}
