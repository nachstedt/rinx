//! A parser for the C declaration grammar used in Sphinx C-domain directive
//! arguments (`.. c:function::`, `.. c:type::`, `.. c:member::`, ...).
//!
//! # Why a hand-rolled parser
//!
//! Extracting the declared name from a C signature cannot be done by string
//! munging: in `int (*Py_tracefunc)(PyObject *obj)` the name sits inside a
//! grouped declarator, not before the first parenthesis. Nor can it be done
//! with an off-the-shelf C parser — those need a table of known typedef names
//! to resolve C's grammatical ambiguity (`A * B` is a declaration if `A` is a
//! type and a multiplication otherwise), and documentation signatures are
//! frequently abbreviated rather than compilable.
//!
//! That ambiguity does not arise here: a C-domain directive argument is
//! *always* a declaration, never an expression, so specifiers can simply be
//! consumed greedily and whatever follows is the declarator. Real Sphinx
//! resolves it the same way, with its own recursive-descent
//! `DefinitionParser` rather than a general C parser.
//!
//! # Guarantees
//!
//! [`parse_declaration`] never panics and never loops forever, on any input
//! whatsoever — recursion is depth-bounded and every loop consumes a token.
//! Malformed input yields [`ParseError`], which callers are expected to treat
//! as "fall back to a heuristic", not as a hard failure: a documentation build
//! must not lose a cross-reference target because a signature was unusual.

mod declaration;
mod parser;
mod token;

pub use declaration::{Declaration, Declarator};
pub use parser::{ParseError, declared_name, parse_declaration};
pub use token::{Token, TokenKind, tokenize};
