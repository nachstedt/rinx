//! The one place `syntect` is called, turning source text into classed HTML.
//!
//! Everything outside this module speaks in code text in and one HTML string
//! per line out, so the choice of highlighting backend never leaks into the
//! node renderers — and never into a `.ast` file either, since highlighting
//! happens at render time rather than at parse time (see
//! `docs/decisions/006-syntax-highlighting.md`).
//!
//! **Why one string per line rather than one per block.** `:linenos:` needs a
//! line-number cell in front of each line and `:emphasize-lines:` needs a
//! wrapper around whole lines, so the caller has to be able to address a line
//! on its own. syntect's own `ClassedHTMLGenerator` emits one flat stream with
//! no line structure, which is why the lower-level `ParseState` /
//! `line_tokens_to_classed_spans` pair is driven here instead.
//!
//! That has one consequence worth understanding before touching this code: a
//! `<span>` syntect opens on one line may not be closed until several lines
//! later (a docstring, a block comment), and interleaving per-line markup
//! would nest those tags illegally. So every line is *balanced on its own* —
//! the scopes still open when a line ends are closed at its end and reopened
//! at the start of the next. The rendered colours are identical; only the tag
//! nesting differs, and it differs in the direction that keeps the HTML
//! well-formed.

use std::sync::OnceLock;

use rusty_sphinx_ast::{DiagnosticCode, ResolvedLanguage, Span};
use syntect::html::{ClassStyle, line_tokens_to_classed_spans};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

/// The prefix on every emitted class name.
///
/// Namespaced so a token class can never collide with one of the document's
/// own (`.document pre`, `.admonition`, …), and so the generated stylesheet
/// can be scoped to exactly these.
const CLASS_PREFIX: &str = "hl-";

const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed {
    prefix: CLASS_PREFIX,
};

/// Source that could not be highlighted, and why.
///
/// Not a [`crate::BrokenLink`]: nothing failed to *resolve*. Either the
/// language has no grammar behind it or a grammar failed part-way through,
/// which are different things to tell an author and report under their own
/// codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightError {
    /// What went wrong, phrased for the document's author.
    pub message: String,
    /// Which of the two failures this was.
    pub kind: HighlightErrorKind,
    /// Where the block was written, when the AST node carried a position.
    pub span: Option<Span>,
}

/// The two ways highlighting can fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightErrorKind {
    /// No grammar is bundled for the language the author named.
    UnknownLanguage,
    /// A grammar was found but failed while parsing the source. Under the
    /// pure-Rust `fancy-regex` backend a few Sublime grammars use constructs
    /// that cannot be compiled, which is the usual cause.
    GrammarFailed,
}

impl HighlightError {
    /// The diagnostic code this reports under — what a `.. noqa:` names to
    /// suppress it.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self.kind {
            HighlightErrorKind::UnknownLanguage => DiagnosticCode::CodeBlockUnknownLanguage,
            HighlightErrorKind::GrammarFailed => DiagnosticCode::CodeBlockHighlightFailed,
        }
    }
}

/// The bundled grammars, loaded at most once per process.
///
/// Decoding the syntax set costs tens of milliseconds and every document in a
/// build pays it otherwise. The set is immutable once built, so sharing one is
/// safe and the load is pure computation over data compiled into the binary —
/// no file is read, which is what keeps a render action hermetic.
fn syntax_set() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(two_face::syntax::extra_newlines)
}

/// Highlights source text into classed HTML.
///
/// Holds no state of its own today, but is a type rather than a free function
/// for the same reason [`crate::math::MathRenderer`] is: it is constructed
/// once per render and threaded on the context, so the grammar set is resolved
/// at the start of a page rather than at every code block.
pub(crate) struct Highlighter {
    syntaxes: &'static SyntaxSet,
}

/// One line of a highlighted block: HTML with its `<span>` tags balanced, so
/// the caller may wrap it or prepend to it freely.
pub(crate) type HighlightedLine = String;

impl Highlighter {
    pub(crate) fn new() -> Self {
        Self {
            syntaxes: syntax_set(),
        }
    }

