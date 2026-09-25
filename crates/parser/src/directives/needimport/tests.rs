use std::collections::HashMap;

use rusty_sphinx_ast::{
    AttributeValue, DiagnosticCode, Directive, Document, Domain, EntityBody, Node,
};
use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};

use crate::context::{LoadedFile, ParseCtx, ParseFileLoader};

/// The schema every test below imports against.
///
/// Deliberately close to `examples/entities/entities.toml`'s first half, since
/// that is the vocabulary a migrating sphinx-needs project actually has.
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

          [[entity_type.attribute]]
          name = "priority"
          type = "int"

          [[entity_type.attribute]]
          name = "tags"
          type = "list<string>"

          [[entity_type.attribute]]
          name = "urgent"
          type = "bool"

          [[entity_type.relation]]
          name = "links"
          to = ["req", "spec"]
          multiple = true
          incoming = "linked_by"

        [[entity_type]]
        name = "spec"
        argument = { fields = ["title"] }

          [[entity_type.attribute]]
          name = "title"
          type = "string"

          [[entity_type.attribute]]
          name = "owner"
          type = "string"
          required = true

        [[entity_type]]
        name = "risk"
        id = { pattern = "^HAZ_" }

          [[entity_type.attribute]]
          name = "code"
          type = "string"
          pattern = "^H[0-9]+$"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

/// A loader serving a fixed set of files, as the other parse-time readers'
/// tests use.
struct FakeFiles(HashMap<String, String>);

impl FakeFiles {
    fn with(files: &[(&str, &str)]) -> Self {
        Self(
            files
                .iter()
                .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
                .collect(),
        )
    }
}

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

/// A `needs.json` holding two requirements, one of which links to the other.
const TWO_REQUIREMENTS: &str = r##"{
    "current_version": "1.0",
    "versions": {
        "1.0": {
            "needs": {
                "REQ_1": {
                    "id": "REQ_1",
                    "type": "req",
                    "title": "Boot quickly",
                    "status": "open",
                    "tags": ["startup", "perf"],
                    "links": ["REQ_2"],
                    "content": "The system boots in under a second.",
                    "docname": "requirements",
                    "lineno": 12,
                    "type_color": "#BFD8D2"
                },
                "REQ_2": {
                    "id": "REQ_2",
                    "type": "req",
                    "title": "Shut down cleanly",
                    "status": "closed"
                }
            }
        },
        "2.0": {
            "needs": {
                "REQ_9": {"id": "REQ_9", "type": "req", "title": "Only in 2.0"}
            }
        }
    }
}"##;

/// Parses `rst` against the test schema with `files` available.
fn parse_with(files: &[(&str, &str)], rst: &str) -> Document {
    let schema = schema();
    let loader = FakeFiles::with(files);
    let ctx = ParseCtx::new(Domain::Py, &loader).with_schema(&schema);
    crate::parse_with_ctx("guide", rst, &ctx)
}

/// Parses `rst` with `files` available and `keys` declared as the schema's
/// `[import_keys]`, already resolved the way the worker resolves them.
fn parse_with_keys(files: &[(&str, &str)], keys: &[(&str, &str)], rst: &str) -> Document {
    let schema = schema();
    let loader = FakeFiles::with(files);
    let import_keys: std::collections::BTreeMap<String, String> = keys
        .iter()
        .map(|(alias, path)| ((*alias).to_string(), (*path).to_string()))
        .collect();
    let ctx = ParseCtx::new(Domain::Py, &loader)
        .with_schema(&schema)
        .with_import_keys(&import_keys);
    crate::parse_with_ctx("guide", rst, &ctx)
}

/// Parses a document importing `TWO_REQUIREMENTS` with the given option lines.
fn import(options: &str) -> Document {
    let rst = format!(".. needimport:: needs.json\n{options}\n");
    parse_with(&[("needs.json", TWO_REQUIREMENTS)], &rst)
}

fn entities(document: &Document) -> Vec<EntityBody> {
    document
        .nodes
        .iter()
        .filter_map(|node| match node {
            Node::Directive(Directive::Entity(entity)) => Some((**entity).clone()),
            _ => None,
        })
        .collect()
}

fn ids(document: &Document) -> Vec<String> {
    entities(document)
        .iter()
        .map(|entity| entity.id.as_str().to_string())
        .collect()
}

