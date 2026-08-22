//! Rendering for the `sphinx.ext.doctest` directive family.
//!
//! Rendering is deliberately independent of execution: the HTML a doctest block
//! produces is a function of the block alone, never of whether its code was run
//! or passed. That is what lets page rendering stay a pure, cacheable step while
//! test execution lives in a separate, opt-in Bazel target.

use rusty_sphinx_ast::{DocTestBlock, DocTestTrim};
use std::fmt::Write as _;

/// The project-wide default for `trim_doctest_flags`, matching Sphinx's own
/// default. A block that specifies neither `:trim-doctest-flags:` nor
/// `:no-trim-doctest-flags:` resolves against this.
const TRIM_DOCTEST_FLAGS_DEFAULT: bool = true;

/// Renders a doctest block, or returns `None` when the block produces no HTML.
///
/// `testsetup`/`testcleanup` never render, and any block carrying `:hide:`
/// renders nothing — in both cases the code still runs, it just isn't shown.
#[must_use]
pub(crate) fn render_doctest_block(block: &DocTestBlock) -> Option<String> {
    if !block.is_rendered() {
        return None;
    }

    let language = match block {
        // `pycon` is the console-session lexer: `>>>` prompts plus their output.
        DocTestBlock::Interactive { .. } => Some("pycon"),
        DocTestBlock::Code { .. } => Some("python"),
        // Expected output is not source in any language, so it stays
        // unhighlighted rather than being mislabelled as Python.
        DocTestBlock::Output { .. } => None,
        DocTestBlock::Setup { .. } | DocTestBlock::Cleanup { .. } => return None,
    };

    let display = display_text(block);
    let escaped = html_escape::encode_text(&display);

    let mut html = String::new();
    if let Some(lang) = language {
        let _ = writeln!(
            html,
            "<pre><code class=\"language-{lang}\">{escaped}</code></pre>"
        );
    } else {
        let _ = writeln!(html, "<pre><code>{escaped}</code></pre>");
    }
    Some(html)
}

/// Derives the text shown on the page from the block's verbatim body.
///
/// The stored content is the *test* source; the presentation form drops markup
/// that exists only for the test runner. Deriving it here (rather than storing
/// a second, pre-trimmed copy on the AST) keeps the test source authoritative
/// and keeps presentation-only options out of the extracted test plan, so
/// toggling one cannot invalidate a cached test run.
fn display_text(block: &DocTestBlock) -> String {
    let body = block.content().body();

    match block {
        DocTestBlock::Interactive { trim, .. } => {
            // `<BLANKLINE>` is how doctest spells "an empty line is expected
            // here". It is always noise on the page, regardless of trimming.
            let without_markers = strip_blankline_markers(body);
            maybe_strip_flag_comments(&without_markers, *trim)
        }
        DocTestBlock::Code { trim, .. } | DocTestBlock::Output { trim, .. } => {
            maybe_strip_flag_comments(body, *trim)
        }
        DocTestBlock::Setup { .. } | DocTestBlock::Cleanup { .. } => body.to_string(),
    }
}

/// Applies [`strip_flag_comments`] when the block's tri-state resolves to
/// trimming.
fn maybe_strip_flag_comments(text: &str, trim: DocTestTrim) -> String {
    if trim.resolve(TRIM_DOCTEST_FLAGS_DEFAULT) {
        strip_flag_comments(text)
    } else {
        text.to_string()
    }
}

