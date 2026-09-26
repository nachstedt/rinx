//! `.. code-block::` and `.. code::` — the two directives that write a code
//! block inline in the document.
//!
//! Both lower to one [`Directive::CodeBlock`]; [`CodeBlockSource`] records
//! which spelling wrote it. The two differ only in how they say the same
//! things, and that difference is consumed here rather than left for the
//! renderer: `.. code::` writes `:number-lines:` (optionally with a start
//! value) where `.. code-block::` writes `:linenos:` plus `:lineno-start:`.
//!
//! Parsing is shallow in the same way [`crate::directives::math`] is: the body
//! is collected verbatim and never inspected for meaning. Whether the language
//! has a grammar behind it is the renderer's question, since only it knows the
//! highlighting backend.

use rinx_ast::{CodeBlock, CodeBlockSource, Directive};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::body::body_span;
use crate::directives::options::{report_unknown_options, scan_option_lines};
use crate::indent::unindent_body_lines;

use super::dedent::apply_dedent;
use super::emphasize::resolve_emphasize_lines;
use super::options::{parse_code_block_options, parse_language_argument};
use rinx_ast::DiagnosticCode;

/// Parses either code-block directive into a [`Directive::CodeBlock`].
pub(in crate::directives) fn parse_code_block(
    source: CodeBlockSource,
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let directive = source.directive_name();
    let language = parse_language_argument(argument);

    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, opt_idx) = scan_option_lines(&unindented_lines);
    let (options, unrecognized) = parse_code_block_options(source, &option_lines, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        directive,
        DiagnosticCode::DirectiveUnknownOption,
        diagnostics,
        ctx,
    );

    let body: Vec<&str> = unindented_lines[opt_idx..]
        .iter()
        .map(String::as_str)
        .collect();
    let content = apply_dedent(&body, options.dedent);

    let line_count = if content.is_empty() {
        0
    } else {
        content.lines().count()
    };
    let emphasize_lines = resolve_emphasize_lines(
        options.emphasize_lines_raw.as_ref(),
        line_count,
        directive,
        diagnostics,
        ctx,
    );

    Directive::CodeBlock(CodeBlock {
        source,
        language,
        content,
        caption: options.caption,
        name: options.name,
        classes: options.classes,
        // `:lineno-start:` implies numbering: an author who says where the
        // numbers begin has already said they want them.
        linenos: options.linenos || options.lineno_start.is_some(),
        lineno_start: options.lineno_start,
        emphasize_lines,
        force: options.force,
        span: body_span(body_lines, ctx),
    })
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{parse, parse_as};
    use rinx_ast::{CodeBlockSource, CodeLanguage, LanguageName};

    #[test]
    fn test_parse_code_block_keeps_the_body_verbatim() {
        // Given — a body whose relative indentation carries meaning
        let body = ["   def f():", "       return 1"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(block.content, "def f():\n    return 1");
    }

    #[test]
    fn test_parse_code_block_keeps_role_lines_after_the_blank_line_as_content() {
        // Given — a block *showing* roles: after the blank line below the
        // directive, a line starting with `:` is code, not an option
        let body = ["", "   :ref:`label`", "   :term:`word`"];

        // When
        let (block, diagnostics) = parse("rst", &body);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(block.content, ":ref:`label`\n:term:`word`");
    }

    #[test]
    fn test_parse_code_block_reads_the_language_argument() {
        // Given / When
        let (block, _) = parse("Python", &["   x = 1"]);

        // Then — normalized on the way in
        assert_eq!(
            block.language,
            CodeLanguage::Named(LanguageName::new("python").unwrap())
        );
    }

    #[test]
    fn test_parse_code_block_treats_a_missing_argument_as_inherit() {
        // Given / When
        let (block, diagnostics) = parse("", &["   x = 1"]);

        // Then — no diagnostic: "nothing written" is meaningful here
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(block.language, CodeLanguage::Inherit);
    }

    #[test]
    fn test_parse_code_block_keeps_option_lines_out_of_the_content() {
        // Given — the bug this whole change exists to fix: option lines used
        // to be rendered as code text
        let body = ["   :linenos:", "   :caption: Example", "", "   x = 1"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(block.content, "x = 1");
        assert!(block.linenos);
        assert_eq!(block.caption.as_deref(), Some("Example"));
    }

    #[test]
    fn test_parse_code_block_carries_a_span_for_render_time_diagnostics() {
        // Given — the renderer must be able to point at an unknown language
        let body = ["   x = 1"];

        // When
        let (block, _) = parse("nonesuch", &body);

        // Then
        assert!(block.span.is_some());
    }

    #[test]
    fn test_parse_code_block_records_which_directive_wrote_it() {
        // Given / When
        let (sphinx, _) = parse_as(CodeBlockSource::CodeBlock, "python", &["   x = 1"]);
        let (docutils, _) = parse_as(CodeBlockSource::Code, "python", &["   x = 1"]);

        // Then
        assert_eq!(sphinx.source, CodeBlockSource::CodeBlock);
        assert_eq!(docutils.source, CodeBlockSource::Code);
    }
}
