//! The "Migrate to rinx" source action (roadmap #37): offered on a Sphinx
//! project's `conf.py`, it writes the files `rinx init --from-sphinx` would,
//! as one workspace edit the editor applies.
//!
//! New files are created rather than written to, and only a client that
//! announces creating files through an edit is offered the action; a plan
//! that would overwrite a file is offered to no client, as `rinx init`
//! refuses it too.

use lsp_types::{
    CodeAction, CodeActionKind, CreateFile, CreateFileOptions, DocumentChangeOperation,
    DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier, Position, Range, ResourceOp,
    TextDocumentEdit, TextEdit, WorkspaceEdit,
};

use crate::migration::{MigrationPlan, PlannedContent, PlannedFile};
use crate::position::{PositionEncoding, to_lsp_position};
use crate::uri::file_uri;

/// The kind the action is offered as: a source action, never a quick fix.
pub const MIGRATE_KIND: &str = "source.migrateToRinx";

/// The action applying `plan`, reading an existing file's text with `read`
/// to append at its end — `None` when the plan would overwrite a file or
/// names one no URI names.
#[must_use]
pub fn migrate_action(
    plan: &MigrationPlan,
    encoding: PositionEncoding,
    read: &dyn Fn(&std::path::Path) -> Option<String>,
) -> Option<CodeAction> {
    if !plan.conflicts().is_empty() {
        return None;
    }
    let mut operations = Vec::new();
    for file in &plan.files {
        operations.extend(file_operations(file, encoding, read)?);
    }
    let names: Vec<String> = plan
        .files
        .iter()
        .filter_map(|file| file.path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    Some(CodeAction {
        title: format!("Migrate to rinx: write {}", names.join(", ")),
        kind: Some(CodeActionKind::new(MIGRATE_KIND)),
        edit: Some(WorkspaceEdit {
            document_changes: Some(DocumentChanges::Operations(operations)),
            ..WorkspaceEdit::default()
        }),
        ..CodeAction::default()
    })
}

/// The operations writing `file`: creating it and inserting its text, or
/// inserting at the end of the text it has.
fn file_operations(
    file: &PlannedFile,
    encoding: PositionEncoding,
    read: &dyn Fn(&std::path::Path) -> Option<String>,
) -> Option<Vec<DocumentChangeOperation>> {
    let uri = file_uri(&file.path)?;
    let insert = |at: Position, text: &str| {
        DocumentChangeOperation::Edit(TextDocumentEdit {
            text_document: OptionalVersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: None,
            },
            edits: vec![OneOf::Left(TextEdit {
                range: Range::new(at, at),
                new_text: text.to_string(),
            })],
        })
    };
    Some(match &file.content {
        PlannedContent::Create(text) => vec![
            DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
                uri: uri.clone(),
                options: Some(CreateFileOptions {
                    overwrite: Some(false),
                    ignore_if_exists: Some(false),
                }),
                annotation_id: None,
            })),
            insert(Position::new(0, 0), text),
        ],
        PlannedContent::Append(text) => {
            vec![insert(end_of(&read(&file.path)?, encoding), text)]
        }
    })
}

/// The position just past the last character of `text`.
fn end_of(text: &str, encoding: PositionEncoding) -> Position {
    let line = text.matches('\n').count();
    let last = text.rsplit('\n').next().unwrap_or_default();
    let past = rinx_ast::Position::new(
        u32::try_from(line + 1).unwrap_or(u32::MAX),
        u32::try_from(last.chars().count() + 1).unwrap_or(u32::MAX),
    );
    to_lsp_position(past, last, encoding)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planned(path: &str, content: PlannedContent) -> PlannedFile {
        PlannedFile {
            path: std::env::temp_dir()
                .join("rinx_migrate_action_never_written")
                .join(path),
            content,
        }
    }

    fn operations(action: &CodeAction) -> &[DocumentChangeOperation] {
        match action
            .edit
            .as_ref()
            .and_then(|edit| edit.document_changes.as_ref())
        {
            Some(DocumentChanges::Operations(operations)) => operations,
            other => panic!("operations, not {other:?}"),
        }
    }

    #[test]
    fn test_migrate_action_creates_each_new_file_and_appends_to_an_existing_one() {
        // Given
        let plan = MigrationPlan {
            files: vec![
                planned(
                    "BUILD.bazel",
                    PlannedContent::Create("load()\n".to_string()),
                ),
                planned("MODULE.bazel", PlannedContent::Append("dep\n".to_string())),
            ],
            findings: Vec::new(),
        };

        // When
        let action = migrate_action(&plan, PositionEncoding::Utf16, &|_| {
            Some("module()\n".to_string())
        })
        .expect("an action");

        // Then
        assert_eq!(action.kind, Some(CodeActionKind::new(MIGRATE_KIND)));
        assert_eq!(
            action.title,
            "Migrate to rinx: write BUILD.bazel, MODULE.bazel"
        );
        let [
            DocumentChangeOperation::Op(ResourceOp::Create(create)),
            DocumentChangeOperation::Edit(written),
            DocumentChangeOperation::Edit(appended),
        ] = operations(&action)
        else {
            panic!("create, write, append: {action:?}");
        };
        assert!(create.uri.as_str().ends_with("/BUILD.bazel"));
        assert_eq!(written.text_document.uri, create.uri);
        let OneOf::Left(edit) = &appended.edits[0] else {
            panic!("a text edit");
        };
        assert_eq!(edit.range.start, Position::new(1, 0));
        assert_eq!(edit.new_text, "dep\n");
    }

    #[test]
    fn test_migrate_action_is_not_offered_over_an_existing_file() {
        // Given — a file that exists
        let existing = std::env::temp_dir().join("rinx_migrate_action_existing");
        std::fs::write(&existing, "").unwrap();
        let plan = MigrationPlan {
            files: vec![PlannedFile {
                path: existing,
                content: PlannedContent::Create(String::new()),
            }],
            findings: Vec::new(),
        };

        // When / Then
        assert!(migrate_action(&plan, PositionEncoding::Utf16, &|_| None).is_none());
    }

    #[test]
    fn test_end_of_is_past_the_last_character_in_the_clients_units() {
        // When / Then
        assert_eq!(end_of("", PositionEncoding::Utf16), Position::new(0, 0));
        assert_eq!(
            end_of("a\nb😀", PositionEncoding::Utf16),
            Position::new(1, 3)
        );
        assert_eq!(
            end_of("a\nb😀", PositionEncoding::Utf32),
            Position::new(1, 2)
        );
        assert_eq!(end_of("a\n", PositionEncoding::Utf16), Position::new(1, 0));
    }
}
