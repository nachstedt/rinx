//! `.. index::` directive parsing for RST documents.
//!
//! `pair:`/`triple:` entries are expanded into plain `single:`-equivalent
//! `IndexEntry::Term`s here, at parse time, mirroring how real Sphinx treats
//! them as pure shorthand — downstream code (analyzer, renderer) therefore
//! only ever has to handle one linkable entry shape.
//!
//! The argument line and every body line share one grammar (confirmed
//! against real-world usage in `CPython`'s own docs): each **physical line**
//! is checked, as a whole, against the known type keywords first
//! (`single:`, `pair:`, `triple:`, `see:`, `seealso:`, optionally
//! `!`-prefixed to mark the entry main). If a type keyword matches,
//! everything after it is *one* value — including any literal `,` it
//! contains, e.g. `single: , (comma); in string formatting`
//! (`Doc/library/string.rst`). Only when *no* type keyword matches does the
//! line fall back to being a comma-separated list of bare shorthand terms
//! (`BNF, grammar, syntax`), and even then a term may itself contain a
//! literal `;`, e.g. `object; code, code object` (`Doc/c-api/code.rst`) —
//! bare terms never get subentry-split. A line is never partly typed and
//! partly bare-shorthand. The `!` main-entry marker is strictly a
//! whole-line prefix *before* the type keyword — never embedded in the
//! value — since real Sphinx entries like `single: ! (exclamation); in
//! formatted string literal` use a literal `!` character as part of the
//! indexed text itself.
//!
//! `single:`'s value/subentry split on `;` (`parse_single_value`) has the
//! same "don't split on a delimiter that's part of the literal indexed
//! text" concern, one level down: `single: ; (semicolon)`
//! (`Doc/library/os.rst`) indexes a literal semicolon, so splitting
//! unconditionally on the first `;` would produce a bogus empty primary. It
//! only splits when *both* resulting parts are non-empty, mirroring real
//! Sphinx's `sphinx.util.index_entries._split_into`.

mod entries;
mod value_parsing;

pub(super) use entries::parse_index_directive;
