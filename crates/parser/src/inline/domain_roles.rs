//! Per-domain-family role handlers dispatched to from
//! [`super::handle_inline_match`]: the Python-only roles (`:func:`/`:mod:`/
//! `:data:`/`:meth:`/`:class:`/`:attr:`/`:exc:`) and the C-only roles
//! (`:macro:`/`:struct:`/`:union:`/`:type:`).

pub(super) mod c;
pub(super) mod py;
