//! A code block written as a directive: `.. code-block::` and its docutils
//! spelling `.. code::`.
//!
//! Distinct from [`crate::Node::LiteralBlock`], which is the `::` form. The
//! two are not structurally identical — a `::` block takes no options at all,
//! and folding them together would leave every literal block carrying eight
//! meaningless defaults forever, the same reasoning `Directive::DataTable`
//! gives for not folding into `Node::Table`.
//!
//! The two *directives*, on the other hand, do share this one node. They
//! differ only in how their source spells the same data out — `.. code::`
//! writes `:number-lines:` where `.. code-block::` writes `:linenos:` plus
//! `:lineno-start:` — and that difference is fully consumed while parsing.
//! [`CodeBlockSource`] records which one wrote it, exactly as
//! [`crate::TableSource`] does for the two table directives.

use serde::{Deserialize, Serialize};

use crate::code_language::CodeLanguage;
use crate::span::Span;
use crate::target_name::TargetName;

/// Which directive produced a [`CodeBlock`].
///
/// Drives the rendered CSS class and nothing else: by the time a block reaches
/// the renderer, the two spellings have already been normalized to the same
/// fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeBlockSource {
    /// `.. code-block::` — Sphinx's spelling.
    CodeBlock,
    /// `.. code::` — docutils' spelling.
    Code,
    /// `.. literalinclude::` — Sphinx's directive for a block whose text is
    /// read from a file rather than written in the document.
    ///
    /// A third variant rather than a separate node because by the time one
    /// reaches the renderer it *is* a code block: the file has been read, the
    /// lines selected, the tabs expanded and the dedent applied, all while
    /// parsing. What is left is the same content and the same nine
    /// presentation options the other two produce.
    LiteralInclude,
}

impl CodeBlockSource {
    /// The directive's name as an author writes it, for diagnostics that must
    /// quote the directive the author actually used.
    #[must_use]
    pub const fn directive_name(self) -> &'static str {
        match self {
            Self::CodeBlock => "code-block",
            Self::Code => "code",
            Self::LiteralInclude => "literalinclude",
        }
    }
}

/// A code block and the presentation options it was given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeBlock {
    /// Which directive wrote this block.
    pub source: CodeBlockSource,
    /// The language as written. [`CodeLanguage::Inherit`] when the directive
    /// took no argument, which the renderer resolves against the enclosing
    /// `.. highlight::` — see [`crate::CodeLanguage`] for why this is an enum
    /// and not an `Option<String>`.
    pub language: CodeLanguage,
    /// Verbatim content, with the common leading indent stripped and
    /// `:dedent:` already applied.
    ///
    /// `:dedent:` is deliberately *not* stored alongside it: it is a source
    /// transform, not a presentation option, so applying it while parsing
    /// keeps the AST holding exactly what the reader will see.
    pub content: String,
    /// `:caption:` — plain text, not parsed inline markup, matching what
    /// `DataTable`'s `title` already does for table captions.
    pub caption: Option<String>,
    /// `:name:` — makes the block a `:ref:` target.
    pub name: Option<TargetName>,
    /// `:class:` — space-separated class names, already split.
    pub classes: Vec<String>,
    /// `:linenos:`, or `.. code::`'s `:number-lines:`.
    ///
    /// Only ever means "the author asked for line numbers". They may still
    /// appear without it, when a `.. highlight::` set a `:linenothreshold:`
    /// this block is longer than — a decision only the renderer can make,
    /// since only it knows the threshold in force.
    pub linenos: bool,
    /// `:lineno-start:`, or the optional argument of `:number-lines:`. Implies
    /// [`Self::linenos`]. A [`NonZeroU32`] because a code listing starting at
    /// line zero is not a thing an author can mean, so the ambiguity is
    /// resolved once, where the source text is read.
    pub lineno_start: Option<std::num::NonZeroU32>,
    /// `:emphasize-lines:` — 1-based line numbers, already expanded from any
    /// `3-5` ranges, deduplicated and sorted. Validated against the block's
    /// own line count while parsing, so an out-of-range line is reported where
    /// it was written.
    pub emphasize_lines: Vec<std::num::NonZeroU32>,
    /// `:force:` — the author's assertion that a highlighting failure is
    /// acceptable, which suppresses the render-time diagnostic.
    pub force: bool,
    /// Where the directive was written, so the renderer can report an unknown
    /// language or a grammar failure against real source. The second
    /// `Directive` to carry one, for the same reason `Directive::Math` does:
    /// content that parses can still fail to render.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl CodeBlock {
    /// The number of lines the block will display.
    ///
    /// Used to decide whether a `:linenothreshold:` applies, and to reject an
    /// `:emphasize-lines:` entry pointing past the end.
    #[must_use]
    pub fn line_count(&self) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        self.content.lines().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    fn block(content: &str) -> CodeBlock {
        CodeBlock {
            source: CodeBlockSource::CodeBlock,
            language: CodeLanguage::Inherit,
            content: content.to_string(),
            caption: None,
            name: None,
            classes: Vec::new(),
            linenos: false,
            lineno_start: None,
            emphasize_lines: Vec::new(),
            force: false,
            span: None,
        }
    }

    #[test]
    fn test_directive_name_reflects_the_spelling_the_author_used() {
        // Given / When / Then
        assert_eq!(CodeBlockSource::CodeBlock.directive_name(), "code-block");
        assert_eq!(CodeBlockSource::Code.directive_name(), "code");
    }

    #[test]
    fn test_line_count_counts_the_lines_of_the_body() {
        // Given
        let code = block("one\ntwo\nthree");

        // When / Then
        assert_eq!(code.line_count(), 3);
    }

    #[test]
    fn test_line_count_is_zero_for_an_empty_body() {
        // Given — a directive with no content at all
        let code = block("");

        // When / Then — not one, as `"".lines()` would otherwise suggest
        assert_eq!(code.line_count(), 0);
    }

    #[test]
    fn test_line_count_counts_a_trailing_blank_line_as_written() {
        // Given — a body whose last line is empty
        let code = block("one\n");

        // When / Then — `lines()` does not invent a line after the final \n
        assert_eq!(code.line_count(), 1);
    }

    #[test]
    fn test_code_block_serialization_roundtrip() {
        // Given — every option set, so each field's serde survives
        let original = CodeBlock {
            source: CodeBlockSource::Code,
            language: CodeLanguage::parse("python"),
            content: "print(1)".to_string(),
            caption: Some("A caption".to_string()),
            name: Some(TargetName::new("my-block")),
            classes: vec!["boxed".to_string()],
            linenos: true,
            lineno_start: Some(NonZeroU32::new(10).unwrap()),
            emphasize_lines: vec![NonZeroU32::new(1).unwrap()],
            force: true,
            span: None,
        };

        // When
        let json = serde_json::to_string(&original).unwrap();
        let restored: CodeBlock = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(restored, original);
    }

    #[test]
    fn test_code_block_omits_an_absent_span_from_its_serialization() {
        // Given
        let code = block("print(1)");

        // When
        let json = serde_json::to_string(&code).unwrap();

        // Then — the field is skipped rather than written as null
        assert!(!json.contains("span"), "{json}");
    }
}