fn codes(document: &Document) -> Vec<DiagnosticCode> {
    document.diagnostics.iter().map(|d| d.code).collect()
}

fn messages(document: &Document) -> String {
    document
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

// ── The happy path ──────────────────────────────────────────────────

#[test]
fn imports_every_need_of_the_current_version_as_an_entity() {
    // Given a needs.json naming a current version holding two needs
    // When it is imported with no options
    let document = import("");

    // Then both become entities of this document, in id order, with nothing
    // reported
    assert_eq!(ids(&document), ["REQ_1", "REQ_2"]);
    assert!(codes(&document).is_empty(), "{}", messages(&document));
}

#[test]
fn an_imported_entity_carries_the_same_body_a_written_one_does() {
    // Given a need with a title, an enum, a list and a relation
    // When it is imported
    let document = import("");
    let entity = entities(&document)
        .into_iter()
        .find(|entity| entity.id.as_str() == "REQ_1")
        .expect("REQ_1 was imported");

    // Then every part of it is an ordinary EntityBody: the declared type, the
    // typed attributes, the relation targets and the parsed prose
    assert_eq!(entity.type_name, "req");
    assert_eq!(entity.title().as_deref(), Some("Boot quickly"));
    assert_eq!(
        entity.attributes.get("status"),
        Some(&AttributeValue::String("open".to_string()))
    );
    assert_eq!(
        entity.attributes.get("tags"),
        Some(&AttributeValue::List(vec![
            "startup".to_string(),
            "perf".to_string()
        ]))
    );
    let links: Vec<&str> = entity
        .relation_targets("links")
        .iter()
        .map(rusty_sphinx_ast::EntityId::as_str)
        .collect();
    assert_eq!(links, ["REQ_2"]);
    assert_eq!(entity.content().len(), 1);
}

#[test]
fn the_imported_entities_are_spliced_into_the_enclosing_block() {
    // Given prose on both sides of an import
    let rst = "Before.\n\n.. needimport:: needs.json\n\nAfter.\n";

    // When the document is parsed
    let document = parse_with(&[("needs.json", TWO_REQUIREMENTS)], rst);

    // Then the entities sit at the document's own level rather than inside a
    // container node, which is what lets the analyzer index them
    let kinds: Vec<&str> = document
        .nodes
        .iter()
        .map(|node| match node {
            Node::Paragraph(_) => "paragraph",
            Node::Directive(Directive::Entity(_)) => "entity",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["paragraph", "entity", "entity", "paragraph"]);
}

#[test]
fn applies_the_schemas_defaults_to_an_imported_entity() {
    // Given a need that sets no status, whose type declares a default
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T"}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then the declared default is supplied, exactly as for a written entity
    let entity = &entities(&document)[0];
    assert_eq!(
        entity.attributes.get("status"),
        Some(&AttributeValue::String("open".to_string()))
    );
}

#[test]
fn reports_a_required_attribute_the_imported_need_omits() {
    // Given a spec, whose type requires an owner, without one
    let json = r#"{"versions": {"1": {"needs": {
        "S": {"id": "S", "type": "spec", "title": "T"}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then the same diagnostic a written `.. spec::` would get is reported
    assert_eq!(
        codes(&document),
        [DiagnosticCode::EntityMissingRequiredAttribute]
    );
}

#[test]
fn reports_an_imported_id_outside_its_types_pattern_and_keeps_it() {
    // Given a risk whose id breaks the type's naming convention
    let json = r#"{"versions": {"1": {"needs": {
        "RISK_1": {"id": "RISK_1", "type": "risk"}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then the same diagnostic a written `.. risk::` would get is reported,
    // and the need keeps the id every link in the file points at
    assert_eq!(codes(&document), [DiagnosticCode::EntityIdPatternMismatch]);
    assert!(messages(&document).contains("RISK_1"));
    assert_eq!(ids(&document), ["RISK_1"]);
}

#[test]
fn reports_an_imported_value_outside_its_attributes_pattern() {
    // Given a risk whose code breaks the attribute's pattern
    let json = r#"{"versions": {"1": {"needs": {
        "HAZ_1": {"id": "HAZ_1", "type": "risk", "code": "X9"}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then the value is refused exactly as a written one would be
    assert_eq!(
        codes(&document),
        [DiagnosticCode::EntityInvalidAttributeValue]
    );
    assert!(!entities(&document)[0].attributes.contains_key("code"));
}

// ── Choosing a version ──────────────────────────────────────────────

#[test]
fn version_selects_a_block_other_than_the_current_one() {
    // Given a file whose current version is 1.0
    // When 2.0 is asked for
    let document = import("   :version: 2.0");

    // Then only that version's needs are imported
    assert_eq!(ids(&document), ["REQ_9"]);
}

#[test]
fn reports_a_version_the_file_does_not_hold() {
    // Given a file holding 1.0 and 2.0
    // When 9.9 is asked for
    let document = import("   :version: 9.9");

    // Then it is refused, and nothing is imported
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportUnknownVersion]);
    assert!(entities(&document).is_empty());
    assert!(messages(&document).contains("9.9"));
}

