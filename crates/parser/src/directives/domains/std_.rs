//! Per-object-type parsers for the `std` domain (currently just
//! `.. option::`/`.. cmdoption::`). Named with a trailing underscore to avoid
//! shadowing the `std` crate. Dispatched to from
//! [`super::parse_domain_object`].

pub(super) mod cmdoption;
