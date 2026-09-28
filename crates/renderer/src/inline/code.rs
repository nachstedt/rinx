//! Rendering `:code:` and the roles a `.. role:: name(code)` derives from it.
//!
//! The class list follows Sphinx's: `code`, then — for a role with a language
//! — `highlight`, the role's own classes and the language (unless a class
//! already is), and the `highlight-<lang>` class a code block's wrapper also
//! carries. `hl-code` is added only when tokens were actually coloured,
//! because it is what hands the element to the generated theme's palette; an
//! unhighlighted role keeps the page's ordinary inline-code look.

use std::fmt::Write as _;

use rinx_ast::{ResolvedLanguage, Span};

use crate::highlight::{
    HighlightError, HighlightedConstruct, HighlightedLine, Highlighter, language_class,
};

/// One `InlineNode::Code`, borrowed.
pub(super) struct CodeRef<'a> {
    pub(super) text: &'a str,
    pub(super) language: &'a ResolvedLanguage,
    pub(super) classes: &'a [String],
    pub(super) span: Option<Span>,
}

/// Renders inline code, highlighting it when its role named a language.
///
/// A language with no grammar is reported under `code-role.*` and the text is
/// shown plain, exactly as a code block falls back.
pub(super) fn render_inline_code(
    html: &mut String,
    code: &CodeRef<'_>,
    highlighter: &Highlighter,
    highlight_errors: &mut Vec<HighlightError>,
) {
    let coloured = match highlighter.highlight(code.text, code.language) {
        Ok(lines) => lines,
        Err(mut error) => {
            error.span = code.span;
            error.construct = HighlightedConstruct::CodeRole;
            highlight_errors.push(error);
            None
        }
    };

    let classes = code_classes(code.language, code.classes, coloured.is_some());
    let _ = write!(
        html,
        "<code class=\"{}\">",
        html_escape::encode_double_quoted_attribute(&classes)
    );
    match coloured {
        Some(lines) => html.push_str(&join_lines(&lines)),
        None => html.push_str(&html_escape::encode_text(code.text)),
    }
    html.push_str("</code>");
}

/// The `class` attribute of the rendered `<code>`.
fn code_classes(language: &ResolvedLanguage, own: &[String], highlighted: bool) -> String {
    let mut classes = vec!["code".to_string()];
    let has_language = language.grammar_name().is_some();
    if has_language {
        classes.push("highlight".to_string());
    }
    classes.extend(own.iter().cloned());
    if has_language {
        let name = language.css_suffix().to_string();
        if !classes.contains(&name) {
            classes.push(name);
        }
        classes.push(language_class(language));
    }
    if highlighted {
        classes.push("hl-code".to_string());
    }
    classes.join(" ")
}

/// The highlighted lines as one run of inline markup. Inline text is reflowed
/// onto one line before it reaches the AST, so there is normally one; were
/// there more, a space stands where the line break was, as it would in prose.
fn join_lines(lines: &[HighlightedLine]) -> String {
    lines.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{DiagnosticCode, LanguageName, Position};

    fn named(name: &str) -> ResolvedLanguage {
        ResolvedLanguage::Named(LanguageName::new(name).unwrap())
    }

    fn render(
        text: &str,
        language: &ResolvedLanguage,
        classes: &[&str],
    ) -> (String, Vec<HighlightError>) {
        let classes: Vec<String> = classes.iter().map(ToString::to_string).collect();
        let mut html = String::new();
        let mut errors = Vec::new();
        render_inline_code(
            &mut html,
            &CodeRef {
                text,
                language,
                classes: &classes,
                span: Some(rinx_ast::Span::new(
                    Position::new(2, 1),
                    Position::new(2, 9),
                )),
            },
            &Highlighter::new(),
            &mut errors,
        );
        (html, errors)
    }

    #[test]
    fn test_render_inline_code_shows_plain_code_escaped() {
        // Given / When
        let (html, errors) = render("a < b", &ResolvedLanguage::None, &[]);

        // Then
        assert_eq!(html, "<code class=\"code\">a &lt; b</code>");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_render_inline_code_keeps_an_unhighlighted_roles_classes() {
        // Given / When
        let (html, _) = render("x", &ResolvedLanguage::None, &["snippet"]);

        // Then
        assert_eq!(html, "<code class=\"code snippet\">x</code>");
    }

    #[test]
    fn test_render_inline_code_highlights_a_named_language() {
        // Given / When
        let (html, errors) = render("print(\"x\")", &named("python"), &["extra"]);

        // Then — Sphinx's class order, the theme's palette, and coloured tokens
        assert!(
            html.starts_with(
                "<code class=\"code highlight extra python highlight-python hl-code\">"
            ),
            "{html}"
        );
        assert!(html.contains("<span class=\"hl-"), "{html}");
        assert!(html.contains("print"), "{html}");
        assert!(html.ends_with("</code>"), "{html}");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_render_inline_code_does_not_repeat_a_language_already_a_class() {
        // Given a role whose class is its language, as `.. role:: python(code)`
        // gets by default
        let (html, _) = render("x", &named("python"), &["python"]);

        // Then
        assert!(
            html.starts_with("<code class=\"code highlight python highlight-python hl-code\">"),
            "{html}"
        );
    }

    #[test]
    fn test_render_inline_code_reports_an_unknown_language_and_shows_the_text() {
        // Given / When
        let (html, errors) = render("a < b", &named("nonesuch-language"), &[]);

        // Then — plain text, classes as Sphinx gives them, reported as a role
        assert_eq!(
            html,
            "<code class=\"code highlight nonesuch-language highlight-nonesuch-language\">\
             a &lt; b</code>"
        );
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code(), DiagnosticCode::CodeRoleUnknownLanguage);
        assert_eq!(errors[0].span.map(|span| span.start.line), Some(2));
    }

    #[test]
    fn test_code_classes_marks_highlighting_only_when_it_happened() {
        // Given / When
        let highlighted = code_classes(&named("rust"), &[], true);
        let fell_back = code_classes(&named("rust"), &[], false);

        // Then
        assert_eq!(highlighted, "code highlight rust highlight-rust hl-code");
        assert_eq!(fell_back, "code highlight rust highlight-rust");
    }

    #[test]
    fn test_join_lines_joins_with_a_space() {
        // Given / When / Then
        assert_eq!(join_lines(&["a".to_string(), "b".to_string()]), "a b");
        assert_eq!(join_lines(&["only".to_string()]), "only");
    }
}
