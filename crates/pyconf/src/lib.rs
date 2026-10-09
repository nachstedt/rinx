//! Reading the settings a Python module assigns — Sphinx's `conf.py` above
//! all — without running it.
//!
//! # Why a reader of our own
//!
//! A `conf.py` is a program, and running it needs the project's Python, its
//! Sphinx and its extensions, and trust in its code: an editor opening a
//! project must not execute it unasked (ADR-038 §4). What a language server
//! needs from it, though, is a handful of settings that are almost always
//! written as literals. So this crate reads the *top-level assignments whose
//! value is a literal*, and says plainly where it could not: a computed
//! value is bound to something unread, and a change the module makes later —
//! `exclude_patterns.append(…)` inside an `if` — is recorded as a
//! modification the reader did not apply. It never guesses.
//!
//! Ruff's and `RustPython`'s parsers were considered and rejected: the first
//! is an internal crate re-released with breaking changes every week and
//! needs a C toolchain in the build, the second is unmaintained, and either
//! would save only the tokenizer and the statement skipping — the literal
//! evaluation and the reporting would still be ours.
//!
//! # How it fits together
//!
//! [`tokenize`] splits a module into tokens with Python's logical lines and
//! indentation, scanning every string literal (f-strings with nested fields
//! included) so the module is read correctly past it. [`read_literal`]
//! reads an expression that is a literal and refuses every other one, and
//! [`read_module`] walks the statements and binds names.
//!
//! # Guarantees
//!
//! [`read_module`] never panics and never loops forever, on any input:
//! nesting is depth-bounded and every loop consumes input. A module Python
//! would refuse is read up to the statement it fails in, and the failure is
//! [`ModuleReading::error`].

mod lexer;
mod literal;
mod module;
mod position;
mod reading;
mod token;
mod value;

pub use lexer::{Tokenized, tokenize};
pub use literal::read_literal;
pub use module::read_module;
pub use position::{Position, Span};
pub use reading::{Binding, BoundValue, ModuleReading};
pub use token::{SyntaxError, SyntaxErrorKind, Token, TokenKind};
pub use value::{Value, ValueKind};
