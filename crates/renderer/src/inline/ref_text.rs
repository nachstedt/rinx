//! The three things every cross-reference role carries into its renderer.

use rusty_sphinx_ast::Span;

/// A cross-reference as the author wrote it: what the reader sees, what to
/// resolve, and where it was written.
///
/// Bundled because all four of the index-resolved roles take exactly this
/// triple and nothing else varies between them — and because `span` arriving
/// as a separate parameter had pushed the widest of them past the argument
/// count clippy allows, which was a fair complaint about three values that
/// are really one.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RefText<'a> {
    /// The visible link text.
    pub display: &'a str,
    /// The name to look up in the project index.
    pub target: &'a str,
    /// Where the role was written, when the parser could place it.
    pub span: Option<Span>,
}
