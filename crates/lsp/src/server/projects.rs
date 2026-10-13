//! The server's projects: which one a file belongs to, how a folder's
//! projects are replaced when the scan — or a changed `conf.py` — finds them
//! again, and what they show the client: their `conf.py`'s findings, the
//! status bar's list, and — once — the extensions the server cannot model.
//!
//! A file belongs to the project whose source root is the nearest above it,
//! and only that project is asked whether the file is a document — an
//! excluded file is no document of an outer project either (ADR-038 §2).
//!
//! Replacing a folder's projects keeps every project whose model is
//! unchanged, with what it has recorded, so a scan confirming the folder's
//! provisional project costs nothing. A new project is given an id no project
//! had before, and every document the graph diagnoses under the folder is
//! read again, so none stays recorded in a project the folder no longer has.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use lsp_server::Message;
use lsp_types::{FileChangeType, FileEvent, Uri};

use super::handlers::show_information;
use super::state::ServerState;
use crate::diagnostics::conf_diagnostics;
use crate::progress::{IndexState, IndexStatus, ProjectStatus};
use crate::project::{
    DiscoveredProject, Project, ProjectSource, contains_conf, discover_projects, is_conf,
    is_visible_under, scan_projects,
};
use crate::uri::{file_path, file_uri};

/// One project, as long as the server keeps it: a project found again with
/// another model gets another id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectId(u64);

/// What replacing a folder's projects changed.
struct Replaced {
    /// The projects the folder did not have the same way before.
    added: Vec<(ProjectId, DiscoveredProject)>,
    /// The projects it kept, as found now: a nested project come or gone
    /// changes which documents are theirs.
    kept: Vec<(ProjectId, DiscoveredProject)>,
    /// Whether any project was added or removed.
    changed: bool,
    /// Every `conf.py` of the folder's projects, before or after.
    confs: BTreeSet<Uri>,
}

impl ServerState {
    /// Adds the project `discovered` in folder `folder`, under a new id.
    pub(super) fn add_project(
        &mut self,
        folder: usize,
        discovered: DiscoveredProject,
    ) -> ProjectId {
        let id = ProjectId(self.next_project);
        self.next_project += 1;
        self.projects.insert(id, Project::new(folder, discovered));
        id
    }

    /// Makes `discovered` folder `folder`'s projects, as the scan found them,
    /// and returns the messages that brings about. Their documents arrive
    /// with the scan's result.
    pub(super) fn install_projects(
        &mut self,
        folder: usize,
        discovered: Vec<DiscoveredProject>,
    ) -> Vec<Message> {
        let replaced = self.replace_projects(folder, discovered);
        self.after_replacing(folder, &replaced)
    }

    /// Finds folder `folder`'s projects again, as its configuration changed,
    /// scanning the documents of each new one at once, and returns the
    /// messages that brings about — the ready status among them, when no
    /// scan is running.
    pub(super) fn reload_folder(&mut self, folder: usize) -> Vec<Message> {
        let Some(root) = self.folders.get(folder).cloned() else {
            return Vec::new();
        };
        let replaced = self.replace_projects(folder, discover_projects(&root));
        // A new project's documents, and those a kept one gained, are read
        // now; nothing else will.
        let mut ids = Vec::new();
        let mut unread = Vec::new();
        for (id, discovered) in replaced.added.iter().chain(&replaced.kept) {
            let Some(project) = self.projects.get(id) else {
                continue;
            };
            let recorded: BTreeSet<&str> =
                project.documents().map(|(doc_path, _)| doc_path).collect();
            let mut missing = discovered.clone();
            missing
                .sources
                .retain(|(doc_path, _)| !recorded.contains(doc_path.as_str()));
            ids.push(*id);
            unread.push(missing);
        }
        for (id, scanned) in ids.into_iter().zip(scan_projects(unread, &|_, _| {})) {
            if let Some(project) = self.projects.get_mut(&id) {
                project.record_scanned(scanned.documents);
            }
        }
        let mut messages = self.after_replacing(folder, &replaced);
        if self.scan.is_none() {
            messages.extend(self.diagnose_includers_of_open_documents());
            let status = self.status(IndexState::Ready, self.scan_elapsed);
            messages.push(status.notification());
        }
        messages
    }

