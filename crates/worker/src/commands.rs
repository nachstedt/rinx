//! One module per CLI subcommand, each pairing a pure `process_*` function
//! (testable without file I/O) with the thin `cmd_*` wrapper that does the
//! reads and writes — the split `main.rs`'s dispatcher calls into.
//!
//! [`parse`], [`validate_toctree`], [`diagrams`], [`extract_doctests`] and
//! [`embed_assets`] are the per-document Phase 1 actions; [`index`] is the
//! single Phase 2 merge;
//! [`render`] and [`genindex`] are Phase 3, and [`preview`] collapses all
//! three into one process for the editor. [`cli_args`] holds the flag parsing
//! they share, [`diagnostics`] the warning formatting [`render`] and
//! [`preview`] share, [`suppression`] the `.. noqa:` filtering applied just
//! before that formatting, and [`parse_files`] the filesystem loader that
//! resolves `.. csv-table::`'s `:file:` option for both of them.

mod cli_args;
mod diagnostics;
mod diagrams;
mod embed_assets;
mod extract_doctests;
mod genindex;
mod index;
mod parse;
mod parse_files;
mod preview;
mod render;
mod suppression;
mod validate_toctree;

pub(crate) use diagrams::{cmd_extract_diagrams, cmd_validate_images};
pub(crate) use embed_assets::cmd_embed_assets;
pub(crate) use extract_doctests::cmd_extract_doctests;
pub(crate) use genindex::cmd_genindex;
pub(crate) use index::cmd_index;
pub(crate) use parse::cmd_parse;
pub(crate) use preview::cmd_preview;
pub(crate) use render::cmd_render;
pub(crate) use validate_toctree::cmd_validate_toctree;
