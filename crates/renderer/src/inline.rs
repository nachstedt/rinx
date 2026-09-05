//! Inline markup rendering: everything that turns a paragraph's
//! [`rusty_sphinx_ast::InlineNode`]s into HTML.
//!
//! [`dispatch`] owns the entry point and matches on the node kind; each
//! remaining module renders one cross-reference role — [`reference`] for
//! `:ref:`, [`hyperlink`] and [`anonymous_reference`] for the two hyperlink
//! forms, [`term_reference`] for `:term:`, [`option_reference`] for
//! `:option:`, and [`domain_object_reference`] for the domain roles. The
//! latter two resolve through [`crate::resolution`].
//!
//! [`RefText`] is the one shape they all share: the visible text, the target
//! to resolve, and where the role was written.

mod anonymous_reference;
mod dispatch;
mod domain_object_reference;
mod hyperlink;
mod math;
mod option_reference;
mod ref_text;
mod reference;
mod term_reference;

pub(crate) use dispatch::render_inline;
use ref_text::RefText;
