//! The completion items a reference role is offered, from a project index.

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionTextEdit, Range,
    TextEdit,
};
use rinx_entity::EntitySchema;
use rinx_index::ProjectIndex;

use super::context::{CompletedRole, RoleContext};
use super::doc_name::{absolute_doc_name, relative_doc_name};

/// What a document with no title is shown as, as Sphinx shows it.
const NO_TITLE: &str = "<no title>";

/// The items completing `context` in the document at `doc_path`, each
/// replacing `range` — the protocol's spelling of `context.replace`.
///
/// A label offers the title a bare `:ref:` to it would show, and the
/// document defining it; a document offers its title; an entity its title,
/// type and document. Document names are relative to `doc_path` unless the
/// author began the name with `/`.
#[must_use]
pub fn completion_items(
    index: &ProjectIndex,
    schema: &EntitySchema,
    doc_path: &str,
    context: &RoleContext,
    range: Range,
) -> Vec<CompletionItem> {
    match &context.role {
        CompletedRole::Ref => ref_items(index, range),
        CompletedRole::Doc => doc_items(index, doc_path, context.typed.starts_with('/'), range),
        CompletedRole::Entity(role) => entity_items(index, schema, role, range),
    }
}

/// One item per entity of a type the role named `role` accepts.
fn entity_items(
    index: &ProjectIndex,
    schema: &EntitySchema,
    role: &str,
    range: Range,
) -> Vec<CompletionItem> {
    let Some(spec) = schema.role(role) else {
        return Vec::new();
    };
    index
        .entities
        .iter()
        .filter(|(_, record)| spec.accepts(&record.type_name))
        .map(|(id, record)| CompletionItem {
            detail: Some(record.display_text(id).to_string()),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(format!(" {}", record.type_name)),
                description: Some(record.doc_path.clone()),
            }),
            kind: Some(CompletionItemKind::REFERENCE),
            ..replacing(id.as_str().to_string(), range)
        })
        .collect()
}

/// One item per label the index resolves.
fn ref_items(index: &ProjectIndex, range: Range) -> Vec<CompletionItem> {
    index
        .targets
        .iter()
        .map(|(label, defined_in)| CompletionItem {
            detail: index.target_titles.get(label).cloned(),
            label_details: Some(CompletionItemLabelDetails {
                detail: None,
                description: Some(defined_in.clone()),
            }),
            kind: Some(CompletionItemKind::REFERENCE),
            ..replacing(label.as_str().to_string(), range)
        })
        .collect()
}

/// One item per document of the index, named as the document at `doc_path`
/// reads it — from the source root when `absolute`.
fn doc_items(
    index: &ProjectIndex,
    doc_path: &str,
    absolute: bool,
    range: Range,
) -> Vec<CompletionItem> {
    index
        .documents
        .iter()
        .chain(index.document_titles.keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|target| {
            let name = if absolute {
                absolute_doc_name(target)
            } else {
                relative_doc_name(doc_path, target)
            };
            CompletionItem {
                detail: Some(index.document_title(target).unwrap_or(NO_TITLE).to_string()),
                kind: Some(CompletionItemKind::FILE),
                ..replacing(name, range)
            }
        })
        .collect()
}

