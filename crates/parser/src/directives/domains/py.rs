//! The `py` domain: a submodule per object type (`.. py:function::`,
//! `.. py:module::`, `.. py:data::`, `.. py:method::`, `.. py:class::`/
//! `.. py:exception::`, `.. py:attribute::`), each with its own body-option
//! extractor, plus the [`dispatch`] that routes to them.

pub(super) mod attribute;
pub(super) mod class;
pub(super) mod data;
mod dispatch;
pub(super) mod function;
pub(super) mod method;
pub(super) mod module;

pub(super) use dispatch::parse_py_domain_object;
