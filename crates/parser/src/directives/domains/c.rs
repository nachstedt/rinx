//! The `c` domain: a submodule per container/declaration type
//! (`.. c:struct::`, `.. c:union::`, `.. c:member::`/`.. c:var::`,
//! `.. c:type::`), plus the [`dispatch`] that routes to them.
//!
//! `c:function`/`c:macro` have no submodule of their own — they carry no
//! type-specific options, so [`dispatch::parse_c_domain_object`] constructs
//! them directly from [`super::body::parse_body`].

mod dispatch;
pub(super) mod member;
pub(super) mod struct_;
pub(super) mod type_;
pub(super) mod union;

pub(super) use dispatch::parse_c_domain_object;