    /// Replaces folder `folder`'s projects with `discovered`, keeping each
    /// one whose model is unchanged.
    fn replace_projects(&mut self, folder: usize, discovered: Vec<DiscoveredProject>) -> Replaced {
        self.discovered.insert(folder);
        let mut confs = self.conf_uris_of(folder);
        let mut kept = BTreeSet::new();
        let mut added = Vec::new();
        let mut unchanged = Vec::new();
        for project in discovered {
            let same = self
                .projects
                .iter()
                .find(|(id, known)| {
                    known.folder() == folder
                        && !kept.contains(*id)
                        && known.model() == &project.model
                })
                .map(|(id, _)| *id);
            let id = if let Some(id) = same {
                if let Some(known) = self.projects.get_mut(&id) {
                    known.set_conf_findings(project.findings.clone(), project.conf_text.clone());
                }
                unchanged.push((id, project));
                id
            } else {
                let id = self.add_project(folder, project.clone());
                added.push((id, project));
                id
            };
            kept.insert(id);
        }
        let before = self.projects.len();
        self.projects
            .retain(|id, project| project.folder() != folder || kept.contains(id));
        let mut changed = !added.is_empty() || self.projects.len() != before;
        for (id, _) in &unchanged {
            changed |= self.forget_documents_elsewhere(*id);
        }
        confs.extend(self.conf_uris_of(folder));
        Replaced {
            added,
            kept: unchanged,
            changed,
            confs,
        }
    }

    /// Forgets every document project `id` has recorded that is no longer
    /// its own — a nested project's now, or excluded — returning whether
    /// there was one.
    fn forget_documents_elsewhere(&mut self, id: ProjectId) -> bool {
        let Some(project) = self.projects.get(&id) else {
            return false;
        };
        let elsewhere: Vec<String> = project
            .documents()
            .filter(|(doc_path, path)| {
                self.locate(path)
                    .is_none_or(|(owner, name)| owner != id || name != *doc_path)
            })
            .map(|(doc_path, _)| doc_path.to_string())
            .collect();
        if let Some(project) = self.projects.get_mut(&id) {
            for doc_path in &elsewhere {
                project.forget(doc_path);
            }
        }
        !elsewhere.is_empty()
    }

    /// The messages replacing a folder's projects brings about: the notice
    /// of each new project's unmodelled extensions, every document diagnosed
    /// under the folder read again, if a project changed, and every `conf.py`
    /// published.
    fn after_replacing(&mut self, folder: usize, replaced: &Replaced) -> Vec<Message> {
        let mut messages: Vec<Message> = replaced
            .added
            .iter()
            .filter_map(|(id, _)| self.announce_unmodelled_extensions(*id))
            .collect();
        if replaced.changed
            && let Some(root) = self.folders.get(folder).cloned()
        {
            let mut under: BTreeSet<Uri> = self.tracked_under(&root).into_iter().collect();
            under.extend(
                self.open_paths_under(&root)
                    .iter()
                    .filter_map(|path| self.uri_of_path(path)),
            );
            messages.extend(self.rediagnose_and_publish(&under));
        }
        for conf in &replaced.confs {
            messages.extend(self.publish(conf.clone(), false));
        }
        messages
    }

    /// The notice naming the extensions project `id` declares that the
    /// server cannot model, unless there are none or the user was told about
    /// exactly these for its `conf.py` already (ADR-038 §5).
    fn announce_unmodelled_extensions(&mut self, id: ProjectId) -> Option<Message> {
        let project = self.projects.get(&id)?;
        let conf = project.conf_path()?.to_path_buf();
        let unmodelled = project.model().settings.strictness.unmodelled_extensions();
        if unmodelled.is_empty() {
            return None;
        }
        let names: Vec<String> = unmodelled.iter().map(|name| format!("`{name}`")).collect();
        let message = format!(
            "rinx does not know the extensions {} that {} declares. Their directives are \
             reported as unknown, and a reference into any domain that does not resolve as a hint, \
             since one of them may define it.",
            names.join(", "),
            self.folders.get(project.folder()).map_or_else(
                || conf.to_string_lossy().into_owned(),
                |root| { relative_name(root, &conf) }
            ),
        );
        let unmodelled = unmodelled.to_vec();
        self.announced
            .insert((conf, unmodelled))
            .then(|| show_information(message))
    }

    /// The URI of every `conf.py` of folder `folder`'s projects.
    fn conf_uris_of(&self, folder: usize) -> BTreeSet<Uri> {
        self.projects
            .values()
            .filter(|project| project.folder() == folder)
            .filter_map(|project| file_uri(project.conf_path()?))
            .collect()
    }

    /// What the `conf.py` at `uri` shows: what reading it found, when it
    /// configures a project.
    pub(super) fn conf_diagnostics_for(&self, uri: &Uri) -> Vec<lsp_types::Diagnostic> {
        let Some(path) = file_path(uri) else {
            return Vec::new();
        };
        self.projects
            .values()
            .filter(|project| project.conf_path() == Some(path.as_path()))
            .flat_map(|project| {
                let (findings, text) = project.conf_findings();
                conf_diagnostics(findings, text, self.encoding)
            })
            .collect()
    }

