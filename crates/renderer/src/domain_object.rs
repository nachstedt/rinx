//! Domain-object directive rendering (`.. py:function::`, `.. c:function::`,
//! `.. option::`, etc.): the shared dispatcher, its prefix-label helpers, and
//! `c`-domain-specific rendering.

pub(super) mod c;
pub(super) mod directive;
pub(super) mod labels;
