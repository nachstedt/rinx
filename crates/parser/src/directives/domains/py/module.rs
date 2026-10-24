use crate::diagnostics::Diagnostics;
use crate::directives::invocation::DirectiveInvocation;
use crate::directives::options::{DirectiveOptions, classify_options, directive_options};
use crate::directives::spec::{Arguments, DirectiveSpec};
use crate::headings::Adornment;
use rinx_ast::{DomainObjectBody, ModuleFlag, ModuleOptions};

/// Parses a `.. py:module::` body: reads its options (a `:synopsis:` wrapped
/// over several lines is one value), reports any option `py:module` does not
/// take, and parses the rest as the module's body.
pub(crate) fn parse_py_module(
    name: String,
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> DomainObjectBody {
    let mut options = ModuleOptions::default();
    for (option, _line) in classify_options::<ModuleOption>(
        &invocation.options,
        "py:module",
        diagnostics,
        &invocation.ctx,
    ) {
        read_module_option(option, &mut options);
    }

    let body = invocation
        .content_with_enclosing_titles()
        .parse(adornment_order, diagnostics);

    DomainObjectBody::PyModule {
        name,
        options,
        body,
    }
}

directive_options! {
    /// The options `.. py:module::` takes: every one of Sphinx 9.1's
    /// `PyModule.option_spec`.
    enum ModuleOption {
        /// `:platform:`.
        Platform(String) => (["platform"], "The platforms the module is available on."),
        /// `:synopsis:`.
        Synopsis(String) => (["synopsis"], "A one-sentence description of the module."),
        /// `:deprecated:`.
        Deprecated => (["deprecated"], Flag, "Marks the module as deprecated."),
        /// `:no-index:`.
        NoIndex => (["no-index", "noindex"], Flag, "Makes the module neither a reference target nor an index entry."),
        /// `:no-index-entry:`.
        NoIndexEntry => (["no-index-entry"], Flag, "Leaves the module out of the general index."),
        /// `:no-contents-entry:`, which Sphinx accepts on a module but never
        /// reads.
        NoContentsEntry => (["no-contents-entry", "nocontentsentry"], Flag, "Has no effect on a module."),
        /// `:no-typesetting:`, likewise.
        NoTypesetting => (["no-typesetting"], Flag, "Has no effect on a module."),
    }
}

/// Records one option of a `.. py:module::`.
///
/// `:no-contents-entry:` and `:no-typesetting:` are consumed and dropped:
/// Sphinx accepts them on a module but its `run` never reads them, so they
/// have no effect there either.
fn read_module_option(option: ModuleOption, options: &mut ModuleOptions) {
    match option {
        ModuleOption::Platform(value) => options.platform = Some(value),
        ModuleOption::Synopsis(value) => options.synopsis = Some(value),
        ModuleOption::Deprecated => options.set(ModuleFlag::Deprecated),
        ModuleOption::NoIndex => options.set(ModuleFlag::NoIndex),
        ModuleOption::NoIndexEntry => options.set(ModuleFlag::NoIndexEntry),
        ModuleOption::NoContentsEntry | ModuleOption::NoTypesetting => {}
    }
}

/// `.. py:module::`'s entry among the domain objects.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["module"],
    doc: "Describes a Python module and makes it the current one.",
    arguments: Arguments::ONE_WORD,
    takes_content: true,
    content_hint: None,
    options: <ModuleOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directives::options::OptionLine;
    use crate::directives::options::scan_option_lines;
    use crate::parse;
    use rinx_ast::{DiagnosticCode, Directive, Node};

    /// The options `option_lines` read as, and the lines `py:module` does
    /// not take.
    fn read_module_options(option_lines: &[OptionLine]) -> (ModuleOptions, Vec<&OptionLine>) {
        let mut options = ModuleOptions::default();
        let mut unrecognized = Vec::new();
        for line in option_lines {
            match crate::directives::options::read_quietly::<ModuleOption>(line) {
                Some(option) => read_module_option(option, &mut options),
                None => unrecognized.push(line),
            }
        }
        (options, unrecognized)
    }

    /// The option lines of `body`, as `parse_py_module` scans them.
    fn option_lines(body: &[&str]) -> Vec<OptionLine> {
        let lines: Vec<String> = body.iter().map(ToString::to_string).collect();
        scan_option_lines(&lines).0
    }

    /// The single `PyModule` a document parses to.
    fn parse_module(input: &str) -> (String, ModuleOptions, Vec<Node>) {
        let doc = parse("test.rst", input);
        match doc.nodes.first() {
            Some(Node::Directive(Directive::DomainObject {
                object:
                    DomainObjectBody::PyModule {
                        name,
                        options,
                        body,
                        ..
                    },
                ..
            })) => (name.clone(), options.clone(), body.clone()),
            other => panic!("Expected PyModule, got {other:?}"),
        }
    }

    #[test]
    fn test_read_module_options_reads_every_option() {
        // Given
        let lines = option_lines(&[
            ":platform: Unix, Windows",
            ":synopsis: Greeting utilities.",
            ":deprecated:",
            ":no-index-entry:",
        ]);

        // When
        let (options, unrecognized) = read_module_options(&lines);

        // Then
        assert_eq!(options.platform.as_deref(), Some("Unix, Windows"));
        assert_eq!(options.synopsis.as_deref(), Some("Greeting utilities."));
        assert!(options.has(ModuleFlag::Deprecated));
        assert!(options.has(ModuleFlag::NoIndexEntry));
        assert!(!options.has(ModuleFlag::NoIndex));
        assert!(unrecognized.is_empty());
    }

    #[test]
    fn test_read_module_options_accepts_both_no_index_spellings() {
        // Given — CPython writes both, one after the other.
        let lines = option_lines(&[":noindex:", ":no-index:"]);

        // When
        let (options, unrecognized) = read_module_options(&lines);

        // Then
        assert!(options.has(ModuleFlag::NoIndex));
        assert!(unrecognized.is_empty());
    }

    #[test]
    fn test_read_module_options_drops_options_sphinx_never_reads() {
        // Given
        let lines = option_lines(&[
            ":no-contents-entry:",
            ":nocontentsentry:",
            ":no-typesetting:",
        ]);

        // When
        let (options, unrecognized) = read_module_options(&lines);

        // Then — accepted, and without an effect, as in Sphinx.
        assert_eq!(options, ModuleOptions::default());
        assert!(unrecognized.is_empty());
    }

    #[test]
    fn test_read_module_options_returns_an_option_py_module_does_not_take() {
        // Given
        let lines = option_lines(&[":synopsis: Greetings.", ":module: other"]);

        // When
        let (options, unrecognized) = read_module_options(&lines);

        // Then
        assert_eq!(options.synopsis.as_deref(), Some("Greetings."));
        assert_eq!(unrecognized.len(), 1);
        assert_eq!(unrecognized[0].name, "module");
    }

    #[test]
    fn test_parse_creates_py_module_domain_object() {
        // Given
        let input = ".. py:module:: greetings\n\n   A module of greetings.";

        // When
        let (name, options, body) = parse_module(input);

        // Then
        assert_eq!(name, "greetings");
        assert_eq!(options, ModuleOptions::default());
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn test_parse_py_module_with_platform_synopsis_and_deprecated_options() {
        // Given
        let input = ".. py:module:: greetings\n   :platform: Unix, Windows\n   :synopsis: Greeting utilities.\n   :deprecated:\n\n   A module of greetings.";

        // When
        let (_, options, body) = parse_module(input);

        // Then
        assert_eq!(options.platform.as_deref(), Some("Unix, Windows"));
        assert_eq!(options.synopsis.as_deref(), Some("Greeting utilities."));
        assert!(options.has(ModuleFlag::Deprecated));
        assert_eq!(body.len(), 1);
        assert!(matches!(body[0], Node::Paragraph(_)));
    }

    #[test]
    fn test_parse_py_module_options_in_any_order_with_no_body() {
        // Given — synopsis and deprecated before platform, and no docstring body
        let input = ".. py:module:: greetings\n   :synopsis: Greeting utilities.\n   :deprecated:\n   :platform: Unix";

        // When
        let (_, options, body) = parse_module(input);

        // Then
        assert_eq!(options.platform.as_deref(), Some("Unix"));
        assert_eq!(options.synopsis.as_deref(), Some("Greeting utilities."));
        assert!(options.has(ModuleFlag::Deprecated));
        assert!(body.is_empty());
    }

    #[test]
    fn test_parse_py_module_joins_a_synopsis_wrapped_over_several_lines() {
        // Given — as CPython's `zlib.rst` writes it.
        let input = ".. module:: zlib\n   :synopsis: Low-level interface to compression and decompression routines\n              compatible with gzip.\n";

        // When
        let (_, options, body) = parse_module(input);

        // Then — one value, and the continuation is not body text.
        assert_eq!(
            options.synopsis.as_deref(),
            Some(
                "Low-level interface to compression and decompression routines compatible with gzip."
            )
        );
        assert!(body.is_empty());
    }

    #[test]
    fn test_parse_py_module_reads_no_index_after_the_synopsis() {
        // Given — as CPython's `email.compat32-message.rst` writes it.
        let input = ".. module:: email.message\n   :synopsis: The base class representing email messages\n              backward compatible with Python 3.2\n   :noindex:\n   :no-index:\n";

        // When
        let (_, options, body) = parse_module(input);

        // Then
        assert!(options.has(ModuleFlag::NoIndex));
        assert_eq!(
            options.synopsis.as_deref(),
            Some("The base class representing email messages backward compatible with Python 3.2")
        );
        assert!(body.is_empty());
    }

    #[test]
    fn test_parse_py_module_reports_an_unknown_option_instead_of_keeping_it_as_body() {
        // Given
        let input = ".. py:module:: greetings\n   :bogus: yes\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        let diagnostic = doc
            .diagnostics
            .iter()
            .find(|d| d.code == DiagnosticCode::DirectiveUnknownOption)
            .expect("the unknown option is reported");
        assert_eq!(diagnostic.span.expect("with a span").start.line, 2);
        let (_, _, body) = parse_module(input);
        assert!(body.is_empty());
    }

    #[test]
    fn test_a_diagnostic_inside_the_body_points_at_the_line_it_was_written_on() {
        // Given — an unknown option on a nested directive, five lines down,
        // below a two-line option block.
        let input = ".. py:module:: greetings\n   :synopsis: Greetings.\n   :deprecated:\n\n   .. contents::\n      :nope: 1\n";

        // When
        let doc = parse("test.rst", input);

        // Then — the body context was rebased, so the position is the real one
        let span = doc
            .diagnostics
            .iter()
            .find(|d| d.code == DiagnosticCode::DirectiveUnknownOption)
            .and_then(|d| d.span)
            .expect("the nested option is reported with a span");
        assert_eq!(span.start.line, 6);
    }

    #[test]
    fn test_parse_py_module_reports_a_following_line_as_surplus_words() {
        // Given — real Sphinx's `module` directive takes exactly one word, and
        // docutils reads the line directly below the marker as the rest of the
        // argument: "maximum 1 argument(s) allowed"
        let input = ".. py:module:: greetings\n   A module of greetings.";

        // When
        let doc = parse("test.rst", input);
        let (name, _, body) = parse_module(input);

        // Then — reported, and the module still named
        let codes: Vec<DiagnosticCode> = doc.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, [DiagnosticCode::DirectiveTooManyArguments]);
        assert_eq!(name, "greetings");
        assert!(body.is_empty());
    }

    #[test]
    fn test_parse_py_module_reads_its_body_below_a_blank_line() {
        // Given
        let input = ".. py:module:: greetings\n\n   A module of greetings.";

        // When
        let (name, _, body) = parse_module(input);

        // Then
        assert_eq!(name, "greetings");
        assert_eq!(body.len(), 1);
    }
}
