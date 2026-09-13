//! `.. needimport::` across the whole pipeline.
//!
//! The unit tests in `rusty_sphinx_parser` assert on the nodes an import
//! produces. These assert the claim those tests cannot reach, and the claim
//! the whole design rests on: **an imported need is an ordinary entity of this
//! project**. Nothing in the analyzer, the index or the renderer was taught
//! about importing, so if these pass, they pass because the entity that
//! arrived is indistinguishable from one an author typed.
//!
//! Four things are checked, each of which would fail if the import were a
//! container node or an index-time merge instead: it is in `ProjectIndex`,
//! it is a `:ref:` target, back-links derive *through* it, and a filter over
//! the graph selects it.

use std::collections::HashMap;

use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_ast as ast;
use rusty_sphinx_ast::EntityId;
use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};
use rusty_sphinx_index::ProjectIndex;
use rusty_sphinx_parser::{self as parser, LoadedFile, ParseCtx, ParseFileLoader};
use rusty_sphinx_renderer as renderer;

/// A project declaring the sphinx-needs-shaped vocabulary a migrating project
/// has: requirements that link to specifications, with the back-link derived.
fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
        label = "Requirement"
        argument = { fields = ["title"] }

          [[entity_type.attribute]]
          name = "title"
          type = "string"

          [[entity_type.attribute]]
          name = "status"
          type = "enum"
          values = ["open", "closed"]
          default = "open"

          [[entity_type.relation]]
          name = "links"
          label = "Links to"
          to = ["spec"]
          multiple = true
          incoming = "linked_by"
          incoming_label = "Linked by"

        [[entity_type]]
        name = "spec"
        argument = { fields = ["title"] }

          [[entity_type.attribute]]
          name = "title"
          type = "string"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

/// A `needs.json` holding one requirement that links to a specification the
/// *document* declares, so the import has to join a graph it did not bring.
const NEEDS_JSON: &str = r#"{
    "current_version": "1.0",
    "versions": {
        "1.0": {
            "needs": {
                "REQ_IMPORTED": {
                    "id": "REQ_IMPORTED",
                    "type": "req",
                    "title": "Imported requirement",
                    "status": "closed",
                    "links": ["SPEC_LOCAL"],
                    "content": "Imported prose.",
                    "docname": "elsewhere"
                }
            }
        }
    }
}"#;

struct FakeFiles(HashMap<String, String>);

impl ParseFileLoader for FakeFiles {
    fn load(&self, path: &str, _relative_to: Option<&str>) -> Result<LoadedFile, String> {
        self.0
            .get(path)
            .map(|text| LoadedFile {
                id: path.to_string(),
                text: text.clone(),
            })
            .ok_or_else(|| format!("cannot read '{path}': no such file"))
    }
}

/// Parses one document that imports `NEEDS_JSON`, then indexes it the way the
/// `index` subcommand does.
fn build(rst: &str) -> (ast::Document, ProjectIndex, EntitySchema) {
    let schema = schema();
    let loader = FakeFiles(HashMap::from([(
        "needs.json".to_string(),
        NEEDS_JSON.to_string(),
    )]));
    let ctx = ParseCtx::new(ast::Domain::Py, &loader).with_schema(&schema);
    let doc = parser::parse_with_ctx("requirements", rst, &ctx);
    let index = analyzer::build_project_index(std::slice::from_ref(&doc), "requirements", &schema);
    (doc, index, schema)
}

/// A document declaring one specification locally and importing one
/// requirement that points at it.
const DOCUMENT: &str = "\
Requirements
============

.. spec:: Local specification
   :id: SPEC_LOCAL

.. needimport:: needs.json

The imported one is :ref:`REQ_IMPORTED`.
";

fn id(written: &str) -> EntityId {
    EntityId::new(written).expect("a legal id")
}

