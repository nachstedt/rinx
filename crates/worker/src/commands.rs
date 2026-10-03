//! One module per CLI subcommand, each pairing a pure `process_*` function
//! (testable without file I/O) with the thin `cmd_*` wrapper that does the
//! reads and writes — the split `main.rs`'s dispatcher calls into.
//!
//! [`parse`], [`validate_toctree`], [`extract_doctests`] and
//! [`embed_assets`] are the per-document Phase 1 actions; [`index`] is the
//! single Phase 2 merge;
//! [`render`], [`genindex`] and [`modindex`] are Phase 3, [`validate_assets`] checks the
//! bundled images and downloads after it, and [`preview`] collapses the first
//! three phases into one process for the editor, and [`lsp`] serves the
//! language server over stdio. [`cli_args`] holds the flag parsing
//! they share, [`diagnostics`] the warning formatting [`render`] and
//! [`preview`] share, [`suppression`] the `.. noqa:` filtering applied just
//! before that formatting, and [`parse_files`] the filesystem loader that
//! resolves `.. csv-table::`'s `:file:` option for both of them.

mod cli_args;
mod diagnostics;
mod embed_assets;
mod entity_json_schema;
mod entity_schema;
mod extract_doctests;
mod genindex;
mod index;
mod inventory;
mod lsp;
mod modindex;
mod parse;
mod parse_files;
mod parse_inputs;
mod preview;
mod render;
mod suppression;
mod validate_assets;
mod validate_toctree;

pub(crate) use embed_assets::cmd_embed_assets;
pub(crate) use entity_json_schema::cmd_entity_json_schema;
pub(crate) use extract_doctests::cmd_extract_doctests;
pub(crate) use genindex::cmd_genindex;
pub(crate) use index::cmd_index;
pub(crate) use inventory::cmd_inventory;
pub(crate) use lsp::cmd_lsp;
pub(crate) use modindex::cmd_modindex;
pub(crate) use parse::cmd_parse;
pub(crate) use preview::cmd_preview;
pub(crate) use render::cmd_render;
pub(crate) use validate_assets::cmd_validate_assets;
pub(crate) use validate_toctree::cmd_validate_toctree;
