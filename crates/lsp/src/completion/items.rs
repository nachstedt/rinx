//! The completion items a reference role is offered, from a project index.

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionTextEdit, Range,
    TextEdit,
};
use rinx_index::ProjectIndex;

use super::context::{CompletedRole, RoleContext};
use super::doc_name::{absolute_doc_name, relative_doc_name};

/// What a document with no title is shown as, as Sphinx shows it.
const NO_TITLE: &str = "<no title>";

/// The items completing `context` in the document at `doc_path`, each
/// replacing `range` — the protocol's spelling of `context.replace`.
///
/// A label offers the title a bare `:ref:` to it would show, and the
/// document defining it; a document offers its title. Document names are
/// relative to `doc_path` unless the author began the name with `/`.
#[must_use]
pub fn completion_items(
    index: &ProjectIndex,
    doc_path: &str,
    context: &RoleContext,
    range: Range,
) -> Vec<CompletionItem> {
    match context.role {
        CompletedRole::Ref => ref_items(index, range),
        CompletedRole::Doc => doc_items(index, doc_path, context.typed.starts_with('/'), range),
    }
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
}
