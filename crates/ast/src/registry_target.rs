//! What a registry role points at — `:pep:`, `:rfc:`, `:cve:` and `:cwe:`,
//! Sphinx's four `ReferenceRole`s that link a numbered document outside the
//! site and index the place it was mentioned.
//!
//! The four roles are one construct with four registries, so one node,
//! `InlineNode::RegistryReference`, carries all of them and differs only in
//! its [`RegistryTarget`]. [`combined`] holds that enum and [`registry`] the
//! [`Registry`] tag naming which one it is; [`pep`], [`rfc`], [`cve`] and
//! [`cwe`] each parse one registry's targets, three of them through
//! [`number`]'s number-and-fragment shape. [`invalid`] is the one refusal
//! all four report through, so their messages are worded alike.

mod combined;
mod cve;
mod cwe;
mod invalid;
mod number;
mod pep;
mod registry;
mod rfc;

pub use combined::RegistryTarget;
pub use cve::CveTarget;
pub use cwe::CweTarget;
pub use invalid::{InvalidRegistryTarget, InvalidRegistryTargetReason};
pub(crate) use number::{NumberError, read_ascii_number};
pub use pep::PepTarget;
pub use registry::Registry;
pub use rfc::RfcTarget;