    /// The folder whose projects `event` may change: one naming a `conf.py`
    /// a project has or the walk would find, a directory created with one
    /// inside, or a directory deleted holding a project.
    pub(super) fn folder_reconfigured_by(&self, event: &FileEvent) -> Option<usize> {
        let path = file_path(&event.uri)?;
        let folder = self
            .folders
            .iter()
            .enumerate()
            .filter(|(_, root)| is_visible_under(root, &path))
            .max_by_key(|(_, root)| root.components().count())
            .map(|(folder, _)| folder)?;
        let configures = self
            .projects
            .values()
            .any(|project| project.conf_path() == Some(path.as_path()));
        let entered = |directory: &Path| {
            self.project_holding(directory)
                .is_none_or(|project| project.model().enters(directory))
        };
        let reconfigured = configures
            || match event.typ {
                FileChangeType::DELETED => self
                    .projects
                    .values()
                    .any(|project| project.folder() == folder && project.root().starts_with(&path)),
                _ if is_conf(&path) => path.parent().is_some_and(entered),
                FileChangeType::CREATED => path.is_dir() && entered(&path) && contains_conf(&path),
                _ => false,
            };
        reconfigured.then_some(folder)
    }

    /// The project whose source root is the nearest at or above `path`.
    pub(super) fn project_holding(&self, path: &Path) -> Option<&Project> {
        self.holding(path).map(|(_, project)| project)
    }

    fn holding(&self, path: &Path) -> Option<(ProjectId, &Project)> {
        self.projects
            .iter()
            .filter(|(_, project)| path.starts_with(project.root()))
            .max_by_key(|(_, project)| project.root().components().count())
            .map(|(id, project)| (*id, project))
    }

    /// The project holding the document at `path`, and the document's name
    /// in it — or `None` when `path` is no document of the project nearest
    /// above it.
    pub(super) fn locate(&self, path: &Path) -> Option<(ProjectId, String)> {
        let (id, project) = self.holding(path)?;
        Some((id, project.doc_path_of(path)?))
    }

    /// Folder `folder`'s project with the model `discovered` has.
    pub(super) fn project_with_model(
        &mut self,
        folder: usize,
        discovered: &DiscoveredProject,
    ) -> Option<&mut Project> {
        self.projects
            .values_mut()
            .find(|project| project.folder() == folder && project.model() == &discovered.model)
    }

    /// The documents indexed, over every project.
    pub(super) fn document_count(&self) -> usize {
        self.projects.values().map(Project::document_count).sum()
    }

    /// The status of the workspace index, `state` and `elapsed` given.
    pub(super) fn status(&self, state: IndexState, elapsed: Option<Duration>) -> IndexStatus {
        IndexStatus {
            state,
            documents: self.document_count(),
            elapsed_ms: elapsed
                .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)),
            projects: self.project_statuses(),
        }
    }

    /// Every Sphinx project, and each folder with documents of its own, in
    /// the order of their roots.
    fn project_statuses(&self) -> Vec<ProjectStatus> {
        let mut projects: Vec<&Project> = self.projects.values().collect();
        projects.sort_by(|a, b| a.root().cmp(b.root()));
        projects
            .into_iter()
            .filter_map(|project| {
                let root = self.folders.get(project.folder())?;
                match &project.model().source {
                    ProjectSource::SphinxConf { conf } => Some(ProjectStatus::Sphinx {
                        conf: relative_name(root, conf),
                    }),
                    ProjectSource::Folder if project.document_count() > 0 => {
                        Some(ProjectStatus::Folder {
                            root: root.file_name().map_or_else(String::new, |name| {
                                name.to_string_lossy().into_owned()
                            }),
                        })
                    }
                    ProjectSource::Folder => None,
                }
            })
            .collect()
    }
}

/// `path` relative to `root`, `/`-separated.
fn relative_name(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) => relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_relative_name_names_a_path_within_its_folder() {
        // When / Then
        assert_eq!(
            relative_name(Path::new("/work"), Path::new("/work/docs/conf.py")),
            "docs/conf.py"
        );
        assert_eq!(
            relative_name(Path::new("/work"), Path::new("/elsewhere/conf.py")),
            "/elsewhere/conf.py"
        );
        assert_eq!(
            relative_name(&PathBuf::from("/w"), Path::new("/w/conf.py")),
            "conf.py"
        );
    }
}
