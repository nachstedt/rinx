//! Migrating a Sphinx project into a rinx Bazel build (ADR-038 §7, roadmap
//! #37): the files `rinx init --from-sphinx` writes, and the "Migrate to
//! rinx" source action offers.
//!
//! [`plan_migration`] works the files out from a project the server has
//! scanned, so the command line and the editor write the same ones. The
//! library's declarations come from what the documents' parses read, showed,
//! linked and drew ([`rinx_analyzer::required_declarations`]); the site's
//! settings from `conf.py` (`rinx.toml`, `rinx_toml.rs`); the `srcs` glob is
//! measured against the package rather than translated from Sphinx's
//! patterns (`srcs.rs`); and the Bazel module is found or made
//! (`module_bazel.rs`, `package.rs`). What cannot be carried over is a
//! [`MigrationFinding`], never a guess.

mod build_file;
mod finding;
mod init;
mod module_bazel;
mod package;
mod plan;
mod rinx_toml;
mod srcs;

pub use finding::MigrationFinding;
pub use init::{InitOptions, init_from_sphinx};
pub use plan::{MigrationOptions, MigrationPlan, PlannedContent, PlannedFile, plan_migration};