/// Removes lines consisting only of a `<BLANKLINE>` marker, leaving the blank
/// line the marker stands for.
/// Splits on `\n` rather than using [`str::lines`], which silently drops a
/// trailing empty line — that would make these transforms lossy when chained,
/// and would swallow a trailing blank line that a `<BLANKLINE>` marker
/// deliberately stands for.
fn strip_blankline_markers(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            if line.trim() == "<BLANKLINE>" {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Removes trailing `# doctest: +FLAG` comments, which direct the runner and
/// mean nothing to a reader.
///
/// Only the comment is dropped; if the line carries code before it, that code
/// stays and its trailing whitespace is trimmed.
/// Splits on `\n` for the same reason as [`strip_blankline_markers`].
fn strip_flag_comments(text: &str) -> String {
    text.split('\n')
        .map(|line| match find_flag_comment(line) {
            Some(index) => line[..index].trim_end().to_string(),
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Returns the byte index at which a `# doctest:` comment starts, if any.
///
/// Matches `#`, optional whitespace, then `doctest:` — the same shape as
/// Sphinx's `doctestopt_re`.
fn find_flag_comment(line: &str) -> Option<usize> {
    line.match_indices('#').find_map(|(index, _)| {
        let rest = line[index + 1..].trim_start();
        rest.starts_with("doctest:").then_some(index)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{DocTestGroupSelector, HashedContent, NonEmptyVector};

    /// A single-selector group list for the default group.
    fn default_groups() -> NonEmptyVector<DocTestGroupSelector> {
        NonEmptyVector::single(DocTestGroupSelector::new(""))
    }

    fn interactive(body: &str, hide: bool, trim: DocTestTrim) -> DocTestBlock {
        DocTestBlock::Interactive {
            groups: default_groups(),
            content: HashedContent::new(body.to_string()),
            hide,
            flags: Vec::new(),
            pyversion: None,
            skipif: None,
            trim,
        }
    }

    fn code(body: &str, hide: bool) -> DocTestBlock {
        DocTestBlock::Code {
            groups: default_groups(),
            content: HashedContent::new(body.to_string()),
            hide,
            pyversion: None,
            skipif: None,
            trim: DocTestTrim::Unset,
        }
    }

    fn output(body: &str) -> DocTestBlock {
        DocTestBlock::Output {
            groups: default_groups(),
            content: HashedContent::new(body.to_string()),
            hide: false,
            flags: Vec::new(),
            pyversion: None,
            skipif: None,
            trim: DocTestTrim::Unset,
        }
    }

    #[test]
    fn test_render_doctest_block_renders_interactive_examples_as_pycon() {
        // Given
        let block = interactive(">>> 1 + 1\n2", false, DocTestTrim::Unset);

        // When
        let html = render_doctest_block(&block).expect("should render");

        // Then
        assert_eq!(
            html,
            "<pre><code class=\"language-pycon\">&gt;&gt;&gt; 1 + 1\n2</code></pre>\n"
        );
    }

    #[test]
    fn test_render_doctest_block_renders_testcode_as_python() {
        // Given
        let block = code("print(1)", false);

        // When
        let html = render_doctest_block(&block).expect("should render");

        // Then
        assert_eq!(
            html,
            "<pre><code class=\"language-python\">print(1)</code></pre>\n"
        );
    }

    #[test]
    fn test_render_doctest_block_renders_testoutput_without_a_language() {
        // Given — expected output is not source in any language.
        let block = output("1");

        // When
        let html = render_doctest_block(&block).expect("should render");

        // Then
        assert_eq!(html, "<pre><code>1</code></pre>\n");
    }

    #[test]
    fn test_render_doctest_block_renders_nothing_for_setup() {
        // Given
        let block = DocTestBlock::Setup {
            groups: default_groups(),
            content: HashedContent::new("import os".to_string()),
            skipif: None,
        };

        // When
        let html = render_doctest_block(&block);

        // Then
        assert_eq!(html, None);
    }

    #[test]
    fn test_render_doctest_block_renders_nothing_for_cleanup() {
        // Given
        let block = DocTestBlock::Cleanup {
            groups: default_groups(),
            content: HashedContent::new("pass".to_string()),
            skipif: None,
        };

        // When
        let html = render_doctest_block(&block);

        // Then
        assert_eq!(html, None);
    }

    #[test]
    fn test_render_doctest_block_renders_nothing_for_a_hidden_block() {
        // Given
        let block = interactive(">>> 1", true, DocTestTrim::Unset);

        // When
        let html = render_doctest_block(&block);

        // Then
        assert_eq!(html, None);
    }

    #[test]
    fn test_render_doctest_block_renders_nothing_for_hidden_testcode() {
        // Given
        let block = code("print(1)", true);

        // When
        let html = render_doctest_block(&block);

        // Then
        assert_eq!(html, None);
    }

    #[test]
    fn test_render_doctest_block_escapes_html_in_the_body() {
        // Given
        let block = code("print('<b>')", false);

        // When
        let html = render_doctest_block(&block).expect("should render");

        // Then
        assert!(html.contains("&lt;b&gt;"));
        assert!(!html.contains("<b>"));
    }

    #[test]
    fn test_display_text_removes_blankline_markers_from_interactive_blocks() {
        // Given
        let block = interactive(
            ">>> f()\nline\n<BLANKLINE>\nafter",
            false,
            DocTestTrim::Unset,
        );

        // When
        let display = display_text(&block);

        // Then — the marker's line becomes the blank line it stands for.
        assert_eq!(display, ">>> f()\nline\n\nafter");
    }

    #[test]
    fn test_display_text_removes_indented_blankline_markers() {
        // Given
        let block = interactive(">>> f()\n   <BLANKLINE>", false, DocTestTrim::Unset);

        // When
        let display = display_text(&block);

        // Then
        assert_eq!(display, ">>> f()\n");
    }

    #[test]
    fn test_display_text_strips_flag_comments_by_default() {
        // Given — no trim option given, so the project default (trim) applies.
        let block = interactive(
            ">>> f()  # doctest: +ELLIPSIS\n...",
            false,
            DocTestTrim::Unset,
        );

        // When
        let display = display_text(&block);

        // Then
        assert_eq!(display, ">>> f()\n...");
    }

    #[test]
    fn test_display_text_keeps_flag_comments_when_trimming_is_disabled() {
        // Given
        let block = interactive(">>> f()  # doctest: +ELLIPSIS", false, DocTestTrim::NoTrim);

        // When
        let display = display_text(&block);

        // Then
        assert_eq!(display, ">>> f()  # doctest: +ELLIPSIS");
    }

    #[test]
    fn test_display_text_strips_flag_comments_when_trimming_is_explicit() {
        // Given
        let block = interactive(">>> f()  # doctest: +SKIP", false, DocTestTrim::Trim);

        // When
        let display = display_text(&block);

        // Then
        assert_eq!(display, ">>> f()");
    }

    #[test]
    fn test_display_text_leaves_ordinary_comments_alone() {
        // Given — only `# doctest:` comments are runner directives.
        let block = code("x = 1  # a normal comment", false);

        // When
        let display = display_text(&block);

        // Then
        assert_eq!(display, "x = 1  # a normal comment");
    }

    #[test]
    fn test_display_text_leaves_the_body_untouched_for_setup() {
        // Given — setup never renders, so no presentation trimming applies.
        let block = DocTestBlock::Setup {
            groups: default_groups(),
            content: HashedContent::new("x = 1  # doctest: +SKIP".to_string()),
            skipif: None,
        };

        // When
        let display = display_text(&block);

        // Then
        assert_eq!(display, "x = 1  # doctest: +SKIP");
    }

    #[test]
    fn test_find_flag_comment_locates_a_comment_without_a_space() {
        // Given
        let line = ">>> f()  #doctest: +SKIP";

        // When
        let index = find_flag_comment(line);

        // Then
        assert_eq!(index, Some(9));
    }

    #[test]
    fn test_find_flag_comment_ignores_a_plain_comment() {
        // Given
        let line = "x = 1  # not a directive";

        // When
        let index = find_flag_comment(line);

        // Then
        assert_eq!(index, None);
    }

    #[test]
    fn test_find_flag_comment_finds_a_later_hash_when_the_first_is_not_a_directive() {
        // Given
        let line = "x = 1  # note  # doctest: +SKIP";

        // When
        let index = find_flag_comment(line);

        // Then — the directive comment, not the first `#`.
        assert_eq!(index, line.find("# doctest:"));
    }

    #[test]
    fn test_strip_flag_comments_drops_a_whole_line_that_is_only_a_directive() {
        // Given
        let text = ">>> f()\n# doctest: +SKIP";

        // When
        let stripped = strip_flag_comments(text);

        // Then
        assert_eq!(stripped, ">>> f()\n");
    }

    #[test]
    fn test_strip_flag_comments_preserves_a_trailing_blank_line() {
        // Given — `str::lines()` would drop this, making the transform lossy
        // when chained after blankline stripping.
        let text = ">>> f()\n";

        // When
        let stripped = strip_flag_comments(text);

        // Then
        assert_eq!(stripped, ">>> f()\n");
    }

    #[test]
    fn test_strip_blankline_markers_preserves_a_trailing_blank_line() {
        // Given
        let text = "a\n";

        // When
        let stripped = strip_blankline_markers(text);

        // Then
        assert_eq!(stripped, "a\n");
    }

    #[test]
    fn test_strip_blankline_markers_leaves_other_lines_untouched() {
        // Given
        let text = "a\nb";

        // When
        let stripped = strip_blankline_markers(text);

        // Then
        assert_eq!(stripped, "a\nb");
    }
}
