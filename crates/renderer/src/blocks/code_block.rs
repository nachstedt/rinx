//! Rendering a code block: the `<div class="highlight">` shell, the optional
//! caption and anchor, and the line-level markup that `:linenos:` and
//! `:emphasize-lines:` need.
//!
//! Every code block in a document comes through here, whatever wrote it — the
//! two directives, a `::` literal block, and the doctest family — so that a
//! code block looks the same however it was written. [`render_code`] is the
//! shared core; the directive form wraps it in the parts only it can have.

use std::fmt::Write as _;
use std::num::NonZeroU32;

use rusty_sphinx_ast::{CodeBlock, ResolvedLanguage};

use crate::RenderCtx;
use crate::highlight::language_class;

/// Renders a `.. code-block::`/`.. code::` with all of its options.
pub(super) fn render_code_block_directive(
    html: &mut String,
    block: &CodeBlock,
    ctx: &mut RenderCtx,
) {
    let language = block.language.resolve(&ctx.highlight_language);
    let linenos = block.linenos || exceeds_threshold(block, ctx.linenothreshold);

    let mut classes = vec![language_class(&language)];
    classes.push("notranslate".to_string());
    for class in &block.classes {
        classes.push(html_escape::encode_double_quoted_attribute(class).to_string());
    }

    let _ = write!(html, "<div class=\"{}\"", classes.join(" "));
    if let Some(name) = &block.name {
        let _ = write!(
            html,
            " id=\"{}\"",
            html_escape::encode_double_quoted_attribute(name.as_str())
        );
    }
    let _ = writeln!(html, ">");

    render_caption(html, block.caption.as_deref());

    let force = block.force || ctx.highlight_force;
    render_code(
        html,
        &block.content,
        &language,
        &CodeLayout {
            linenos,
            lineno_start: block.lineno_start,
            emphasize_lines: &block.emphasize_lines,
        },
        force,
        block.span,
        ctx,
    );

    let _ = writeln!(html, "</div>");
}

/// The per-line presentation a block asked for. Grouped into one type so
/// [`render_code`]'s signature does not grow a parameter per option.
pub(crate) struct CodeLayout<'a> {
    pub linenos: bool,
    pub lineno_start: Option<NonZeroU32>,
    pub emphasize_lines: &'a [NonZeroU32],
}

impl CodeLayout<'_> {
    /// The layout a block with no options of its own gets: no numbers, no
    /// emphasis. What a `::` literal block and a doctest block both use.
    pub(crate) const fn plain() -> Self {
        Self {
            linenos: false,
            lineno_start: None,
            emphasize_lines: &[],
        }
    }
}

/// Emits the `<div class="highlight"><pre>` shell and its highlighted content.
///
/// Reports a highlighting failure unless `force` was set, then falls back to
/// escaped plain text — the page always shows the author's code, whatever the
/// backend made of it.
pub(crate) fn render_code(
    html: &mut String,
    content: &str,
    language: &ResolvedLanguage,
    layout: &CodeLayout<'_>,
    force: bool,
    span: Option<rusty_sphinx_ast::Span>,
    ctx: &mut RenderCtx,
) {
    let highlighted = match ctx.highlighter.highlight(content, language) {
        Ok(lines) => lines,
        Err(mut error) => {
            if !force {
                error.span = span;
                ctx.highlight_errors.push(error);
            }
            None
        }
    };

    let lines: Vec<String> = highlighted.unwrap_or_else(|| {
        content
            .split('\n')
            .map(|line| html_escape::encode_text(line).to_string())
            .collect()
    });

    // `hl-code` is the class syntect's generated stylesheet puts the theme's
    // own foreground and background on. Carrying it here makes the theme
    // authoritative for the code area, so tokens it colours and tokens it
    // leaves alone agree with each other — without it the latter would fall
    // back to the page's body colour and sit visibly apart from the rest.
    let _ = writeln!(html, "<div class=\"highlight hl-code\"><pre>");
    let start = layout.lineno_start.map_or(1, NonZeroU32::get);
    for (offset, line) in lines.iter().enumerate() {
        let number = start.saturating_add(u32::try_from(offset).unwrap_or(u32::MAX));
        let line_number = u32::try_from(offset + 1).unwrap_or(u32::MAX);
        let emphasized = layout
            .emphasize_lines
            .iter()
            .any(|emphasized| emphasized.get() == line_number);

        if emphasized {
            html.push_str("<span class=\"hll\">");
        }
        if layout.linenos {
            let _ = write!(html, "<span class=\"linenos\">{number}</span>");
        }
        html.push_str(line);
        if emphasized {
            // The newline goes *inside* the highlight, so the emphasized band
            // spans the full width of the block rather than stopping at the
            // end of the text. This is the one case where a trailing newline
            // is emitted for the final line too, and it is load-bearing.
            html.push('\n');
            html.push_str("</span>");
        } else if offset + 1 < lines.len() {
            // No newline after the last line: inside a `<pre>` it would render
            // as an empty final row.
            html.push('\n');
        }
    }
    let _ = writeln!(html, "</pre></div>");
}

