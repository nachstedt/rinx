//! What the server knows about the workspace beyond the open documents: its
//! projects, and each project's documents and index (ADR-038 §2, §3).
//!
//! [`discover`] finds a workspace folder's projects — one per `conf.py`, and
//! the folder itself for what lies under none — with each one's
//! [`model`], read for a Sphinx project by [`sphinx_conf`] — or, where the
//! workspace allows it, run by [`conf_eval`] — and matched
//! against its files by [`exclusion`], its entity schema converted from its
//! sphinx-needs configuration by [`needs`] (reading `conf.py`'s literals
//! through [`python_value`]). [`scan`] parses and analyses their
//! documents in parallel when the server starts, and [`index`] keeps one
//! analysis per document and folds them into the project's index.

mod bazel_manifest;
mod conf_eval;
mod discover;
mod exclusion;
mod extension_profile;
mod index;
mod intersphinx;
mod inventories;
mod model;
mod needs;
mod python_value;
mod scan;
mod sphinx_conf;
mod strictness;

pub use bazel_manifest::{BazelGrant, BazelRoots, read_bazel_projects};
#[cfg(test)]
pub use conf_eval::ConfValues;
pub use conf_eval::{CONF_RUN_TIMEOUT, ConfEvaluation, ConfEvaluations, run_conf};
pub use discover::{
    DiscoveredProject, contains_conf, discover_projects, discover_projects_with, is_conf,
    is_visible_under, sources_under,
};
pub use index::{IndexedDocument, Project};
pub use inventories::Fetched;
pub use model::{ParseSettings, ProjectSource, Splice};
pub use scan::{ScannedProject, scan_projects};
#[cfg(test)]
pub use scan::{parse_project_documents, scan_folder};
#[cfg(test)]
pub use sphinx_conf::read_sphinx_conf;
pub use strictness::{Judgement, Strictness};
