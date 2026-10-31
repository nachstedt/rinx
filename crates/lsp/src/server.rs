//! The protocol loop, and the handlers it dispatches to.
//!
//! Split the way every subcommand is (`process_*`/`cmd_*`): the handlers are
//! pure functions from the server's state and one incoming message to the
//! messages to send back, and [`run`] is the thin loop that receives and sends.
//! A handler is therefore testable without a connection, and the loop holds no
//! logic worth testing beyond the one end-to-end conversation below.
//!
//! [`state`] holds what the server remembers and turns a change into the
//! diagnostics to publish, with [`projects`] keeping the workspace's projects
//! and routing each file to one, [`inventories`] getting their
//! intersphinx inventories in, [`conf_runs`] running their `conf.py` where
//! that is allowed and [`bazel_manifests`] getting a Bazel workspace's site
//! manifests from the client; [`handlers`] are the pure functions from one
//! message to the replies, and [`run`] is the loop around them. [`scan`] is
//! the thread scanning the workspace beside the loop; the file-system
//! events the client reports reach [`state`] like any other notification.
//! [`check`] drives the same state in-process over a whole folder, for
//! measuring a corpus rather than serving an editor.

mod bazel_manifests;
mod check;
mod conf_runs;
mod handlers;
mod inventories;
mod projects;
mod renames;
mod run;
mod scan;
mod state;

#[cfg(test)]
mod bazel_manifest_tests;
#[cfg(test)]
mod code_action_tests;
#[cfg(test)]
mod completion_tests;
// The interpreter the tests run is a shell script.
#[cfg(all(test, unix))]
mod conf_run_tests;
#[cfg(test)]
mod definition_tests;
#[cfg(test)]
mod document_symbol_tests;
#[cfg(test)]
mod entity_tests;
#[cfg(test)]
mod folding_range_tests;
#[cfg(test)]
mod hover_tests;
#[cfg(test)]
mod inventory_tests;
#[cfg(test)]
mod needs_tests;
#[cfg(test)]
mod notification_tests;
#[cfg(test)]
mod preview_link_tests;
#[cfg(test)]
mod preview_tests;
#[cfg(test)]
mod project_tests;
#[cfg(test)]
mod property_tests;
#[cfg(test)]
mod references_tests;
#[cfg(test)]
mod rename_tests;
#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod semantic_tokens_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod strictness_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod watch_tests;
#[cfg(test)]
mod workspace_symbol_tests;
#[cfg(test)]
mod workspace_tests;

pub use check::{CheckOptions, FolderCheck, ReferencesAt, check_folder};
pub use run::run;
