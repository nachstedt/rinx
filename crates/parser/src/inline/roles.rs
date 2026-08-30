//! Per-domain-family role handlers dispatched to from
//! [`super::handle_inline_match`]: the Python-only roles (`:func:`/`:mod:`/
//! `:data:`/`:meth:`/`:class:`/`:attr:`/`:exc:`), the C-only roles
//! (`:macro:`/`:struct:`/`:union:`/`:type:`), and the one `std`-domain role
//! (`:option:`), plus the domain-agnostic target-parsing logic shared by all
//! of them.
//!
//! [`math`] is the exception to the per-domain grouping: `:math:`/`:eq:`
//! belong to no domain and resolve nothing per-domain, so they sit flat here.

pub(super) mod c;
pub(super) mod math;
pub(super) mod py;
pub(super) mod std_;
pub(super) mod target;
