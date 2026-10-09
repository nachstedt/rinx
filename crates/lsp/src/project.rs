//! What the server knows about the workspace beyond the open documents: its
//! projects, and each project's documents and index (ADR-038 §2, §3).
//!
//! [`discover`] finds a workspace folder's projects — one per `conf.py`, and
//! the folder itself for what lies under none — with each one's
//! [`model`], read for a Sphinx project by [`sphinx_conf`] and matched
//! against its files by [`exclusion`]. [`scan`] parses and analyses their
//! documents in parallel when the server starts, and [`index`] keeps one
//! analysis per document and folds them into the project's index.

mod discover;
mod exclusion;
mod index;
mod model;
mod scan;
mod sphinx_conf;

pub use discover::{
    DiscoveredProject, contains_conf, discover_projects, is_conf, is_visible_under, sources_under,
};
pub use index::{IndexedDocument, Project};
pub use model::{ParseSettings, ProjectSource};
pub use scan::{ScannedProject, scan_projects};
#[cfg(test)]
pub use scan::{parse_project_documents, scan_folder};
#[cfg(test)]
pub use sphinx_conf::read_sphinx_conf;