/// An item labelled `text` that writes `text` over `range`.
fn replacing(text: String, range: Range) -> CompletionItem {
    CompletionItem {
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range,
            new_text: text.clone(),
        })),
        label: text,
        ..CompletionItem::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;
    use rinx_ast::TargetName;

    fn range() -> Range {
        Range::new(Position::new(3, 6), Position::new(3, 9))
    }

    fn context(role: CompletedRole, typed: &str) -> RoleContext {
        RoleContext {
            role,
            typed: typed.to_string(),
            replace: 6..9,
        }
    }

    fn index() -> ProjectIndex {
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("install"), "guide/setup.rst".to_string());
        index
            .targets
            .insert(TargetName::new("note"), "index.rst".to_string());
        index
            .target_titles
            .insert(TargetName::new("install"), "Installing".to_string());
        index
            .documents
            .extend(["index.rst", "guide/setup.rst", "guide/notes.rst"].map(ToString::to_string));
        index
            .document_titles
            .insert("guide/setup.rst".to_string(), "Setup".to_string());
        index
    }

    /// Each item's label and detail.
    fn shown(items: &[CompletionItem]) -> Vec<(&str, Option<&str>)> {
        items
            .iter()
            .map(|item| (item.label.as_str(), item.detail.as_deref()))
            .collect()
    }

    #[test]
    fn test_completion_items_offers_every_label_with_its_title() {
        // When
        let items = completion_items(
            &index(),
            &EntitySchema::empty(),
            "index.rst",
            &context(CompletedRole::Ref, ""),
            range(),
        );

        // Then
        assert_eq!(
            shown(&items),
            [("install", Some("Installing")), ("note", None)]
        );
        assert_eq!(
            items[0]
                .label_details
                .as_ref()
                .and_then(|d| d.description.as_deref()),
            Some("guide/setup.rst")
        );
        assert_eq!(items[0].kind, Some(CompletionItemKind::REFERENCE));
    }

    #[test]
    fn test_completion_items_names_documents_relative_to_the_current_one() {
        // When
        let items = completion_items(
            &index(),
            &EntitySchema::empty(),
            "guide/notes.rst",
            &context(CompletedRole::Doc, "se"),
            range(),
        );

        // Then
        assert_eq!(
            shown(&items),
            [
                ("notes", Some(NO_TITLE)),
                ("setup", Some("Setup")),
                ("../index", Some(NO_TITLE)),
            ]
        );
        assert_eq!(items[0].kind, Some(CompletionItemKind::FILE));
    }

    #[test]
    fn test_completion_items_names_documents_from_the_root_after_a_slash() {
        // When
        let items = completion_items(
            &index(),
            &EntitySchema::empty(),
            "guide/notes.rst",
            &context(CompletedRole::Doc, "/"),
            range(),
        );

        // Then
        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, ["/guide/notes", "/guide/setup", "/index"]);
    }

    #[test]
    fn test_completion_items_includes_a_document_known_only_by_its_title() {
        // Given — an index that recorded the document only by its title
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("old.rst".to_string(), "Old".to_string());

        // When
        let items = doc_items(&index, "index.rst", false, range());

        // Then
        assert_eq!(shown(&items), [("old", Some("Old"))]);
    }

    #[test]
    fn test_ref_items_is_empty_for_an_index_without_labels() {
        // When / Then
        assert!(ref_items(&ProjectIndex::default(), range()).is_empty());
    }

    #[test]
    fn test_replacing_writes_its_label_over_the_range() {
        // When
        let item = replacing("install".to_string(), range());

        // Then
        assert_eq!(item.label, "install");
        assert_eq!(
            item.text_edit,
            Some(CompletionTextEdit::Edit(TextEdit {
                range: range(),
                new_text: "install".to_string(),
            }))
        );
    }

    /// A schema of two types, `req` and `spec`, with a role for each and
    /// sphinx-needs' `:need:` over both.
    fn needs_schema() -> EntitySchema {
        rinx_entity::load_schema(
            "[[entity_type]]\nname = \"req\"\n[[entity_type]]\nname = \"spec\"\n\
             [[role]]\nname = \"req\"\ntypes = [\"req\"]\n\
             [[role]]\nname = \"need\"\n",
            &rinx_entity::NoReservedNames,
        )
        .expect("a valid schema")
    }

    fn entity_index() -> ProjectIndex {
        let record = |type_name: &str, title: Option<&str>| rinx_index::EntityRecord {
            relation_spans: std::collections::BTreeMap::new(),
            type_name: type_name.to_string(),
            doc_path: "reqs.rst".to_string(),
            title: title.map(ToString::to_string),
            attributes: std::collections::BTreeMap::new(),
            outgoing: std::collections::BTreeMap::new(),
            uml: std::collections::BTreeMap::new(),
        };
        let mut index = ProjectIndex::default();
        index.entities.insert(
            rinx_ast::EntityId::new("R_1").expect("an id"),
            record("req", Some("Brakes")),
        );
        index.entities.insert(
            rinx_ast::EntityId::new("S_1").expect("an id"),
            record("spec", None),
        );
        index
    }

    #[test]
    fn test_completion_items_offers_every_entity_the_role_accepts() {
        // When
        let items = completion_items(
            &entity_index(),
            &needs_schema(),
            "index.rst",
            &context(CompletedRole::Entity("need".into()), ""),
            range(),
        );

        // Then each id is offered with its title, or the id without one
        assert_eq!(
            shown(&items),
            [("R_1", Some("Brakes")), ("S_1", Some("S_1"))]
        );
        assert_eq!(
            items[0]
                .label_details
                .as_ref()
                .and_then(|d| d.description.as_deref()),
            Some("reqs.rst")
        );
    }

    #[test]
    fn test_completion_items_offers_only_the_types_a_role_names() {
        // When
        let items = completion_items(
            &entity_index(),
            &needs_schema(),
            "index.rst",
            &context(CompletedRole::Entity("req".into()), ""),
            range(),
        );

        // Then
        assert_eq!(shown(&items), [("R_1", Some("Brakes"))]);
    }

    #[test]
    fn test_completion_items_offers_nothing_for_a_role_the_schema_lacks() {
        // When
        let items = completion_items(
            &entity_index(),
            &needs_schema(),
            "index.rst",
            &context(CompletedRole::Entity("spec".into()), ""),
            range(),
        );

        // Then
        assert!(items.is_empty());
    }
}