/// Whether a `.. highlight::`'s `:linenothreshold:` makes this block long
/// enough to number without being asked.
/// Applies a `.. highlight::`, which renders nothing and exists only to change
/// what the code blocks below it inherit.
///
/// Lives here rather than in the dispatcher for the same reason
/// [`super::scope_directives::apply_scope_directive`] does: a directive whose
/// whole effect is a context mutation belongs with the construct that reads
/// that context, not in the match that happens to reach it. Document order is
/// what makes it correct, which is why it runs during the node walk rather
/// than in a pre-pass.
pub(super) fn apply_highlight_directive(
    language: &ResolvedLanguage,
    linenothreshold: Option<NonZeroU32>,
    force: bool,
    ctx: &mut RenderCtx<'_>,
) {
    ctx.highlight_language = language.clone();
    ctx.linenothreshold = linenothreshold;
    ctx.highlight_force = force;
}

fn exceeds_threshold(block: &CodeBlock, threshold: Option<NonZeroU32>) -> bool {
    let Some(threshold) = threshold else {
        return false;
    };
    u32::try_from(block.line_count()).unwrap_or(u32::MAX) >= threshold.get()
}

/// Emits a block's `:caption:`, if it has one.
fn render_caption(html: &mut String, caption: Option<&str>) {
    if let Some(caption) = caption {
        let _ = writeln!(
            html,
            "<div class=\"code-block-caption\"><span class=\"caption-text\">{}</span></div>",
            html_escape::encode_text(caption)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::render_test_support::{with_ctx, with_highlight_language};
    use rusty_sphinx_ast::{
        CodeBlockSource, CodeLanguage, Document, LanguageName, Node, TargetName,
    };
    use rusty_sphinx_index::ProjectIndex;

    fn block(language: &str, content: &str) -> CodeBlock {
        CodeBlock {
            source: CodeBlockSource::CodeBlock,
            language: CodeLanguage::parse(language),
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

    fn render(block: &CodeBlock) -> String {
        with_ctx(|ctx| {
            let mut html = String::new();
            render_code_block_directive(&mut html, block, ctx);
            html
        })
    }

    fn named(name: &str) -> ResolvedLanguage {
        ResolvedLanguage::Named(LanguageName::new(name).unwrap())
    }

    /// Renders a whole document, the way `render` itself would.
    fn render_doc(doc: &Document) -> String {
        let index = ProjectIndex::default();
        crate::render(doc, &index, &doc.path).html
    }

    #[test]
    fn test_render_code_block_highlights_with_classed_spans() {
        // Given
        let code = block("python", "x = 1");

        // When
        let html = render(&code);

        // Then — token classes, all under the one namespaced prefix
        assert!(
            html.contains("<div class=\"highlight hl-code\"><pre>"),
            "{html}"
        );
        assert!(html.contains("hl-source hl-python"), "{html}");
    }

    #[test]
    fn test_render_code_block_names_the_language_on_its_wrapper() {
        // Given
        let code = block("python", "x = 1");

        // When
        let html = render(&code);

        // Then
        assert!(
            html.contains("<div class=\"highlight-python notranslate\""),
            "{html}"
        );
    }

    #[test]
    fn test_render_code_block_escapes_html_in_the_source() {
        // Given — source that would otherwise become markup
        let code = block("none", "a < b && b > c");

        // When
        let html = render(&code);

        // Then
        assert!(html.contains("a &lt; b &amp;&amp; b &gt; c"), "{html}");
        assert!(!html.contains("<b>"), "{html}");
    }

    #[test]
    fn test_render_code_block_leaves_an_explicit_none_unhighlighted() {
        // Given
        let code = block("none", "x = 1");

        // When
        let html = render(&code);

        // Then — no token classes at all
        assert!(!html.contains("<span class=\"hl-"), "{html}");
        assert!(html.contains("x = 1"), "{html}");
    }

    #[test]
    fn test_render_code_block_emits_a_caption() {
        // Given
        let mut code = block("python", "x = 1");
        code.caption = Some("An example".to_string());

        // When
        let html = render(&code);

        // Then
        assert!(
            html.contains(
                "<div class=\"code-block-caption\"><span class=\"caption-text\">An example</span></div>"
            ),
            "{html}"
        );
    }

    #[test]
    fn test_render_code_block_escapes_a_caption() {
        // Given
        let mut code = block("python", "x = 1");
        code.caption = Some("a <b> caption".to_string());

        // When
        let html = render(&code);

        // Then
        assert!(html.contains("a &lt;b&gt; caption"), "{html}");
    }

    #[test]
    fn test_render_code_block_emits_the_name_as_an_anchor_id() {
        // Given
        let mut code = block("python", "x = 1");
        code.name = Some(TargetName::new("my-block"));

        // When
        let html = render(&code);

        // Then
        assert!(html.contains("id=\"my-block\""), "{html}");
    }

    #[test]
    fn test_render_code_block_appends_the_class_option() {
        // Given
        let mut code = block("python", "x = 1");
        code.classes = vec!["boxed".to_string()];

        // When
        let html = render(&code);

        // Then — after the language and `notranslate`, not instead of them
        assert!(
            html.contains("class=\"highlight-python notranslate boxed\""),
            "{html}"
        );
    }

    #[test]
    fn test_render_code_block_numbers_lines_from_one() {
        // Given
        let mut code = block("none", "a\nb\nc");
        code.linenos = true;

        // When
        let html = render(&code);

        // Then
        assert!(html.contains("<span class=\"linenos\">1</span>"), "{html}");
        assert!(html.contains("<span class=\"linenos\">3</span>"), "{html}");
    }

    #[test]
    fn test_render_code_block_numbers_lines_from_lineno_start() {
        // Given
        let mut code = block("none", "a\nb");
        code.linenos = true;
        code.lineno_start = NonZeroU32::new(10);

        // When
        let html = render(&code);

        // Then
        assert!(html.contains("<span class=\"linenos\">10</span>"), "{html}");
        assert!(html.contains("<span class=\"linenos\">11</span>"), "{html}");
        assert!(!html.contains("<span class=\"linenos\">1</span>"), "{html}");
    }

    #[test]
    fn test_render_code_block_omits_line_numbers_by_default() {
        // Given
        let code = block("none", "a\nb");

        // When
        let html = render(&code);

        // Then
        assert!(!html.contains("linenos"), "{html}");
    }

    #[test]
    fn test_render_code_block_wraps_an_emphasized_line() {
        // Given — the second of three lines
        let mut code = block("none", "a\nb\nc");
        code.emphasize_lines = vec![NonZeroU32::new(2).unwrap()];

        // When
        let html = render(&code);

        // Then — the newline sits inside the band, so it spans the full width
        assert!(html.contains("<span class=\"hll\">b\n</span>"), "{html}");
    }

    #[test]
    fn test_render_code_block_emphasizes_only_the_named_lines() {
        // Given
        let mut code = block("none", "a\nb\nc");
        code.emphasize_lines = vec![NonZeroU32::new(1).unwrap()];

        // When
        let html = render(&code);

        // Then
        assert_eq!(html.matches("class=\"hll\"").count(), 1, "{html}");
    }

    #[test]
    fn test_render_code_block_reports_an_unknown_language() {
        // Given — a language no bundled grammar provides
        let code = block("nonesuch-language", "x = 1");

        // When
        let errors = with_ctx(|ctx| {
            let mut html = String::new();
            render_code_block_directive(&mut html, &code, ctx);
            ctx.highlight_errors.clone()
        });

        // Then
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(
            errors[0].code(),
            rusty_sphinx_ast::DiagnosticCode::CodeBlockUnknownLanguage
        );
    }

    #[test]
    fn test_render_code_block_still_shows_the_source_of_an_unknown_language() {
        // Given — a failure must never cost the reader the content
        let code = block("nonesuch-language", "x = 1");

        // When
        let html = render(&code);

        // Then
        assert!(html.contains("x = 1"), "{html}");
    }

    #[test]
    fn test_render_code_block_force_suppresses_the_unknown_language_report() {
        // Given
        let mut code = block("nonesuch-language", "x = 1");
        code.force = true;

        // When
        let errors = with_ctx(|ctx| {
            let mut html = String::new();
            render_code_block_directive(&mut html, &code, ctx);
            ctx.highlight_errors.clone()
        });

        // Then
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_render_code_block_reports_an_unknown_language_against_its_span() {
        // Given — a block whose parser recorded where it was written
        let mut code = block("nonesuch-language", "x = 1");
        let span = rusty_sphinx_ast::Span::new(
            rusty_sphinx_ast::Position::new(7, 1),
            rusty_sphinx_ast::Position::new(7, 5),
        );
        code.span = Some(span);

        // When
        let errors = with_ctx(|ctx| {
            let mut html = String::new();
            render_code_block_directive(&mut html, &code, ctx);
            ctx.highlight_errors.clone()
        });

        // Then — the diagnostic can point at real source
        assert_eq!(errors[0].span, Some(span));
    }

    #[test]
    fn test_render_code_block_does_not_report_an_unknown_default_language() {
        // Given — `default` promises Python only if it fits, so a miss is
        // never worth telling the author about
        let code = block("default", "not really python <<<");

        // When
        let errors = with_ctx(|ctx| {
            let mut html = String::new();
            render_code_block_directive(&mut html, &code, ctx);
            ctx.highlight_errors.clone()
        });

        // Then
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_render_code_block_inherits_the_current_highlight_language() {
        // Given — a block with no language of its own, under `.. highlight::`
        let code = block("", "let x = 1;");

        // When
        let html = with_highlight_language(named("rust"), |ctx| {
            let mut html = String::new();
            render_code_block_directive(&mut html, &code, ctx);
            html
        });

        // Then
        assert!(html.contains("highlight-rust"), "{html}");
        assert!(html.contains("hl-source hl-rust"), "{html}");
    }

    #[test]
    fn test_render_code_block_own_language_beats_the_inherited_one() {
        // Given
        let code = block("python", "x = 1");

        // When
        let html = with_highlight_language(named("rust"), |ctx| {
            let mut html = String::new();
            render_code_block_directive(&mut html, &code, ctx);
            html
        });

        // Then
        assert!(html.contains("highlight-python"), "{html}");
    }

    #[test]
    fn test_render_code_block_numbers_lines_past_the_linenothreshold() {
        // Given — a three-line block and a threshold of three
        let code = block("none", "a\nb\nc");

        // When
        let html = with_ctx(|ctx| {
            ctx.linenothreshold = NonZeroU32::new(3);
            let mut html = String::new();
            render_code_block_directive(&mut html, &code, ctx);
            html
        });

        // Then — numbered without the block asking
        assert!(html.contains("class=\"linenos\""), "{html}");
    }

    #[test]
    fn test_render_code_block_leaves_a_block_below_the_linenothreshold_alone() {
        // Given — two lines against a threshold of three
        let code = block("none", "a\nb");

        // When
        let html = with_ctx(|ctx| {
            ctx.linenothreshold = NonZeroU32::new(3);
            let mut html = String::new();
            render_code_block_directive(&mut html, &code, ctx);
            html
        });

        // Then
        assert!(!html.contains("linenos"), "{html}");
    }

    #[test]
    fn test_render_code_block_balances_spans_on_every_line() {
        // Given — a string whose scope stays open across three lines, which
        // would otherwise nest tags illegally around per-line markup
        let mut code = block("python", "x = \"\"\"multi\nline\nstring\"\"\"");
        code.linenos = true;

        // When
        let html = render(&code);

        // Then — every line closes what it opened
        for line in html.lines().filter(|line| line.contains("linenos")) {
            assert_eq!(
                line.matches("<span").count(),
                line.matches("</span>").count(),
                "unbalanced spans in:\n{line}"
            );
        }
    }

    #[test]
    fn test_render_literal_block_is_highlighted_by_the_inherited_language() {
        // Given — a `::` block names no language but is still highlighted,
        // as Sphinx does
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::LiteralBlock {
                language: CodeLanguage::Inherit,
                content: "def hello():\n    pass".to_string(),
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then — the site default is Sphinx's `default`, i.e. Python
        assert!(result.contains("<span class=\"hl-"), "{result}");
        assert!(result.contains("def"), "{result}");
    }

    #[test]
    fn test_render_literal_block_escapes_html() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::LiteralBlock {
                language: CodeLanguage::None,
                content: "a < b && b > c".to_string(),
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("a &lt; b &amp;&amp; b &gt; c"), "{result}");
    }

    #[test]
    fn test_render_literal_block_has_no_directive_wrapper() {
        // Given — a `::` block has no caption, name or classes to hang on one
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::LiteralBlock {
                language: CodeLanguage::None,
                content: "x = 1".to_string(),
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(
            result.contains("<div class=\"highlight hl-code\"><pre>"),
            "{result}"
        );
        assert!(!result.contains("notranslate"), "{result}");
    }

    #[test]
    fn test_exceeds_threshold_is_false_without_one() {
        // Given
        let code = block("none", "a\nb\nc");

        // When / Then — no `.. highlight::` set one, so nothing is numbered
        assert!(!exceeds_threshold(&code, None));
    }

    #[test]
    fn test_exceeds_threshold_compares_inclusively() {
        // Given — a three-line block
        let code = block("none", "a\nb\nc");

        // When / Then — "at least this many", matching Sphinx
        assert!(exceeds_threshold(&code, NonZeroU32::new(3)));
        assert!(!exceeds_threshold(&code, NonZeroU32::new(4)));
    }
}