    /// Highlights `code` as `language`, one HTML string per line.
    ///
    /// Returns `Ok(None)` when the language asks for no highlighting at all —
    /// a distinct answer from a failure, so the caller renders plain text
    /// without reporting anything.
    ///
    /// # Errors
    ///
    /// Returns a [`HighlightError`] when the language names no known grammar,
    /// or when a grammar fails while parsing. Both leave the caller to fall
    /// back to plain text.
    pub(crate) fn highlight(
        &self,
        code: &str,
        language: &ResolvedLanguage,
    ) -> Result<Option<Vec<HighlightedLine>>, HighlightError> {
        let Some(name) = language.grammar_name() else {
            return Ok(None);
        };

        if is_unhighlightable(name) {
            return Ok(None);
        }

        let Some(syntax) = self.find_syntax(name) else {
            // `default` means "Python if it fits", so a missing grammar is an
            // expected outcome rather than something to tell the author about.
            if language.tolerates_failure() {
                return Ok(None);
            }
            return Err(HighlightError {
                message: format!("no syntax highlighting is available for language '{name}'"),
                kind: HighlightErrorKind::UnknownLanguage,
                span: None,
            });
        };

        match self.highlight_lines(code, syntax) {
            Ok(lines) => Ok(Some(lines)),
            Err(message) => {
                if language.tolerates_failure() {
                    return Ok(None);
                }
                Err(HighlightError {
                    message,
                    kind: HighlightErrorKind::GrammarFailed,
                    span: None,
                })
            }
        }
    }

    /// Finds a grammar by the name an author writes.
    ///
    /// Tries the human-readable name first (`Python`, `Rust`), then the
    /// extension aliases (`py`, `rs`), because Sphinx authors use both and
    /// Pygments accepts both.
    fn find_syntax(&self, name: &str) -> Option<&'static syntect::parsing::SyntaxReference> {
        let name = resolve_alias(name);
        self.syntaxes
            .find_syntax_by_token(name)
            .or_else(|| self.syntaxes.find_syntax_by_extension(name))
    }

    /// Drives the parser line by line, balancing each line's `<span>` tags.
    fn highlight_lines(
        &self,
        code: &str,
        syntax: &syntect::parsing::SyntaxReference,
    ) -> Result<Vec<HighlightedLine>, String> {
        let mut state = ParseState::new(syntax);
        let mut stack = ScopeStack::new();
        let mut lines = Vec::new();

        for line in code.split_inclusive('\n') {
            // The scopes already open when this line begins. They were closed
            // at the end of the previous line, so this line must reopen them
            // before emitting anything of its own.
            let carried = stack.scopes.clone();

            let ops = state
                .parse_line(line, self.syntaxes)
                .map_err(|error| error.to_string())?;
            let (body, delta) = line_tokens_to_classed_spans(line, &ops, CLASS_STYLE, &mut stack)
                .map_err(|error| error.to_string())?;

            let mut html = String::with_capacity(body.len() + carried.len() * 32);
            for scope in &carried {
                push_open_tag(&mut html, *scope);
            }
            // `body` never contains the trailing newline as markup — it is
            // escaped text — but a `<pre>` gets its line breaks from the
            // caller, which knows whether a line-number cell precedes them.
            html.push_str(body.trim_end_matches('\n'));

            let still_open = isize::try_from(carried.len()).unwrap_or(isize::MAX) + delta;
            for _ in 0..still_open.max(0) {
                html.push_str("</span>");
            }
            lines.push(html);
        }

        Ok(lines)
    }
}

/// Translates a Pygments lexer alias to the name the bundled grammars use.
///
/// Pygments and Sublime name the same language differently often enough to
/// matter: measured against the `CPython` corpus, `python3` and `shell` alone
/// accounted for 35 blocks that would otherwise have gone unhighlighted and
/// reported. Kept deliberately small and grounded in names real documents
/// actually use, rather than transcribing Pygments' whole alias table.
fn resolve_alias(name: &str) -> &str {
    match name {
        "python3" | "py3" => "python",
        "shell" | "sh" => "bash",
        "dosbatch" | "batch" => "cmd",
        other => other,
    }
}

