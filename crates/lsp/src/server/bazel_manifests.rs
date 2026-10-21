//! Asking for the Bazel manifests of a workspace's sites (ADR-038 §2,
//! roadmap #18).
//!
//! A `rinx_site`'s `lsp_manifest` output group describes every library's
//! sources and parse settings; building it, and knowing where Bazel put it,
//! is the editor's, which already runs Bazel to find the binary. The server
//! never runs Bazel and never queries its graph.
//!
//! - **The VS Code extension builds.** A client passing the `bazelManifests`
//!   initialization option is asked with a `rinx/bazelManifests` request,
//!   once per Bazel workspace folder — one holding `MODULE.bazel` or a
//!   `WORKSPACE` — and answers with the manifests it built, or why there are
//!   none. When they may have changed — a `BUILD` file edited, the preferred
//!   site changed — it sends `rinx/bazelManifestsChanged`, and every folder
//!   is asked about again.
//! - **`rinx lsp --check --manifest <path>`** grants manifests for every
//!   folder, the folder being both the workspace and the execution root: a
//!   generated file is then found through the folder's `bazel-out` link.
//! - **Any other client** is never asked: its folders stay folders.
//!
//! An answer finds the folder's projects again; until it arrives, the folder
//! is read as it would be without Bazel.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lsp_server::{Message, Request, RequestId, Response};
use lsp_types::Uri;
use serde::{Deserialize, Serialize};

use super::handlers::show_warning;
use super::state::ServerState;
use crate::project::{
    BazelGrant, BazelRoots, DiscoveredProject, ProjectSource, read_bazel_projects,
};
use crate::uri::file_uri;

/// The request asking the client for a folder's Bazel manifests.
pub const BAZEL_MANIFESTS_METHOD: &str = "rinx/bazelManifests";

/// The notification saying the answers to `rinx/bazelManifests` may have
/// changed.
pub const BAZEL_MANIFESTS_CHANGED_METHOD: &str = "rinx/bazelManifestsChanged";

/// The files whose presence at a folder's root makes it a Bazel workspace.
const WORKSPACE_FILES: [&str; 3] = ["MODULE.bazel", "WORKSPACE", "WORKSPACE.bazel"];

/// The parameters of `rinx/bazelManifests`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BazelManifestsParams {
    /// The workspace folder the manifests are asked for.
    pub folder: Uri,
}

/// The answer to `rinx/bazelManifests`: the manifests the editor built, with
/// where their files are, or why there are none.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BazelManifestsResult {
    /// One `<site>.lsp_manifest.json` per site.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manifests: Vec<PathBuf>,
    /// The directory holding `MODULE.bazel`; the folder when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<PathBuf>,
    /// `bazel info execution_root`, under which generated files are; the
    /// workspace root when absent, through its `bazel-out` link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_root: Option<PathBuf>,
    /// The label of the site a document several sites list belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_site: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
}

impl BazelManifestsResult {
    /// What this answer grants for the folder at `folder`.
    fn into_grant(self, folder: &Path) -> Result<BazelGrant, String> {
        if let Some(reason) = self.unavailable {
            return Err(reason);
        }
        let workspace = self.workspace_root.unwrap_or_else(|| folder.to_path_buf());
        Ok(BazelGrant {
            roots: BazelRoots {
                execution: self.execution_root.unwrap_or_else(|| workspace.clone()),
                workspace,
            },
            manifests: self.manifests,
            preferred_site: self.preferred_site,
        })
    }
}

/// Where the server comes by a folder's manifests.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ManifestSource {
    /// Nowhere: every folder is read without Bazel.
    #[default]
    Off,
    /// The client answers for each Bazel folder, as one passing the
    /// `bazelManifests` initialization option does.
    AskClient,
    /// These manifests, for every folder, as `rinx lsp --check --manifest`
    /// says.
    Granted(Vec<PathBuf>),
}

