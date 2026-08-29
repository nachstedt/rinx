//! Per-object-type parsers for the `py` domain (`.. py:function::`,
//! `.. py:module::`, `.. py:data::`, `.. py:method::`, `.. py:class::`/
//! `.. py:exception::`, `.. py:attribute::`), each with its own body-option
//! extractor. Dispatched to from [`super::parse_py_domain_object`].

pub(super) mod attribute;
pub(super) mod class;
pub(super) mod data;
pub(super) mod function;
pub(super) mod method;
pub(super) mod module;
