//! Renaming a label, an entity id or an equation label across its project:
//! `textDocument/prepareRename` and `textDocument/rename` (roadmap #31).
//!
//! What the cursor names is what find-references asks ([`ServerState::
//! use_targets_at`]), and the places rewritten are its definition's name and
//! every use [`crate::project::Project::uses_of`] lists, each narrowed to the
//! name's characters by [`crate::rename`]. Every place is read and checked
//! before an edit is offered, so a rename either rewrites all of them or is
//! refused with the reason; there is no partial rename.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use lsp_types::{
    DocumentChangeOperation, DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier,
    PrepareRenameResponse, TextDocumentEdit, TextEdit, Uri, WorkspaceEdit,
};
use rinx_ast::{Position, Span, TargetName};
use rinx_index::ProjectIndex;

use super::projects::ProjectId;
use super::state::{ServerState, file_text};
use crate::position::to_lsp_range;
use crate::project::UseForm;
use crate::rename::{NamePlace, RenameRefusal, Renamed, definition_name_place, use_name_place};
use crate::uri::file_uri;

/// How a client takes a workspace edit: as versioned document changes, when
/// it announces `workspace.workspaceEdit.documentChanges`, else by URI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditShape {
    Versioned,
    ByUri,
}

/// Every place a rename would rewrite, found and checked.
struct RenamePlan {
    renamed: Renamed,
    project: ProjectId,
    /// Each place, with the whole construct it stands in, for finding the
    /// one under the cursor.
    places: Vec<Placed>,
}

/// One place a name stands, in a file.
struct Placed {
    path: PathBuf,
    place: NamePlace,
    /// The construct the name stands in: the use, or the definition.
    around: (Position, Position),
}

impl ServerState {
    /// The name under `position` in the open document `uri` and how it is
    /// written there, when a rename of what it names would succeed short of
    /// a new name — `None` where the cursor names nothing.
    ///
    /// # Errors
    ///
    /// Why what the cursor names cannot be renamed.
    pub(super) fn prepare_rename(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Result<Option<PrepareRenameResponse>, RenameRefusal> {
        let Some(plan) = self.rename_plan(uri, position)? else {
            return Ok(None);
        };
        let (Some(path), Some(at)) = (crate::uri::file_path(uri), self.position_in(uri, position))
        else {
            return Ok(None);
        };
        let under = plan
            .places
            .iter()
            .filter(|placed| placed.path == path)
            .find(|placed| placed.around.0 <= at && at < placed.around.1);
        let Some(under) = under else {
            return Ok(None);
        };
        let Some(text) = file_text(&self.documents, &under.path) else {
            return Ok(None);
        };
        let span = Span::new(under.place.start, under.place.end);
        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: to_lsp_range(span, &text, self.encoding),
            placeholder: under.place.written.clone(),
        }))
    }

    /// The edit renaming what `position` in the open document `uri` names to
    /// `new_name`, in every file that writes it — `None` where the cursor
    /// names nothing.
    ///
    /// # Errors
    ///
    /// Why it cannot be renamed, or not to `new_name`.
    pub(super) fn rename(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
        new_name: &str,
    ) -> Result<Option<WorkspaceEdit>, RenameRefusal> {
        let Some(plan) = self.rename_plan(uri, position)? else {
            return Ok(None);
        };
        let (new_name, new) = plan.renamed.check_new_name(new_name)?;
        let Some(project) = self.projects.get_mut(&plan.project) else {
            return Ok(None);
        };
        let schema = std::sync::Arc::clone(&project.model().settings.parse.schema);
        let index = project.project_index();
        check_free(index, &plan.renamed, &new, &new_name)?;
        if let (Renamed::Entity(old), Renamed::Entity(new)) = (&plan.renamed, &new)
            && let Some(entity_type) = index
                .entities
                .get(old)
                .and_then(|record| schema.entity_type(&record.type_name))
        {
            entity_type
                .id
                .check_pattern(new)
                .map_err(|mismatch| RenameRefusal::IdPattern {
                    id: mismatch.id,
                    pattern: mismatch.pattern,
                })?;
        }
        let mut edits: BTreeMap<PathBuf, Vec<TextEdit>> = BTreeMap::new();
        for placed in &plan.places {
            let Some(text) = file_text(&self.documents, &placed.path) else {
                return Err(RenameRefusal::Unreadable {
                    path: placed.path.clone(),
                });
            };
            let span = Span::new(placed.place.start, placed.place.end);
            edits
                .entry(placed.path.clone())
                .or_default()
                .push(TextEdit::new(
                    to_lsp_range(span, &text, self.encoding),
                    placed.place.replacement(&new_name),
                ));
        }
        Ok(Some(self.workspace_edit(edits)))
    }

    /// Every place renaming what `position` in `uri` names would rewrite,
    /// each read and checked against its text — `None` where the cursor
    /// names nothing.
    fn rename_plan(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Result<Option<RenamePlan>, RenameRefusal> {
        let Some((targets, project_id)) = self.use_targets_at(uri, position) else {
            return Ok(None);
        };
        // A `:glob:` entry leads to several documents, all of them documents.
        let [target] = targets.as_slice() else {
            return Err(RenameRefusal::Document);
        };
        let renamed = Renamed::of(target.definition.as_ref())?;
        let Some(project) = self.projects.get_mut(&project_id) else {
            return Ok(None);
        };
        if is_contested(&project.project_index().ambiguous_definitions, &renamed) {
            return Err(RenameRefusal::Ambiguous {
                name: renamed.name().to_string(),
            });
        }
        let site = project
            .definitions()
            .site(&target.doc_path, &renamed.key())
            .ok_or_else(|| RenameRefusal::UnknownSite {
                name: renamed.name().to_string(),
            })?;
        let Some(name) = site.name else {
            return Err(match &renamed {
                Renamed::Entity(id) => RenameRefusal::UnwrittenId {
                    id: id.as_str().to_string(),
                },
                Renamed::Label(_) | Renamed::Equation(_) => RenameRefusal::UnknownSite {
                    name: renamed.name().to_string(),
                },
            });
        };
        let mut texts: HashMap<PathBuf, Option<String>> = HashMap::new();
        let mut text_of = |path: &PathBuf| {
            texts
                .entry(path.clone())
                .or_insert_with(|| file_text(&self.documents, path).map(Cow::into_owned))
                .clone()
                .ok_or_else(|| RenameRefusal::Unreadable { path: path.clone() })
        };
        let definition = text_of(&site.path)?;
        let mut places = vec![checked(
            &renamed,
            site.path.clone(),
            definition_name_place(&definition, name),
            (site.start, site.end),
            name.start.line,
        )?];
        for use_site in project.uses_of(target) {
            let Some(path) = use_site.path else {
                return Err(RenameRefusal::InPrologOrEpilog {
                    name: renamed.name().to_string(),
                });
            };
            // A reference reaching the definition through an alias writes the
            // alias's name, which stays: the alias's own line is the use.
            if let UseForm::Named(written) = &use_site.form
                && !renamed.is_named_by(written.as_str())
            {
                continue;
            }
            let text = text_of(&path)?;
            let place = use_name_place(&use_site.form, &text, use_site.start, use_site.end);
            places.push(checked(
                &renamed,
                path,
                place,
                (use_site.start, use_site.end),
                use_site.start.line,
            )?);
        }
        if !self.folders.is_empty()
            && let Some(outside) = places.iter().find(|placed| {
                !self
                    .folders
                    .iter()
                    .any(|root| placed.path.starts_with(root))
            })
        {
            return Err(RenameRefusal::OutsideWorkspace {
                path: outside.path.clone(),
            });
        }
        // A fragment two documents include holds each of its uses twice.
        places.sort_by_key(|placed| (placed.path.clone(), placed.place.start));
        places.dedup_by(|a, b| a.path == b.path && a.place == b.place);
        Ok(Some(RenamePlan {
            renamed,
            project: project_id,
            places,
        }))
    }

    /// `edits` as the client takes a workspace edit: versioned per document,
    /// or by URI.
    pub(super) fn workspace_edit(&self, edits: BTreeMap<PathBuf, Vec<TextEdit>>) -> WorkspaceEdit {
        let by_uri = edits
            .into_iter()
            .filter_map(|(path, edits)| Some((path.clone(), file_uri(&path)?, edits)));
        if self.edit_shape == EditShape::ByUri {
            return WorkspaceEdit {
                changes: Some(by_uri.map(|(_, uri, edits)| (uri, edits)).collect()),
                ..WorkspaceEdit::default()
            };
        }
        let operations = by_uri
            .map(|(path, uri, edits)| {
                DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier {
                        uri,
                        version: self.documents.get_by_path(&path).map(|open| open.version),
                    },
                    edits: edits.into_iter().map(OneOf::Left).collect(),
                })
            })
            .collect();
        WorkspaceEdit {
            document_changes: Some(DocumentChanges::Operations(operations)),
            ..WorkspaceEdit::default()
        }
    }
}