// ── Narrowing the set ───────────────────────────────────────────────

#[test]
fn ids_imports_only_the_needs_it_names() {
    // Given a file holding two needs
    // When one is named
    let document = import("   :ids: REQ_2");

    // Then only it is imported
    assert_eq!(ids(&document), ["REQ_2"]);
}

#[test]
fn reports_an_ids_entry_the_file_does_not_hold() {
    // Given a file holding REQ_1 and REQ_2
    // When a third is named beside a real one
    let document = import("   :ids: REQ_1, REQ_404");

    // Then the missing one is reported and the real one still imports
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportUnknownId]);
    assert_eq!(ids(&document), ["REQ_1"]);
}

#[test]
fn filter_selects_on_a_declared_attribute() {
    // Given needs with different statuses
    // When a filter names one
    let document = import(r#"   :filter: status == "closed""#);

    // Then only the matching need is imported
    assert_eq!(ids(&document), ["REQ_2"]);
}

#[test]
fn filter_reads_a_list_attribute_the_way_the_index_will() {
    // Given a need whose tags the file spells as a JSON array
    // When a containment filter is written over them
    let document = import(r#"   :filter: "startup" in tags"#);

    // Then it matches, because the value went through the declared type
    assert_eq!(ids(&document), ["REQ_1"]);
}

#[test]
fn filter_selects_on_the_built_in_fields() {
    // Given the built-in field vocabulary
    // When a filter names the type
    let document = import(r#"   :filter: type == "req""#);

    // Then every need of that type is selected
    assert_eq!(ids(&document), ["REQ_1", "REQ_2"]);
}

#[test]
fn reports_a_filter_the_language_cannot_parse() {
    // Given an expression outside this build's filter language
    // When it is written
    let document = import("   :filter: len(tags) > 0");

    // Then it is refused by name, and the import falls back to everything
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportInvalidFilter]);
    assert_eq!(ids(&document), ["REQ_1", "REQ_2"]);
}

#[test]
fn reports_a_filter_naming_a_field_no_type_declares() {
    // Given a filter over a field the schema does not declare
    // When it is written
    let document = import(r#"   :filter: asil == "B""#);

    // Then the unknown field is reported, offering the vocabulary — and the
    // selection it could not make is reported too, since a filter over a field
    // nothing declares matches nothing
    assert_eq!(
        codes(&document),
        [
            DiagnosticCode::NeedImportUnknownFilterField,
            DiagnosticCode::NeedImportEmptyResult
        ]
    );
    assert!(messages(&document).contains("status"));
}

#[test]
fn reports_a_selection_that_matched_nothing() {
    // Given a filter no need satisfies
    // When it is written
    let document = import(r#"   :filter: status == "draft""#);

    // Then the empty result is reported rather than passing silently
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportEmptyResult]);
    assert!(entities(&document).is_empty());
}

// ── Ids the project does not own ────────────────────────────────────

#[test]
fn id_prefix_renames_every_imported_entity() {
    // Given an import under a prefix
    // When the entities are built
    let document = import("   :id_prefix: EXT_");

    // Then each carries the prefixed id
    assert_eq!(ids(&document), ["EXT_REQ_1", "EXT_REQ_2"]);
}

#[test]
fn id_prefix_rewrites_a_link_into_the_same_import() {
    // Given REQ_1 linking to REQ_2, both imported under a prefix
    // When the entities are built
    let document = import("   :id_prefix: EXT_");
    let entity = entities(&document)
        .into_iter()
        .find(|entity| entity.id.as_str() == "EXT_REQ_1")
        .expect("the prefixed entity exists");

    // Then the link follows the copy it was imported with
    let links: Vec<&str> = entity
        .relation_targets("links")
        .iter()
        .map(rusty_sphinx_ast::EntityId::as_str)
        .collect();
    assert_eq!(links, ["EXT_REQ_2"]);
}

#[test]
fn id_prefix_leaves_a_link_out_of_the_import_alone() {
    // Given a need linking to an id this import does not bring in
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T", "links": ["PROJECT_OWNED"]}}}}}"#;

    // When it is imported under a prefix
    let document = parse_with(
        &[("needs.json", json)],
        ".. needimport:: needs.json\n   :id_prefix: EXT_\n",
    );

    // Then the outside target keeps the id the project already knows it by
    let entity = &entities(&document)[0];
    let links: Vec<&str> = entity
        .relation_targets("links")
        .iter()
        .map(rusty_sphinx_ast::EntityId::as_str)
        .collect();
    assert_eq!(links, ["PROJECT_OWNED"]);
}

#[test]
fn reports_a_need_whose_id_is_not_a_legal_entity_id() {
    // Given a need whose id holds a character an entity id may not
    let json = r#"{"versions": {"1": {"needs": {
        "bad": {"id": "not a id!", "type": "req", "title": "T"}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then it is skipped rather than given an invented id
    assert!(codes(&document).contains(&DiagnosticCode::NeedImportInvalidId));
    assert!(entities(&document).is_empty());
}

#[test]
fn falls_back_to_the_map_key_when_a_need_names_no_id() {
    // Given a need with no `id` field
    let json = r#"{"versions": {"1": {"needs": {
        "REQ_K": {"type": "req", "title": "T"}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then the key it is stored under is its id
    assert_eq!(ids(&document), ["REQ_K"]);
}

// ── The schema is the authority ─────────────────────────────────────

#[test]
fn ignores_sphinx_needs_own_bookkeeping_fields() {
    // Given a need carrying docname, lineno and type_color
    // When it is imported
    let document = import("");
    let entity = entities(&document)
        .into_iter()
        .find(|entity| entity.id.as_str() == "REQ_1")
        .expect("REQ_1 was imported");

    // Then none of them became an attribute and none was reported
    assert!(!entity.attributes.contains_key("docname"));
    assert!(!entity.attributes.contains_key("lineno"));
    assert!(!entity.attributes.contains_key("type_color"));
    assert!(codes(&document).is_empty(), "{}", messages(&document));
}

#[test]
fn reports_a_field_the_type_does_not_declare() {
    // Given a need carrying project data the schema has no home for
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T", "asil": "B"}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then it is reported, offering the type's vocabulary, and the entity
    // still imports
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportUnknownField]);
    assert!(messages(&document).contains(":status:"));
    assert_eq!(ids(&document), ["R"]);
}

