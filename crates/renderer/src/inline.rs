//! Inline markup rendering: everything that turns a paragraph's
//! [`rinx_ast::InlineNode`]s into HTML.
//!
//! [`dispatch`] owns the entry point and matches on the node kind; each
//! remaining module renders one cross-reference role — [`reference`] for
//! `:ref:`, [`doc_reference`] for `:doc:`, [`download_reference`] for
//! `:download:` (which needs no index at all), [`hyperlink`] and
//! [`anonymous_reference`] for the two hyperlink forms, [`term_reference`]
//! for `:term:`, [`option_reference`] for `:option:`, and
//! [`domain_object_reference`] for the domain roles. The latter two resolve
//! through [`crate::resolution`], as do [`doc_reference`] and
//! [`any_reference`] — the last drawing each kind it finds through the other
//! modules' href builders.
//!
//! A reference that no document of this site defines but another site's
//! inventory lists is written by [`external_link`], whichever role found it.
//!
//! [`math`] and [`code`] are the two non-reference roles with a module of
//! their own, each because it calls a backend: the math renderer for
//! `:math:`, the syntax highlighter for `:code:` and the roles derived from
//! it. [`registry_reference`] is a reference that needs no index either:
//! `:pep:`, `:rfc:`, `:cve:` and `:cwe:` link outside the site, below their
//! registry's address — as do [`docutils_pep_reference`] and
//! [`docutils_rfc_reference`], docutils' plainer `:pep-reference:` and
//! `:rfc-reference:`. [`index_reference`] is no reference at all: `:index:`
//! writes the anchor its general-index entries link to and shows its text.
//!
//! [`RefText`] is the one shape they all share: the visible text, the target
//! to resolve, and where the role was written.

mod anonymous_reference;
mod any_reference;
mod code;
mod dispatch;
mod doc_reference;
mod docutils_pep_reference;
mod docutils_rfc_reference;
mod domain_object_reference;
mod download_reference;
pub(crate) mod entity_reference;
mod external_link;
mod hyperlink;
mod index_reference;
mod math;
mod number_reference;
mod option_reference;
mod ref_text;
mod reference;
mod registry_reference;
mod term_reference;

pub(crate) use dispatch::render_inline;
use ref_text::RefText;
