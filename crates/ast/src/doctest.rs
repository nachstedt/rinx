//! The `sphinx.ext.doctest` node family.
//!
//! [`block`] holds [`DocTestBlock`], the one node every `doctest`/`testcode`/
//! `testoutput`/`testsetup`/`testcleanup` directive parses into; [`group`]
//! holds the group name and selector that decide which blocks run together,
//! and [`flag`] the `doctest` comparison flags a block may set.

mod block;
mod flag;
mod group;

pub use block::{DocTestBlock, DocTestTrim};
pub use flag::{DocTestFlag, DocTestFlagName};
pub use group::{DocTestGroup, DocTestGroupSelector};