#[test]
fn ignores_an_undeclared_field_that_carries_no_value() {
    // Given the shape a real export writes: every registered option present,
    // most of them empty. sphinx-needs' own demo exports sixteen such fields
    // per need, registered by its github and jira services.
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T",
              "avatar": "", "query": "   ", "params": [], "service": ""}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then none of them is reported — the author wrote none of them, and an
    // empty value says nothing that could be stored anyway
    assert!(codes(&document).is_empty(), "{}", messages(&document));
    assert_eq!(ids(&document), ["R"]);
}

#[test]
fn still_reports_an_undeclared_field_that_carries_a_value() {
    // Given one empty undeclared field and one holding real project data
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T", "avatar": "", "asil": "B"}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then only the one saying something is reported, so quieting the noise
    // cannot hide a schema that is genuinely missing a declaration
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportUnknownField]);
    assert!(messages(&document).contains("asil"));
}

#[test]
fn ignores_a_back_link_the_schema_derives() {
    // Given an export storing both directions of a link, as sphinx-needs does
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T", "linked_by": ["SOMETHING"]}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then the stored copy is dropped without a word: this build derives the
    // incoming side project-wide, and ingesting a file's idea of it would
    // duplicate something already computed
    assert!(codes(&document).is_empty(), "{}", messages(&document));
    assert!(!entities(&document)[0].relations.contains_key("linked_by"));
}

