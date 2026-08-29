//! Per-role-type handlers for the C-only inline roles. Dispatched to from
//! [`crate::inline::dispatch::handle_inline_match`].

pub(crate) mod macro_;
pub(crate) mod struct_;
pub(crate) mod type_;
pub(crate) mod union;
