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
//! and routing each file to one; [`handlers`] are the pure functions from one
//! message to the replies, and [`run`] is the loop around them. [`scan`] is
//! the thread scanning the workspace beside the loop; the file-system
//! events the client reports reach [`state`] like any other notification.

mod handlers;
mod projects;
mod run;
mod scan;
mod state;

#[cfg(test)]
mod completion_tests;
#[cfg(test)]
mod definition_tests;
#[cfg(test)]
mod hover_tests;
#[cfg(test)]
mod notification_tests;
#[cfg(test)]
mod project_tests;
#[cfg(test)]
mod property_tests;
#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod strictness_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod watch_tests;
#[cfg(test)]
mod workspace_tests;

pub use check::{FolderCheck, check_folder};
pub use run::run;
