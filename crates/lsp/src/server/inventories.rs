//! Getting the projects' intersphinx inventories in (ADR-038 §6).
//!
//! The server never touches the network. An inventory named by an address
//! is the editor's to download, with the user's consent, into a cache of its
//! own; the server asks for it with a `rinx/fetchInventory` request and reads
//! the file the answer names. Only a client announcing it — the
//! `fetchInventories` initialization option the VS Code extension passes —
//! is asked, so another editor never sees a request it does not know, and
//! its references into an inventory stay hints.
//!
//! An address is asked for once per session, however many projects declare
//! it, and its outcome is remembered: a project found again — a `conf.py`
//! edited — does not ask the user a second time.

use std::collections::BTreeMap;
use std::path::PathBuf;

use lsp_server::{Message, Request, RequestId, Response};
use serde::{Deserialize, Serialize};

use super::projects::ProjectId;
use super::state::ServerState;
use crate::project::Fetched;
use crate::uri::file_uri;

/// The request asking the client to download an inventory.
pub const FETCH_INVENTORY_METHOD: &str = "rinx/fetchInventory";

/// Why a download is unavailable when the client was never asked.
pub const NOT_FETCHED_BY_CLIENT: &str = "this editor does not download inventories";

/// The parameters of `rinx/fetchInventory`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchInventoryParams {
    /// The address of the `objects.inv` to download.
    pub url: String,
    /// How many days a cached copy stays fresh; negative for ever — the
    /// project's `intersphinx_cache_limit`.
    pub cache_limit_days: i64,
}

/// The answer to `rinx/fetchInventory`: the file the inventory was saved to,
/// or why there is none.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchInventoryResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
}

impl FetchInventoryResult {
    /// What became of the download, as a project takes it.
    fn into_fetched(self) -> Fetched {
        match (self.path, self.unavailable) {
            (Some(path), _) => Ok(path),
            (None, Some(reason)) => Err(reason),
            (None, None) => Err("the editor named no file".to_string()),
        }
    }
}

/// The downloads the server has asked the client for.
#[derive(Debug, Default)]
pub struct InventoryFetches {
    /// Whether the client downloads inventories at all.
    enabled: bool,
    /// What became of each address asked for.
    outcomes: BTreeMap<String, Fetched>,
    /// The address each request still unanswered asks for.
    in_flight: BTreeMap<RequestId, String>,
}

impl InventoryFetches {
    /// The downloads of a client that does them when `enabled`.
    #[must_use]
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    /// What became of the download of `url`, `None` while it is unknown.
    fn outcome(&self, url: &str) -> Option<Fetched> {
        if !self.enabled {
            return Some(Err(NOT_FETCHED_BY_CLIENT.to_string()));
        }
        self.outcomes.get(url).cloned()
    }

    /// Whether `url` has been asked for already.
    fn asked(&self, url: &str) -> bool {
        self.outcomes.contains_key(url) || self.in_flight.values().any(|asked| asked == url)
    }
}

impl ServerState {
    /// Loads what of the inventories of the projects `ids` can be loaded
    /// now, and asks the client for each download they wait for that was not
    /// asked for yet — returning those requests, and the `conf.py` of every
    /// project whose inventories changed published again. Its documents
    /// render again on their own, the project's index having changed.
    pub(super) fn load_inventories(&mut self, ids: &[ProjectId]) -> Vec<Message> {
        let mut messages = Vec::new();
        let mut wanted: Vec<(String, i64)> = Vec::new();
        for id in ids {
            let Some(project) = self.projects.get_mut(id) else {
                continue;
            };
            let fetches = &self.fetches;
            let changed = project.advance_inventories(&|url: &str| fetches.outcome(url));
            for url in project.awaited_inventories() {
                if !fetches.asked(&url) && !wanted.iter().any(|(asked, _)| *asked == url) {
                    wanted.push((url, project.inventory_cache_limit_days()));
                }
            }
            if changed && let Some(conf) = project.conf_path().and_then(file_uri) {
                messages.extend(self.publish(conf, false));
            }
        }
        for (url, cache_limit_days) in wanted {
            let id = self.next_request_id();
            self.fetches.in_flight.insert(id.clone(), url.clone());
            let params = FetchInventoryParams {
                url,
                cache_limit_days,
            };
            messages.push(Request::new(id, FETCH_INVENTORY_METHOD.to_string(), params).into());
        }
        messages
    }

    /// The messages the answer `response` to a `rinx/fetchInventory` brings
    /// about — or `None` when it answers some other request. An error, or an
    /// answer that does not parse, makes the download unavailable.
    pub(super) fn on_inventory_response(&mut self, response: &Response) -> Option<Vec<Message>> {
        let url = self.fetches.in_flight.remove(&response.id)?;
        let fetched = match &response.response_result {
            Err(error) => Err(error.message.clone()),
            Ok(result) => serde_json::from_value::<FetchInventoryResult>(result.clone())
                .map_err(|error| format!("the editor's answer did not parse: {error}"))
                .and_then(FetchInventoryResult::into_fetched),
        };
        self.fetches.outcomes.insert(url, fetched);
        let waiting: Vec<ProjectId> = self.projects.keys().copied().collect();
        Some(self.load_inventories(&waiting))
    }

    /// Reads every inventory read from a file at one of `paths` again, as
    /// they changed on the disk, returning the messages that brings about.
    pub(super) fn reload_inventory_files(&mut self, paths: &[PathBuf]) -> Vec<Message> {
        let reloaded: Vec<ProjectId> = self
            .projects
            .iter_mut()
            .filter_map(|(id, project)| {
                let mut reloaded = false;
                for path in paths {
                    reloaded |= project.reload_inventory_file(path);
                }
                reloaded.then_some(*id)
            })
            .collect();
        self.load_inventories(&reloaded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_params_are_spelled_as_the_extension_reads_them() {
        // Given
        let params = FetchInventoryParams {
            url: "https://a.org/objects.inv".to_string(),
            cache_limit_days: -1,
        };

        // When / Then
        assert_eq!(
            serde_json::to_value(&params).unwrap(),
            serde_json::json!({"url": "https://a.org/objects.inv", "cacheLimitDays": -1})
        );
    }

    #[test]
    fn test_a_result_names_a_file_or_why_there_is_none() {
        // Given
        let parse = |json: serde_json::Value| {
            serde_json::from_value::<FetchInventoryResult>(json)
                .unwrap()
                .into_fetched()
        };

        // When / Then
        assert_eq!(
            parse(serde_json::json!({"path": "/cache/a.inv"})),
            Ok(PathBuf::from("/cache/a.inv"))
        );
        assert_eq!(
            parse(serde_json::json!({"unavailable": "declined"})),
            Err("declined".to_string())
        );
        assert!(parse(serde_json::json!({})).is_err());
    }

    #[test]
    fn test_a_client_that_does_not_download_has_every_download_unavailable() {
        // Given
        let fetches = InventoryFetches::new(false);

        // When / Then
        assert_eq!(
            fetches.outcome("https://a.org/objects.inv"),
            Some(Err(NOT_FETCHED_BY_CLIENT.to_string()))
        );
        assert_eq!(InventoryFetches::new(true).outcome("https://a.org"), None);
    }
}
