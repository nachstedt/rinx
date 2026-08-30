//! Enumerated-list markers: what `1.`, `(a)` and `IV)` parse into.
//!
//! [`value`] holds [`Enumerator`] itself — a sequence, a format and an
//! ordinal, validated on construction — over the two enums that classify it:
//! [`sequence`]'s numbering system (arabic, roman, alphabetic) and
//! [`format`]'s surrounding punctuation.

mod format;
mod sequence;
mod value;

pub use format::EnumeratorFormat;
pub use sequence::EnumeratorSequence;
pub use value::{Enumerator, EnumeratorError};