/// Whether this is a language the bundled grammars are known not to cover, so
/// the block renders as plain text without a diagnostic.
///
/// Every entry is one of Pygments' *session* or *traceback* lexers: they
/// describe a transcript of an interactive session (a `>>>` prompt and its
/// output, a shell prompt and its output) rather than a language, and the
/// Sublime grammar set has no equivalent for any of them.
///
/// Silence rather than a warning is the point. These are real, correctly
/// spelled languages that this backend simply cannot draw, so telling the
/// author would be reporting our own limitation as their mistake — and,
/// measured against the `CPython` corpus, would have meant 200 warnings a reader
/// could do nothing about. A name that is *not* on this list and has no
/// grammar is still reported, which is what the diagnostic is for.
fn is_unhighlightable(name: &str) -> bool {
    matches!(
        name,
        "pycon" | "pytb" | "py3tb" | "console" | "shell-session" | "doscon" | "ps1con"
    )
}

/// Writes the opening tag for one scope, reproducing exactly what
/// `line_tokens_to_classed_spans` emits for a pushed scope.
///
/// syntect's own `scope_to_classes` is private, so this restates it: a scope's
/// class list is its dot-separated atoms, each carrying the prefix. Keeping
/// the two in step matters — a reopened tag whose classes differed from the
/// original would style the continuation of a string differently from its
/// first line.
fn push_open_tag(html: &mut String, scope: syntect::parsing::Scope) {
    html.push_str("<span class=\"");
    let name = scope.build_string();
    for (index, atom) in name.split('.').enumerate() {
        if index > 0 {
            html.push(' ');
        }
        html.push_str(CLASS_PREFIX);
        html.push_str(atom);
    }
    html.push_str("\">");
}

