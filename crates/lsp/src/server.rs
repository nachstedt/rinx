//! The protocol loop, and the handlers it dispatches to.
//!
//! Split the way every subcommand is (`process_*`/`cmd_*`): the handlers are
//! pure functions from the server's state and one incoming message to the
//! messages to send back, and [`run`] is the thin loop that receives and sends.
//! A handler is therefore testable without a connection, and the loop holds no
//! logic worth testing beyond the one end-to-end conversation below.
//!
//! [`state`] holds what the server remembers and turns a change into the
//! diagnostics to publish, [`handlers`] are the pure functions from one
//! message to the replies, and [`run`] is the loop around them. [`scan`] is
//! the thread scanning the workspace beside the loop.

mod handlers;
mod run;
mod scan;
mod state;

#[cfg(test)]
mod notification_tests;
#[cfg(test)]
mod property_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod workspace_tests;

pub use run::run;
