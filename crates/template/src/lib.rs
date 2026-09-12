//! Rendering a document's source as a Jinja template, before it is parsed.
//!
//! # What this is for
//!
//! Real Sphinx projects template their `.rst` files. There is no Sphinx
//! feature for it: a `conf.py` connects the `source-read` event and runs each
//! file through Jinja2 on its way to docutils — the recipe from Eric
//! Holscher's "Integrating Jinja with RST", which projects reach for to share
//! a boilerplate header, to generate repetitive sections from data, or to
//! vary a page by build. useblocks' sphinx-needs demo opens 24 of its 31
//! documents with
//!
//! ```text
//! {% set page="index.rst" %}
//! {% include "demo_page_header.rst" with context %}
//! ```
//!
//! rusty-sphinx cannot run a `conf.py`, so the transform has to be a declared
//! feature of the build instead of a hook. This crate is that transform.
//!
//! # Why it is its own crate
//!
//! The same reason `rusty_sphinx_cdecl` and `rusty_sphinx_filter` are: it is a
//! self-contained text-to-text step with an injected seam for what it cannot
//! do itself, and it depends on nothing of ours at all. What it needs to read
//! a template arrives through [`TemplateLoader`], which `rusty_sphinx_parser`
//! implements over the file loader an `.. include::` already reads through.
//!
//! # The part that is not just "call `MiniJinja`"
//!
//! A Jinja pass moves lines. An `{% include %}` splices a whole file in, so
//! every position below it shifts, and `MiniJinja` offers no source map. A
//! renderer that ignored that would make every diagnostic below the first
//! include point at a line with nothing on it — and the parser's own
//! positions, a `.. noqa:`, and the span stored on every AST node for the
//! renderer to report against later all run through those numbers.
//!
//! So [`render_source`] returns a [`SourceLine`] for every rendered line,
//! built by injecting markers into the text before rendering and reading them
//! off afterwards; `marker` explains the mechanism and `scan` the scanner that
//! decides where a marker may go. The result is that a diagnostic raised
//! inside an included header names *that header* and its line, exactly as one
//! raised inside an `.. include::` fragment does.
//!
//! # Narrowings
//!
//! - **Whitespace-control modifiers (`{%-`, `-%}`) are refused.** A line
//!   marker is not whitespace, so it would defeat the trim they ask for, and
//!   in reStructuredText a silently changed indent changes what a block
//!   contains. Refusing by name beats rendering something subtly wrong.
//! - **A template name must be a quoted literal.** The build declares the
//!   files an action reads before it runs, so a name computed at render time
//!   could not be declared — and would fail inside the sandbox instead.
//! - **An undefined value is an error**, not the empty string Jinja2
//!   substitutes by default.
//! - **Templates resolve by the name written**, globally: two files that
//!   write the same include name mean the same template.

mod error;
mod loader;
mod marker;
mod render;
mod scan;

pub use error::{TemplateError, TemplateErrorKind};
pub use loader::{LoadedTemplate, RejectTemplates, TemplateLoader};
pub use marker::SourceLine;
pub use render::{RenderedSource, render_source};