#[test]
fn reports_a_value_outside_its_declared_type() {
    // Given a status the enum does not permit
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T", "status": "wobbly"}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then the same diagnostic a written `:status:` would get is reported
    assert_eq!(
        codes(&document),
        [DiagnosticCode::EntityInvalidAttributeValue]
    );
}

#[test]
fn reads_a_json_number_into_an_int_attribute() {
    // Given a priority the file spells as a JSON number
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T", "priority": 3}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then it is stored as the declared type, not as text
    assert_eq!(
        entities(&document)[0].attributes.get("priority"),
        Some(&AttributeValue::Int(3))
    );
}

#[test]
fn treats_an_empty_json_string_as_an_unset_attribute() {
    // Given the empty value a real needs.json writes for an unset field
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T", "priority": ""}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then it is simply unset, rather than reported on every need in the file
    assert!(codes(&document).is_empty(), "{}", messages(&document));
    assert!(!entities(&document)[0].attributes.contains_key("priority"));
}

#[test]
fn treats_an_explicit_null_as_an_unset_attribute() {
    // Given the value an export writes for a `nullable` field a need left
    // blank — sphinx-needs' own demo does this for `status`
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T", "status": null}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then it is unset rather than malformed — `null` says nothing, where an
    // object says something this model cannot store — so the declared default
    // applies exactly as it would for an omitted field
    assert!(codes(&document).is_empty(), "{}", messages(&document));
    assert_eq!(
        entities(&document)[0].attributes.get("status"),
        Some(&AttributeValue::String("open".to_string()))
    );
}

#[test]
fn does_not_read_an_empty_json_value_as_a_set_flag() {
    // Given a bool attribute exported as empty, which means unset
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T", "urgent": ""}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then it stays unset. A bare `:urgent:` meaning true is RST option
    // syntax, not JSON: an export writes `true`/`false` for a flag it holds,
    // so reading the empty case as true would invent a value.
    assert!(codes(&document).is_empty(), "{}", messages(&document));
    assert!(!entities(&document)[0].attributes.contains_key("urgent"));
}

#[test]
fn reports_a_value_with_no_written_spelling() {
    // Given a field whose JSON value is an object
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": {"nested": 1}}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then it is reported rather than coerced into a string
    assert!(codes(&document).contains(&DiagnosticCode::NeedImportInvalidValue));
}

#[test]
fn reports_a_need_whose_type_the_schema_does_not_declare() {
    // Given a need of a type this project has no vocabulary for
    let json = r#"{"versions": {"1": {"needs": {
        "H": {"id": "H", "type": "hazard", "title": "T"}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then it is skipped, and the declared types are offered
    assert!(codes(&document).contains(&DiagnosticCode::NeedImportUnknownType));
    assert!(messages(&document).contains("req"));
    assert!(entities(&document).is_empty());
}

#[test]
fn reports_a_need_naming_no_type_at_all() {
    // Given a need with no `type` field
    let json = r#"{"versions": {"1": {"needs": {"X": {"id": "X"}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then it is skipped, naming the need
    assert!(codes(&document).contains(&DiagnosticCode::NeedImportUnknownType));
    assert!(messages(&document).contains("\"X\""));
}

// ── Tags ────────────────────────────────────────────────────────────

#[test]
fn tags_are_added_to_the_types_own_tags_attribute() {
    // Given a need already carrying two tags
    // When an import adds another
    let document = import("   :tags: imported");
    let entity = entities(&document)
        .into_iter()
        .find(|entity| entity.id.as_str() == "REQ_1")
        .expect("REQ_1 was imported");

    // Then it joins the ones the file held
    assert_eq!(
        entity.attributes.get("tags"),
        Some(&AttributeValue::List(vec![
            "startup".to_string(),
            "perf".to_string(),
            "imported".to_string()
        ]))
    );
}

#[test]
fn tags_are_given_to_a_need_that_had_none() {
    // Given a need with no tags of its own
    // When an import adds one
    let document = import("   :tags: imported");
    let entity = entities(&document)
        .into_iter()
        .find(|entity| entity.id.as_str() == "REQ_2")
        .expect("REQ_2 was imported");

    // Then the attribute is created
    assert_eq!(
        entity.attributes.get("tags"),
        Some(&AttributeValue::List(vec!["imported".to_string()]))
    );
}