/// `place`, found in the file at `path` for a construct spanning `around`
/// that starts on `line`, when it names `renamed`.
fn checked(
    renamed: &Renamed,
    path: PathBuf,
    place: Option<NamePlace>,
    around: (Position, Position),
    line: u32,
) -> Result<Placed, RenameRefusal> {
    match place {
        Some(place) if renamed.is_named_by(&place.written) => Ok(Placed {
            path,
            place,
            around,
        }),
        _ => Err(RenameRefusal::Stale {
            path,
            line,
            name: renamed.name().to_string(),
        }),
    }
}

/// Whether two documents define `renamed`, which therefore leads nowhere.
fn is_contested(ambiguous: &rinx_index::AmbiguousDefinitions, renamed: &Renamed) -> bool {
    match renamed {
        Renamed::Label(name) => ambiguous.targets.contains_key(name),
        Renamed::Equation(name) => ambiguous.equations.contains_key(name),
        Renamed::Entity(id) => {
            ambiguous.entities.contains_key(id)
                || ambiguous
                    .targets
                    .contains_key(&TargetName::new(id.as_str()))
        }
    }
}

/// Whether `new`, the definition renaming `old` to `new_name` gives, is free
/// in `index`: no other definition of its kind, and for an entity no label
/// of its name, which every entity also is. The same definition spelled
/// differently — a label's case — is the definition itself.
fn check_free(
    index: &ProjectIndex,
    old: &Renamed,
    new: &Renamed,
    new_name: &str,
) -> Result<(), RenameRefusal> {
    if new == old {
        return Ok(());
    }
    let ambiguous = &index.ambiguous_definitions;
    let taken = match new {
        Renamed::Label(name) => {
            index.targets.contains_key(name) || ambiguous.targets.contains_key(name)
        }
        Renamed::Equation(name) => {
            index.equations.contains_key(name) || ambiguous.equations.contains_key(name)
        }
        Renamed::Entity(id) => {
            let label = TargetName::new(id.as_str());
            let own_label =
                matches!(old, Renamed::Entity(old) if TargetName::new(old.as_str()) == label);
            index.entities.contains_key(id)
                || ambiguous.entities.contains_key(id)
                || (!own_label && index.targets.contains_key(&label))
        }
    };
    if taken {
        return Err(RenameRefusal::Taken {
            name: new_name.to_string(),
        });
    }
    Ok(())
}