/// The CSS class the block's wrapper carries, naming the language so a theme
/// can style one language differently.
#[must_use]
pub(crate) fn language_class(language: &ResolvedLanguage) -> String {
    format!("highlight-{}", language.css_suffix())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::LanguageName;

    fn named(name: &str) -> ResolvedLanguage {
        ResolvedLanguage::Named(LanguageName::new(name).unwrap())
    }

    #[test]
    fn test_highlight_emits_prefixed_classes() {
        // Given
        let highlighter = Highlighter::new();

        // When
        let lines = highlighter
            .highlight("x = 1", &named("python"))
            .expect("python is a bundled grammar")
            .expect("a named language is highlighted");

        // Then — every class carries the namespace prefix
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("hl-source hl-python"), "{}", lines[0]);
    }

    #[test]
    fn test_highlight_returns_one_entry_per_line() {
        // Given — the property `:linenos:` depends on
        let highlighter = Highlighter::new();

        // When
        let lines = highlighter
            .highlight("a = 1\nb = 2\nc = 3", &named("python"))
            .unwrap()
            .unwrap();

        // Then
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_highlight_balances_spans_within_every_line() {
        // Given — a triple-quoted string keeps a scope open across lines
        let highlighter = Highlighter::new();

        // When
        let lines = highlighter
            .highlight("x = \"\"\"one\ntwo\nthree\"\"\"", &named("python"))
            .unwrap()
            .unwrap();

        // Then — each line stands alone, so per-line markup can wrap it
        for line in &lines {
            assert_eq!(
                line.matches("<span").count(),
                line.matches("</span>").count(),
                "unbalanced spans in:\n{line}"
            );
        }
    }

    #[test]
    fn test_highlight_reopens_a_carried_scope_with_the_same_classes() {
        // Given — the continuation of a string must be styled like its start
        let highlighter = Highlighter::new();

        // When
        let lines = highlighter
            .highlight("x = \"\"\"one\ntwo\"\"\"", &named("python"))
            .unwrap()
            .unwrap();

        // Then — line two reopens the string scope rather than losing it
        assert!(
            lines[1].contains("hl-string hl-quoted hl-double hl-block hl-python"),
            "{}",
            lines[1]
        );
    }

    #[test]
    fn test_highlight_escapes_html_in_the_source() {
        // Given — source that would otherwise become markup
        let highlighter = Highlighter::new();

        // When
        let lines = highlighter
            .highlight("a < b", &named("python"))
            .unwrap()
            .unwrap();

        // Then
        assert!(lines[0].contains("&lt;"), "{}", lines[0]);
        assert!(!lines[0].contains("a < b"), "{}", lines[0]);
    }

    #[test]
    fn test_highlight_returns_nothing_to_do_for_none() {
        // Given
        let highlighter = Highlighter::new();

        // When
        let result = highlighter.highlight("x = 1", &ResolvedLanguage::None);

        // Then — `Ok(None)` is "nothing to highlight", not a failure
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn test_highlight_reports_an_unknown_language() {
        // Given
        let highlighter = Highlighter::new();

        // When
        let error = highlighter
            .highlight("x = 1", &named("nonesuch-language"))
            .expect_err("an unknown language is an error");

        // Then
        assert_eq!(error.kind, HighlightErrorKind::UnknownLanguage);
        assert_eq!(error.code(), DiagnosticCode::CodeBlockUnknownLanguage);
        assert!(error.message.contains("nonesuch-language"), "{error:?}");
    }

    #[test]
    fn test_highlight_degrades_quietly_for_default() {
        // Given — `default` means "Python if it fits"
        let highlighter = Highlighter::new();

        // When
        let result = highlighter.highlight("x = 1", &ResolvedLanguage::Default);

        // Then — it fits, so it is highlighted
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_highlight_accepts_an_extension_alias() {
        // Given — Sphinx authors write both `python` and `py`
        let highlighter = Highlighter::new();

        // When
        let by_name = highlighter.highlight("x = 1", &named("python")).unwrap();
        let by_alias = highlighter.highlight("x = 1", &named("py")).unwrap();

        // Then
        assert_eq!(by_name, by_alias);
    }

    #[test]
    fn test_highlight_handles_a_language_whose_name_has_punctuation() {
        // Given — the name normalization must not have mangled `c++`
        let highlighter = Highlighter::new();

        // When
        let lines = highlighter.highlight("int x = 1;", &named("c++")).unwrap();

        // Then
        assert!(lines.is_some(), "c++ should resolve to a grammar");
    }

    #[test]
    fn test_highlight_produces_no_lines_for_empty_source() {
        // Given
        let highlighter = Highlighter::new();

        // When
        let lines = highlighter
            .highlight("", &named("python"))
            .unwrap()
            .unwrap();

        // Then — `split_inclusive` yields nothing for an empty string
        assert!(lines.is_empty());
    }

    #[test]
    fn test_language_class_names_each_kind_of_language() {
        // Given / When / Then
        assert_eq!(language_class(&named("python")), "highlight-python");
        assert_eq!(language_class(&ResolvedLanguage::None), "highlight-none");
        assert_eq!(
            language_class(&ResolvedLanguage::Default),
            "highlight-default"
        );
    }

    #[test]
    fn test_push_open_tag_matches_syntects_own_class_list() {
        // Given — a scope with several atoms
        let scope = syntect::parsing::Scope::new("source.python").unwrap();

        // When
        let mut html = String::new();
        push_open_tag(&mut html, scope);

        // Then — one prefixed class per atom, space separated, exactly as
        // syntect's own (private) `scope_to_classes` writes it
        assert_eq!(html, "<span class=\"hl-source hl-python\">");
    }

    #[test]
    fn test_syntax_set_is_loaded_once() {
        // Given / When — two calls
        let first = syntax_set();
        let second = syntax_set();

        // Then — the same set, so no document pays the decode twice
        assert!(std::ptr::eq(first, second));
    }
}

/// The theme the checked-in token colours are generated from.
///
/// A dark palette, because `.document pre` has always had a dark background.
/// Base16 Ocean Dark over the other bundled dark themes for its size: it emits
/// roughly a fifth of the CSS Nord does, and a stylesheet every page loads is
/// not the place to spend 16 KB on colours a reader cannot tell apart.
#[cfg(test)]
const THEME: two_face::theme::EmbeddedThemeName =
    two_face::theme::EmbeddedThemeName::Base16OceanDark;

/// The token-colour CSS for [`THEME`], as it must appear in
/// `assets/default.css`.
///
/// Generating this rather than hand-writing it is the point: the class names
/// come from the same `ClassStyle` the renderer emits with, so the two cannot
/// drift into disagreeing about what a token is called. `css_drift_test`
/// compares this against the checked-in copy.
#[cfg(test)]
pub(crate) fn generated_theme_css() -> String {
    let themes = two_face::theme::extra();
    syntect::html::css_for_theme_with_class_style(themes.get(THEME), CLASS_STYLE)
        .expect("a bundled theme converts to CSS")
}

#[cfg(test)]
mod css_drift_test {
    use super::generated_theme_css;

    /// The markers bracketing the generated block inside `assets/default.css`.
    const BEGIN: &str =
        "/* ── BEGIN generated syntax-highlighting theme ─────────────────────── */";
    const END: &str = "/* ── END generated syntax-highlighting theme ───────────────────────── */";

    fn stylesheet() -> String {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/default.css");
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
    }

    #[test]
    fn test_default_css_carries_the_generated_theme_verbatim() {
        // Given — the checked-in stylesheet and the theme it was generated from
        let css = stylesheet();
        let start = css
            .find(BEGIN)
            .expect("assets/default.css must carry the generated-theme block");
        let end = css.find(END).expect("the block must be closed");
        let checked_in = &css[start + BEGIN.len()..end];

        // When
        let generated = generated_theme_css();

        // Then — regenerate with the same theme and the bytes must match, so
        // a two-face upgrade that changes a colour fails here rather than
        // silently leaving the page styled by a stale palette
        assert_eq!(
            checked_in.trim(),
            generated.trim(),
            "assets/default.css is out of date with the generated theme"
        );
    }

    #[test]
    fn test_generated_theme_css_uses_the_renderers_own_class_names() {
        // Given / When
        let generated = generated_theme_css();

        // Then — the selectors and the emitted classes share one prefix, which
        // is the whole reason this is generated rather than written by hand
        assert!(generated.contains(".hl-"), "{generated}");
    }
}

#[cfg(test)]
mod alias_tests {
    use super::*;
    use rusty_sphinx_ast::LanguageName;

    fn named(name: &str) -> ResolvedLanguage {
        ResolvedLanguage::Named(LanguageName::new(name).unwrap())
    }

    #[test]
    fn test_resolve_alias_maps_pygments_names_onto_bundled_grammars() {
        // Given / When / Then — the aliases the CPython corpus actually uses
        assert_eq!(resolve_alias("python3"), "python");
        assert_eq!(resolve_alias("shell"), "bash");
        assert_eq!(resolve_alias("batch"), "cmd");
    }

    #[test]
    fn test_resolve_alias_leaves_an_unaliased_name_alone() {
        // Given / When / Then
        assert_eq!(resolve_alias("rust"), "rust");
    }

    #[test]
    fn test_aliased_languages_actually_resolve_to_a_grammar() {
        // Given — an alias table is only worth having if its targets exist,
        // so this asserts against the real grammar set rather than the table
        let highlighter = Highlighter::new();

        // When / Then
        for alias in ["python3", "py3", "shell", "sh", "batch", "dosbatch"] {
            let result = highlighter.highlight("x", &named(alias));
            assert!(
                matches!(result, Ok(Some(_))),
                "'{alias}' should resolve to a grammar, got {result:?}"
            );
        }
    }

    #[test]
    fn test_session_lexers_render_plain_without_a_diagnostic() {
        // Given — real languages this backend cannot draw
        let highlighter = Highlighter::new();

        // When / Then — silently unhighlighted, never reported: the author
        // spelled a correct language and can do nothing about our gap
        for name in ["pycon", "console", "shell-session", "doscon", "pytb"] {
            let result = highlighter.highlight(">>> 1", &named(name));
            assert_eq!(result, Ok(None), "'{name}' should degrade silently");
        }
    }

    #[test]
    fn test_a_genuinely_unknown_language_is_still_reported() {
        // Given — the diagnostic must survive the narrowing above
        let highlighter = Highlighter::new();

        // When
        let result = highlighter.highlight("x", &named("nonesuch-language"));

        // Then
        assert!(result.is_err(), "{result:?}");
    }
}