#[test]
fn reports_tags_written_for_a_type_declaring_no_tag_list() {
    // Given a spec, whose type declares no `tags` attribute
    let json = r#"{"versions": {"1": {"needs": {
        "S": {"id": "S", "type": "spec", "title": "T", "owner": "alice"}}}}}"#;

    // When an import adds tags
    let document = parse_with(
        &[("needs.json", json)],
        ".. needimport:: needs.json\n   :tags: imported\n",
    );

    // Then the values are reported rather than silently dropped
    assert!(codes(&document).contains(&DiagnosticCode::NeedImportNoTagsAttribute));
}

// ── Refusals ────────────────────────────────────────────────────────

#[test]
fn reports_an_import_with_no_path() {
    // Given a directive with no argument
    // When it is parsed
    let document = parse_with(&[], ".. needimport::\n");

    // Then it is refused and drawn as an error block quoting the source
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportMissingPath]);
    assert!(matches!(
        document.nodes.first(),
        Some(Node::Directive(Directive::Malformed { .. }))
    ));
}

#[test]
fn refuses_a_url_by_name_rather_than_fetching_it() {
    // Given an argument naming a remote file
    // When it is parsed
    let document = parse_with(&[], ".. needimport:: https://example.test/needs.json\n");

    // Then it is refused, pointing at the local alternative
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportRemoteSource]);
    assert!(messages(&document).contains("parse_data"));
}

#[test]
fn refuses_an_http_url_as_well_as_an_https_one() {
    // Given the insecure spelling, in mixed case
    // When it is parsed
    let document = parse_with(&[], ".. needimport:: HTTP://example.test/needs.json\n");

    // Then it is refused the same way
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportRemoteSource]);
}

#[test]
fn refuses_a_name_no_import_key_declares() {
    // Given the argument sphinx-needs' own demo writes, and a schema that
    // declares no key for it
    // When it is parsed
    let document = parse_with(&[], ".. needimport:: imported_project\n");

    // Then it is refused as the missing *declaration* it is, rather than as a
    // file that happens to be missing — one diagnostic for both would blame
    // the filesystem, and would fail the build under the parse_data contract
    assert_eq!(
        codes(&document),
        [DiagnosticCode::NeedImportUnsupportedImportKey]
    );
    assert!(messages(&document).contains("[import_keys]"));
}

#[test]
fn refusing_a_name_offers_the_keys_the_schema_does_declare() {
    // Given a schema declaring one key, and a document naming another
    let document = parse_with_keys(
        &[],
        &[("upstream", "/needs.json")],
        ".. needimport:: typo_project\n",
    );

    // Then the vocabulary is offered, the way an unknown entity option lists
    // what its type accepts
    assert_eq!(
        codes(&document),
        [DiagnosticCode::NeedImportUnsupportedImportKey]
    );
    assert!(
        messages(&document).contains("upstream"),
        "{}",
        messages(&document)
    );
}

#[test]
fn an_import_key_resolves_to_the_file_it_names() {
    // Given a schema mapping an alias onto a needs.json
    // When a document imports through the alias
    let document = parse_with_keys(
        &[("/data/needs.json", TWO_REQUIREMENTS)],
        &[("upstream_platform", "/data/needs.json")],
        ".. needimport:: upstream_platform\n",
    );

    // Then the file behind it is read, and nothing is reported
    assert_eq!(ids(&document), ["REQ_1", "REQ_2"]);
    assert!(codes(&document).is_empty(), "{}", messages(&document));
}

#[test]
fn an_import_key_is_looked_up_before_the_path_check() {
    // Given a key whose *name* is spelled like a filename, and a different
    // file of that name
    let other = r#"{"versions": {"1": {"needs": {
        "OTHER": {"id": "OTHER", "type": "req", "title": "T"}}}}}"#;

    // When a document names it
    let document = parse_with_keys(
        &[
            ("needs.json", other),
            ("/data/needs.json", TWO_REQUIREMENTS),
        ],
        &[("needs.json", "/data/needs.json")],
        ".. needimport:: needs.json\n",
    );

    // Then the key wins over the file, which is the order sphinx-needs
    // resolves in
    assert_eq!(ids(&document), ["REQ_1", "REQ_2"]);
}

