//! Per-domain-family role handlers dispatched to from
//! [`super::handle_inline_match`]: the Python-only roles (`:func:`/`:mod:`/
//! `:data:`/`:meth:`/`:class:`/`:attr:`/`:exc:`), the C-only roles
//! (`:macro:`/`:struct:`/`:union:`/`:type:`), and the one `std`-domain role
//! (`:option:`), plus the domain-agnostic target-parsing logic shared by all
//! of them.
//!
//! [`math`], [`code`], [`script`], [`title_reference`], [`custom`], [`any`],
//! [`doc`], [`download`], [`registry`], [`pep_reference`], [`rfc_reference`]
//! and [`index`] are the exceptions to the per-domain grouping:
//! `:math:`/`:eq:`, `:code:`, `:sub:`/`:sup:`, `:title-reference:` and the
//! roles a `.. role::` derives from them belong to no domain,
//! `:any:` searches every one, `:doc:` names a document rather than an
//! object, `:download:` a file, `:pep:`/`:rfc:`/`:cve:`/`:cwe:` and
//! docutils' `:pep-reference:`/`:rfc-reference:` a page outside the site, and
//! `:index:` nothing at all, so they sit flat here.

pub(super) mod any;
pub(super) mod c;
pub(super) mod code;
pub(super) mod custom;
pub(super) mod doc;
pub(super) mod download;
pub(super) mod entity;
pub(super) mod index;
pub(super) mod math;
pub(super) mod numref;
pub(super) mod pep_reference;
pub(super) mod py;
pub(super) mod registry;
pub(super) mod rfc_reference;
pub(super) mod script;
pub(super) mod std_;
pub(super) mod target;
pub(super) mod title_reference;
