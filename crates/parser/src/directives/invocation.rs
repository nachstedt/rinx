//! What every directive is interpreted from: its block, already split into
//! arguments, options and content the way docutils splits it.
//!
//! The split belongs to the syntax a directive is written in — for
//! reStructuredText, [`super::reader`] — and the [`DirectiveInvocation`] it
//! yields is what every `interpret_*` reads, so one signature serves them all
//! and no directive splits its own block.

use std::ops::Range;

use crate::context::{ParseCtx, SectionTitles};
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use rinx_ast::{Directive, Node, Span};

use super::body::DirectiveContent;
use super::error_node::unknown_directive;
use super::options::OptionLine;

/// One directive as written, split by its spec's shape.
///
/// Its lines are the *block*: index 0 is the text after the marker's `::`,
/// index `i` the `i`-th line below the marker, unindented — docutils'
/// `indented` block. [`Self::options`] and every position read off
/// [`Self::lines`] are in that index space, positioned by [`Self::ctx`].
pub(in crate::directives) struct DirectiveInvocation<'a, 'c> {
    /// The name, as written before the `::`.
    pub(in crate::directives) name: &'a str,
    /// The whole marker line, for a diagnostic about the directive itself.
    pub(in crate::directives) span: Option<Span>,
    /// The arguments, as many as were written and the shape accepts; a
    /// missing one has been reported and is absent.
    pub(in crate::directives) arguments: Vec<String>,
    /// The block lines the arguments were read from, for a directive reading
    /// one entry per line of its argument (`.. index::`, a domain object's
    /// signatures) with each line's position.
    pub(in crate::directives) argument_lines: Range<usize>,
    /// The option lines, in block index space.
    pub(in crate::directives) options: Vec<OptionLine>,
    /// The block lines the options were read from, continuations included.
    pub(in crate::directives) option_lines: Range<usize>,
    /// Every line of the block, unindented.
    pub(in crate::directives) lines: Vec<String>,
    /// Positions [`Self::lines`]; takes no section titles, as docutils'
    /// nested parse of a directive's content does not.
    pub(in crate::directives) ctx: ParseCtx<'c>,
    /// The index of the content's first line within [`Self::lines`].
    pub(in crate::directives) content_start: usize,
    /// Whether section titles are allowed where the directive stands.
    pub(in crate::directives) enclosing_titles: SectionTitles,
    /// The body as written below the marker, for a node that quotes its
    /// source back.
    pub(in crate::directives) source_body: &'a [&'a str],
}

impl<'c> DirectiveInvocation<'_, 'c> {
    /// The span of the content's first line, or of the marker when there is
    /// no content: for a node standing for the content, or a finding inside it
    /// that no longer knows its own line, such as a list table's row.
    ///
    /// A verdict about the directive as a whole is reported on
    /// [`Self::span`] instead, as docutils reports one on the directive's line.
    pub(in crate::directives) fn content_span(&self) -> Option<Span> {
        match self.lines.get(self.content_start) {
            Some(text) => self.ctx.line_span(self.content_start, text),
            None => self.span,
        }
    }

    /// The argument at `index`, or the empty string when it was not written.
    pub(in crate::directives) fn argument(&self, index: usize) -> &str {
        self.arguments.get(index).map_or("", String::as_str)
    }

    /// The content: the block's lines from its first content line on, with
    /// any option line among them blanked so every index stays the same,
    /// under a context positioned on that first line.
    pub(in crate::directives) fn content(&self) -> DirectiveContent<'c> {
        let start = self.content_start.min(self.lines.len());
        let lines = self.lines[start..]
            .iter()
            .enumerate()
            .map(|(offset, line)| {
                let index = start + offset;
                if self.option_lines.contains(&index) {
                    String::new()
                } else {
                    line.clone()
                }
            })
            .collect();
        DirectiveContent {
            lines,
            ctx: self.ctx.nested(start, 0),
        }
    }

    /// [`Self::content`] under a context that allows section titles exactly
    /// where the directive stands — for a directive that splices its nodes
    /// into the enclosing block, and a domain object, whose content Sphinx
    /// parses with `allow_section_headings=True`.
    pub(in crate::directives) fn content_with_enclosing_titles(&self) -> DirectiveContent<'c> {
        let content = self.content();
        DirectiveContent {
            ctx: content.ctx.with_section_titles(self.enclosing_titles),
            ..content
        }
    }
}

/// The document-wide state every directive's parse may add to.
pub(in crate::directives) struct ParseState<'s> {
    /// The section adornments seen so far, in order.
    pub(in crate::directives) adornment_order: &'s mut Vec<Adornment>,
    /// The diagnostics found so far.
    pub(in crate::directives) diagnostics: &'s mut Diagnostics,
}

/// The nodes of a directive yielding exactly one.
pub(in crate::directives) fn single(directive: Directive) -> Vec<Node> {
    vec![Node::Directive(directive)]
}

/// The nodes of a directive whose name is no spelling its parser reads back:
/// reported and kept as [`Directive::Unknown`], as a name without a table entry
/// is. The directive table's own tests rule this out.
pub(in crate::directives) fn unknown(
    invocation: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<Node> {
    single(unknown_directive(
        invocation.name.to_string(),
        invocation.arguments.join(" "),
        invocation.source_body,
        invocation.span,
        state.diagnostics,
    ))
}
