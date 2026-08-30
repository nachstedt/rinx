//! Per-role-type handlers for the Python-domain (and, for `func`/`data`,
//! dual-domain) inline roles. Dispatched to from
//! [`crate::inline::dispatch::handle_inline_match`].

pub(crate) mod attr;
pub(crate) mod class;
pub(crate) mod data;
pub(crate) mod exc;
pub(crate) mod func;
pub(crate) mod meth;
pub(crate) mod mod_;

#[cfg(test)]
mod pipeline_tests;