/// The manifests the server has asked for and been told about.
#[derive(Debug, Default)]
pub struct BazelManifests {
    source: ManifestSource,
    /// What was answered for each folder, by its index.
    answers: BTreeMap<usize, Result<BazelGrant, String>>,
    /// The folder each unanswered request asks about.
    in_flight: BTreeMap<RequestId, usize>,
    /// Every finding the user has been shown, so a folder found again says
    /// nothing new.
    announced: BTreeSet<String>,
}

impl BazelManifests {
    /// Nothing asked yet, manifests coming from `source`.
    #[must_use]
    pub fn new(source: ManifestSource) -> Self {
        Self {
            source,
            ..Self::default()
        }
    }

    /// What was granted for folder `folder`, if anything.
    fn grant(&self, folder: usize) -> Option<&BazelGrant> {
        self.answers.get(&folder)?.as_ref().ok()
    }
}

/// Whether the folder at `root` is a Bazel workspace.
fn is_bazel_workspace(root: &Path) -> bool {
    WORKSPACE_FILES.iter().any(|name| root.join(name).is_file())
}

impl ServerState {
    /// The Bazel projects of folder `folder`, read from the manifests the
    /// client granted for it — none before an answer — and the user told,
    /// once, what reading them found.
    pub(super) fn bazel_projects_of(
        &mut self,
        folder: usize,
    ) -> (Vec<DiscoveredProject>, Vec<Message>) {
        let Some(grant) = self.bazel.grant(folder) else {
            return (Vec::new(), Vec::new());
        };
        let (projects, failures) = read_bazel_projects(grant);
        let mut findings: Vec<String> = failures
            .iter()
            .map(|failure| failure.message.clone())
            .collect();
        for project in &projects {
            let ProjectSource::Bazel { site, .. } = &project.model.source else {
                continue;
            };
            findings.extend(
                project
                    .findings
                    .iter()
                    .map(|finding| format!("{site}: {}", finding.message)),
            );
        }
        let messages = findings
            .into_iter()
            .filter(|finding| self.bazel.announced.insert(finding.clone()))
            .map(|finding| {
                show_warning(format!(
                    "rinx cannot read the Bazel project as the build does: {finding}"
                ))
            })
            .collect();
        (projects, messages)
    }

    /// Asks the client for the manifests of every Bazel folder it has not
    /// been asked about — or takes the granted ones, finding those folders'
    /// projects again — returning the requests and what finding them brings
    /// about.
    pub(super) fn ask_bazel_manifests(&mut self) -> Vec<Message> {
        let mut messages = Vec::new();
        let mut granted = Vec::new();
        for (folder, root) in self.folders.clone().iter().enumerate() {
            let bazel = &self.bazel;
            if bazel.answers.contains_key(&folder)
                || bazel.in_flight.values().any(|asked| *asked == folder)
            {
                continue;
            }
            match &bazel.source {
                ManifestSource::Off => {}
                ManifestSource::Granted(manifests) => {
                    let grant = BazelGrant {
                        roots: BazelRoots {
                            workspace: root.clone(),
                            execution: root.clone(),
                        },
                        manifests: manifests.clone(),
                        preferred_site: None,
                    };
                    self.bazel.answers.insert(folder, Ok(grant));
                    granted.push(folder);
                }
                ManifestSource::AskClient => {
                    if !is_bazel_workspace(root) {
                        continue;
                    }
                    let Some(uri) = file_uri(root) else {
                        continue;
                    };
                    let id = self.next_request_id();
                    self.bazel.in_flight.insert(id.clone(), folder);
                    let params = BazelManifestsParams { folder: uri };
                    messages
                        .push(Request::new(id, BAZEL_MANIFESTS_METHOD.to_string(), params).into());
                }
            }
        }
        for folder in granted {
            messages.extend(self.reload_folder(folder));
        }
        messages
    }