#[test]
fn an_unreadable_file_behind_a_key_is_still_a_build_failure() {
    // Given a key pointing at a file nothing serves
    // When a document imports through it
    let document = parse_with_keys(
        &[],
        &[("upstream", "/data/absent.json")],
        ".. needimport:: upstream\n",
    );

    // Then the *file* is the fault, not the declaration — the key resolved
    // fine, so this must reach the loader and fail the build like every other
    // undeclared parse-time read
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportFileUnreadable]);
}

#[test]
fn an_import_key_never_reaches_the_file_loader() {
    // Given a loader that records every path it is asked for
    struct RecordingFiles(std::cell::RefCell<Vec<String>>);
    impl ParseFileLoader for RecordingFiles {
        fn load(&self, path: &str, _relative_to: Option<&str>) -> Result<LoadedFile, String> {
            self.0.borrow_mut().push(path.to_string());
            Err(format!("cannot read '{path}'"))
        }
    }

    // When an import key is parsed
    let schema = schema();
    let loader = RecordingFiles(std::cell::RefCell::new(Vec::new()));
    let ctx = ParseCtx::new(Domain::Py, &loader).with_schema(&schema);
    let _ = crate::parse_with_ctx("guide", ".. needimport:: imported_project\n", &ctx);

    // Then no read was attempted at all. This is what keeps the refusal a
    // warning: the worker fails the build on any *recorded* loader failure,
    // so asking for a path that was never a path would turn an unsupported
    // feature into a broken build.
    assert!(
        loader.0.borrow().is_empty(),
        "an import key should never be opened as a file, but {:?} was requested",
        loader.0.borrow()
    );
}

#[test]
fn still_reads_a_path_whose_name_is_unusual_but_ends_in_json() {
    // Given a file named without the conventional `needs` stem
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T"}}}}}"#;

    // When it is imported
    let document = parse_with(
        &[("exports/2026-Q3.JSON", json)],
        ".. needimport:: exports/2026-Q3.JSON\n",
    );

    // Then the suffix check is on the extension only, and case-insensitive
    assert_eq!(ids(&document), ["R"]);
}

#[test]
fn reports_a_file_that_cannot_be_read() {
    // Given a path no file answers to
    // When it is parsed
    let document = parse_with(&[], ".. needimport:: missing.json\n");

    // Then the loader's own message is reported
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportFileUnreadable]);
}

#[test]
fn reports_a_file_that_is_not_a_needs_json() {
    // Given a file that is not JSON at all
    // When it is imported
    let document = parse_with(
        &[("needs.json", "not json")],
        ".. needimport:: needs.json\n",
    );

    // Then it is refused, quoting the deserializer
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportMalformedJson]);
}

#[test]
fn refuses_sphinx_needs_presentation_options_by_name() {
    // Given the options this build cannot honour
    // When each is written
    for option in [
        "hide",
        "collapse",
        "layout",
        "style",
        "setup",
        "pre_template",
        "post_template",
    ] {
        let document = import(&format!("   :{option}: x"));

        // Then it is refused by name, with advice, and the import still
        // happens
        assert!(
            codes(&document).contains(&DiagnosticCode::NeedImportUnsupportedOption),
            "{option} should be refused by name"
        );
        assert_eq!(ids(&document), ["REQ_1", "REQ_2"], "{option}");
    }
}

#[test]
fn reports_an_option_it_does_not_accept_at_all() {
    // Given an option belonging to no directive
    // When it is written
    let document = import("   :wobbly: yes");

    // Then it is reported as unknown rather than as unsupported
    assert_eq!(codes(&document), [DiagnosticCode::NeedImportUnknownOption]);
}

// ── Imported prose ──────────────────────────────────────────────────

#[test]
fn parses_content_as_restructuredtext() {
    // Given a need whose content holds markup
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T",
              "content": "Some **bold** prose.\n\nA second paragraph."}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then it becomes real nodes rather than a literal string
    let content = entities(&document)[0].content().to_vec();
    assert_eq!(content.len(), 2);
    assert!(matches!(content[0], Node::Paragraph(_)));
}

