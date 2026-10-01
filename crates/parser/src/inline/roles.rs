//! Per-domain-family role handlers dispatched to from
//! [`super::handle_inline_match`]: the Python-only roles (`:func:`/`:mod:`/
//! `:data:`/`:meth:`/`:class:`/`:attr:`/`:exc:`), the C-only roles
//! (`:macro:`/`:struct:`/`:union:`/`:type:`), and the one `std`-domain role
//! (`:option:`), plus the domain-agnostic target-parsing logic shared by all
//! of them.
//!
//! [`math`], [`code`], [`any`], [`doc`], [`download`], [`registry`],
//! [`pep_reference`] and [`rfc_reference`] are the exceptions to the
//! per-domain grouping: `:math:`/`:eq:` and `:code:` belong to no domain,
//! `:any:` searches every one, `:doc:` names a document rather than an
//! object, `:download:` a file and `:pep:`/`:rfc:`/`:cve:`/`:cwe:` and
//! docutils' `:pep-reference:`/`:rfc-reference:` a page outside the site, so
//! they sit flat here.

pub(super) mod any;
pub(super) mod c;
pub(super) mod code;
pub(super) mod doc;
pub(super) mod download;
pub(super) mod entity;
pub(super) mod math;
pub(super) mod numref;
pub(super) mod pep_reference;
pub(super) mod py;
pub(super) mod registry;
pub(super) mod rfc_reference;
pub(super) mod std_;
pub(super) mod target;