    /// The messages the answer `response` to a `rinx/bazelManifests` brings
    /// about — or `None` when it answers some other request. An error, or an
    /// answer that does not parse, leaves the folder without Bazel.
    pub(super) fn on_bazel_manifests_response(
        &mut self,
        response: &Response,
    ) -> Option<Vec<Message>> {
        let folder = self.bazel.in_flight.remove(&response.id)?;
        let root = self.folders.get(folder)?.clone();
        let grant = match &response.response_result {
            Err(error) => Err(error.message.clone()),
            Ok(result) => serde_json::from_value::<BazelManifestsResult>(result.clone())
                .map_err(|error| format!("the editor's answer did not parse: {error}"))
                .and_then(|result| result.into_grant(&root)),
        };
        let was_granted = self.bazel.grant(folder).is_some();
        let now_granted = grant.is_ok();
        self.bazel.answers.insert(folder, grant);
        // A folder that had no Bazel project and still has none is
        // unchanged; anything else finds its projects again.
        Some(if was_granted || now_granted {
            self.reload_folder(folder)
        } else {
            Vec::new()
        })
    }

    /// The messages `rinx/bazelManifestsChanged` brings about: every answer
    /// forgotten and the client asked again. The projects stay as they are
    /// until the new answers arrive, so a rebuild does not flicker.
    pub(super) fn on_bazel_manifests_changed(&mut self) -> Vec<Message> {
        if self.bazel.source != ManifestSource::AskClient {
            return Vec::new();
        }
        self.bazel.answers.clear();
        self.bazel.in_flight.clear();
        self.ask_bazel_manifests()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_params_are_spelled_as_the_extension_reads_them() {
        // Given
        let params = BazelManifestsParams {
            folder: "file:///work".parse().expect("a URI"),
        };

        // When / Then
        assert_eq!(
            serde_json::to_value(params).expect("serializes"),
            serde_json::json!({"folder": "file:///work"})
        );
    }

    #[test]
    fn test_an_answer_grants_its_manifests_under_its_roots() {
        // Given
        let answer: BazelManifestsResult = serde_json::from_value(serde_json::json!({
            "manifests": ["/out/docs/site.lsp_manifest.json"],
            "workspaceRoot": "/work",
            "executionRoot": "/cache/execroot/_main",
            "preferredSite": "//docs:site",
        }))
        .expect("parses");

        // When
        let grant = answer.into_grant(Path::new("/work/sub")).expect("a grant");

        // Then
        assert_eq!(
            grant,
            BazelGrant {
                roots: BazelRoots {
                    workspace: PathBuf::from("/work"),
                    execution: PathBuf::from("/cache/execroot/_main"),
                },
                manifests: vec![PathBuf::from("/out/docs/site.lsp_manifest.json")],
                preferred_site: Some("//docs:site".to_string()),
            }
        );
    }

    #[test]
    fn test_an_answer_without_roots_takes_the_folder() {
        // Given
        let answer = BazelManifestsResult {
            manifests: vec![PathBuf::from("/m.json")],
            ..BazelManifestsResult::default()
        };

        // When
        let grant = answer.into_grant(Path::new("/work")).expect("a grant");

        // Then
        assert_eq!(grant.roots.workspace, PathBuf::from("/work"));
        assert_eq!(grant.roots.execution, PathBuf::from("/work"));
    }

    #[test]
    fn test_an_unavailable_answer_grants_nothing() {
        // Given
        let answer = BazelManifestsResult {
            unavailable: Some("bazel failed".to_string()),
            ..BazelManifestsResult::default()
        };

        // When / Then
        assert_eq!(
            answer.into_grant(Path::new("/work")),
            Err("bazel failed".to_string())
        );
    }

    #[test]
    fn test_is_bazel_workspace_looks_for_a_module_or_workspace_file() {
        // Given
        let root = std::env::temp_dir().join("rinx_lsp_is_bazel_workspace");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("plain")).expect("mkdir");
        std::fs::create_dir_all(root.join("module")).expect("mkdir");
        std::fs::create_dir_all(root.join("legacy")).expect("mkdir");
        std::fs::write(root.join("module/MODULE.bazel"), "").expect("write");
        std::fs::write(root.join("legacy/WORKSPACE"), "").expect("write");

        // When / Then
        assert!(!is_bazel_workspace(&root.join("plain")));
        assert!(is_bazel_workspace(&root.join("module")));
        assert!(is_bazel_workspace(&root.join("legacy")));
    }
}