#[test]
fn gives_an_imported_entity_no_sections_when_it_has_no_content() {
    // Given a need with no content
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T", "content": "   "}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then it has no sections at all, rather than an empty one
    assert!(entities(&document)[0].sections.is_empty());
}

#[test]
fn reports_a_diagnostic_from_imported_prose_without_a_position() {
    // Given content holding a directive this build does not know
    let json = r#"{"versions": {"1": {"needs": {
        "R": {"id": "R", "type": "req", "title": "T",
              "content": ".. wobbly::\n\n   body"}}}}}"#;

    // When it is imported
    let document = parse_with(&[("needs.json", json)], ".. needimport:: needs.json\n");

    // Then the problem is still reported, but carries no position — JSON has
    // no line a reader could open to
    let unknown = document
        .diagnostics
        .iter()
        .find(|d| d.code == DiagnosticCode::DirectiveUnknown)
        .expect("the unknown directive is reported");
    assert!(unknown.span.is_none());
}

#[test]
fn reports_an_import_problem_at_the_directive_line() {
    // Given a document whose import sits on the third line
    let json = r#"{"versions": {"1": {"needs": {
        "H": {"id": "H", "type": "hazard", "title": "T"}}}}}"#;
    let rst = "Intro.\n\n.. needimport:: needs.json\n";

    // When the unknown type is reported
    let document = parse_with(&[("needs.json", json)], rst);
    let reported = document
        .diagnostics
        .iter()
        .find(|d| d.code == DiagnosticCode::NeedImportUnknownType)
        .expect("the unknown type is reported");

    // Then it points at the line the author can act on
    assert_eq!(
        reported
            .span
            .expect("the directive has a position")
            .start
            .line,
        3
    );
}

// ── Helpers ─────────────────────────────────────────────────────────

#[test]
fn is_url_recognises_only_the_two_fetchable_schemes() {
    // Given arguments that do and do not name something to fetch
    // When each is tested
    // Then only http and https count, so a path holding a colon still reads
    // as a path
    assert!(super::is_url("http://example.test/n.json"));
    assert!(super::is_url("https://example.test/n.json"));
    assert!(super::is_url("HTTPS://example.test/n.json"));
    assert!(!super::is_url("needs.json"));
    assert!(!super::is_url("/shared/needs.json"));
    assert!(!super::is_url("C:/data/needs.json"));
    assert!(!super::is_url("ftp://example.test/n.json"));
}

#[test]
fn unsupported_advice_answers_for_every_listed_option_and_no_other() {
    // Given the refusal table
    // When each listed name is looked up, and one that is not
    // Then every entry has advice and nothing else does
    for (option, _) in super::UNSUPPORTED_OPTIONS {
        assert!(
            super::unsupported_advice(option).is_some(),
            "{option} should carry advice"
        );
    }
    assert!(super::unsupported_advice("version").is_none());
    assert!(super::unsupported_advice("filter").is_none());
}

#[test]
fn prefix_internal_targets_rewrites_only_the_imported_ids() {
    // Given one target inside the import and one outside
    let imported = ["REQ_1"].into_iter().collect();

    // When the targets are prefixed
    let written = super::prefix_internal_targets("REQ_1, OUTSIDE", "EXT_", &imported);

    // Then only the imported one is renamed
    assert_eq!(written, "EXT_REQ_1, OUTSIDE");
}

#[test]
fn prefix_internal_targets_leaves_text_alone_without_a_prefix() {
    // Given no prefix
    let imported = ["REQ_1"].into_iter().collect();

    // When the targets are prefixed
    let written = super::prefix_internal_targets("REQ_1, OUTSIDE", "", &imported);

    // Then the text is untouched, spacing included
    assert_eq!(written, "REQ_1, OUTSIDE");
}

#[test]
fn names_a_json_file_separates_a_path_from_an_import_key() {
    // Given arguments of both kinds
    // When each is tested
    // Then only something ending in `.json` counts as a path to open
    assert!(super::names_a_json_file("needs.json"));
    assert!(super::names_a_json_file("/shared/needs.json"));
    assert!(super::names_a_json_file("../exports/NEEDS.JSON"));
    assert!(!super::names_a_json_file("imported_project"));
    assert!(!super::names_a_json_file("needs"));
    assert!(!super::names_a_json_file("needs.jsonl"));
    assert!(!super::names_a_json_file("json"));
}
