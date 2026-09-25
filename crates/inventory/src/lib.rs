//! Sphinx's `objects.inv` inventory format, read and written.
//!
//! An inventory lists every cross-reference target a documentation site
//! defines — name, `domain:role`, priority, URI and display name — and is how
//! intersphinx links one site into another. This crate owns only the *file
//! format* (version 2: a four-line plain-text header followed by a
//! zlib-compressed body): it knows nothing about a `ProjectIndex` or about
//! resolving a reference, which are the index's and the renderer's business.
//! It is a leaf crate like `rinx_cdecl` and `rinx_filter`,
//! and for the same reason: the worker writes an inventory, the index action
//! reads one and the AST names one, and those phases may not depend on each
//! other.
//!
//! The format is Sphinx's own (`sphinx.util.inventory`), and the checked-in
//! `testdata/sphinx-9.1.0.inv` — written by a real `sphinx-build` — is the
//! compatibility bar both directions are tested against.

mod entry;
mod entry_type;
mod error;
mod inventory_name;
mod read;
mod write;

pub use entry::{Inventory, InventoryEntry};
pub use entry_type::{EntryType, InvalidEntryType};
pub use error::{InventoryError, MalformedLine};
pub use inventory_name::{InvalidInventoryName, InventoryName};
pub use read::{ReadInventory, read_inventory};
pub use write::write_inventory;