#[test]
fn test_e2e_an_imported_need_is_an_ordinary_entity_in_the_project_index() {
    // Given — a document importing one need.
    let (doc, index, _) = build(DOCUMENT);

    // Then — the parse reported nothing, and the index holds the import
    // beside the locally written entity, with its attributes typed and its
    // document set to the one that imported it.
    assert!(
        doc.diagnostics.is_empty(),
        "expected a clean parse, got {:?}",
        doc.diagnostics
            .iter()
            .map(|d| (d.code.as_str(), d.message.as_str()))
            .collect::<Vec<_>>()
    );

    let record = index
        .entities
        .get(&id("REQ_IMPORTED"))
        .expect("the imported need is in the index");
    assert_eq!(record.type_name, "req");
    assert_eq!(record.doc_path, "requirements");
    assert_eq!(record.title.as_deref(), Some("Imported requirement"));
    assert_eq!(
        record.attributes.get("status").map(ToString::to_string),
        Some("closed".to_string())
    );
    // `docname` in the file named the *exporting* project's document. It is
    // sphinx-needs' own bookkeeping and must not have become an attribute.
    assert!(!record.attributes.contains_key("docname"));
}

#[test]
fn test_e2e_an_imported_entity_is_a_ref_target() {
    // Given — a document whose prose refers to the imported need by id.
    let (_, index, _) = build(DOCUMENT);

    // Then — the import registered a target like any entity, which is what
    // makes the `:ref:` in the prose resolvable. An index-time merge could not
    // have produced this: the target names the document the entity lives in.
    let target = index
        .targets
        .get(&ast::TargetName::new("REQ_IMPORTED"))
        .expect("the imported entity is a reference target");
    assert!(
        format!("{target:?}").contains("requirements"),
        "the target should name the importing document, got {target:?}"
    );
}

#[test]
fn test_e2e_back_links_derive_through_an_imported_relation() {
    // Given — an imported requirement linking to a locally written spec.
    let (_, index, _) = build(DOCUMENT);

    // Then — the incoming side is derived project-wide, so the *local* entity
    // knows it is linked by the *imported* one. Nothing in the analyzer was
    // told the edge came from a file.
    let incoming = index
        .entity_backlinks
        .get(&id("SPEC_LOCAL"))
        .expect("the local spec has back-links");
    assert_eq!(
        incoming.get("linked_by"),
        Some(&vec![id("REQ_IMPORTED")]),
        "the imported requirement should appear as an incoming link"
    );
}

#[test]
fn test_e2e_an_imported_entity_is_listed_by_a_filter_over_the_graph() {
    // Given — an `.. entity-table::` selecting on an attribute value that only
    // the imported entity carries.
    let rst = format!("{DOCUMENT}\n.. entity-table::\n   :filter: status == \"closed\"\n");
    let (doc, index, schema) = build(&rst);

    // Then — the listing directive selects it. The table resolves its rows
    // against `ProjectIndex` while rendering and has no idea an import
    // happened; it finds the entity because the entity is simply there.
    let output = renderer::render_with_assets(
        &doc,
        &index,
        "requirements",
        &rusty_sphinx_renderer::config::SiteConfig::default(),
        &rusty_sphinx_renderer::EmbeddedAssets::new(),
        &schema,
        &rusty_sphinx_renderer::EntityTemplates::new(),
    );
    assert!(
        output.html.contains("REQ_IMPORTED"),
        "the imported entity should be a row of the filtered table"
    );
    // And the locally written spec, whose status is not `closed`, is not —
    // so the table really filtered rather than listing everything.
    assert!(
        !output.html.contains(">SPEC_LOCAL</a></td>"),
        "the filter should have excluded the local spec"
    );
}

#[test]
fn test_e2e_an_unreadable_import_degrades_without_losing_the_rest() {
    // Given — a document importing a file that is not there, with prose after
    // it.
    let (doc, index, _) = build(
        "\
Requirements
============

.. needimport:: absent.json

Prose after the import.
",
    );

    // Then — the failure is reported, and the document still parsed: the
    // parser degrades rather than aborting, which is what the live preview
    // needs and what lets the `parse` subcommand decide the build's fate.
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == ast::DiagnosticCode::NeedImportFileUnreadable),
        "the unreadable file should be reported"
    );
    assert!(index.entities.is_empty());
    assert!(
        doc.nodes
            .iter()
            .any(|node| matches!(node, ast::Node::Paragraph(_))),
        "the prose after the failed import should survive"
    );
}
