//! Per-role-type handlers for the `std`-domain inline roles (currently just
//! `:option:`). Named with a trailing underscore to avoid shadowing the
//! `std` crate. Dispatched to from [`crate::inline::dispatch::handle_inline_match`].

pub(crate) mod option;
