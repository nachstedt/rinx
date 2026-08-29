//! Domain-object directive rendering (`.. py:function::`, `.. c:function::`,
//! `.. option::`, etc.).
//!
//! [`directive`] is the shared dispatcher [`super::dispatch`] calls; it reads
//! the signature prefix labels from [`labels`] and hands the `std`-domain
//! `option` bodies to [`c`], which also renders the `c`-domain objects whose
//! signatures need the C declaration parser.

mod c;
mod directive;
mod labels;

pub(crate) use directive::render_domain_object;
