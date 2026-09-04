//! The `.. toctree::` directive's content: the entries it lists and the
//! options that shape how they are displayed and numbered.
//!
//! [`content`] holds [`Toctree`], the directive as a whole; [`entry`] holds
//! [`TocEntry`], one interpreted body line; [`options`] holds
//! [`ToctreeOptions`], the four valued option lines; [`flag`] holds
//! [`ToctreeFlag`], the five valueless ones; and [`numbering`] holds
//! [`NumberedDepth`], which is `:numbered:`'s two spellings.
//!
//! The split matters because a toctree is read by every later phase and each
//! reads a different half: the analyzer resolves `entries` against the source
//! tree, the renderer applies `options` while walking them, and the section
//! numbering pass needs both. Interpreting the entry kinds once, here, is what
//! lets all three agree on what an entry line meant.

mod content;
mod entry;
mod flag;
mod numbering;
mod options;

pub use content::Toctree;
pub use entry::TocEntry;
pub use flag::ToctreeFlag;
pub use numbering::NumberedDepth;
pub use options::ToctreeOptions;
