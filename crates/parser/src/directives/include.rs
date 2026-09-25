//! `.. include::` — splicing another file's reStructuredText into this one.
//!
//! The only directive that yields *several* nodes, and that is the whole point
//! of it: an include is a source-level transclusion, not a container. Sections,
//! hyperlink targets, `.. toctree::` entries and index entries inside the
//! included file belong to the *including* document, exactly as if the author
//! had typed them. Wrapping the result in a node of its own would break every
//! one of those — section nesting first.
//!
//! That is also why inclusion happens here, while parsing, rather than being
//! left as a marker for a later phase to resolve. By the time the `.ast` is
//! written there is nothing left to say that the text came from elsewhere,
//! save the [`Span::file`](rinx_ast::Span::file) every node inside it
//! carries — which is what lets a diagnostic name the fragment rather than the
//! document, at parse time and at render time alike.
//!
//! The cost is that there is no cache firewall: an included file's bytes are
//! genuinely part of the including document, so editing it must re-parse and
//! re-render every page that includes it. That is correct rather than a gap.

use rinx_ast::{
    CodeBlock, CodeBlockSource, CodeLanguage, Diagnostic, DiagnosticCode, Directive, Node, Span,
};

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::body::body_span;
use crate::directives::error_node::{malformed_directive, malformed_node};
use crate::directives::options::{OptionLine, report_unknown_options, scan_option_lines};
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;

use super::code_block::{Selection, check_encoding, expand_tabs};

/// The directive name, used throughout this module's diagnostics.
const DIRECTIVE: &str = "include";

/// How deep a chain of includes the parser will follow.
///
/// Well past anything a real document nests — shared fragments are two or
/// three deep at most — so hitting it means something is wrong that a cycle
/// check did not catch, such as a directory of files each including the next.
/// A limit rather than unbounded recursion because the parser must not be able
/// to overflow its stack on an input it was handed.
const MAX_DEPTH: usize = 16;

/// The options `.. include::` accepts, as written.
#[derive(Default)]
struct IncludeOptions {
    encoding: Option<String>,
    tab_width: Option<i32>,
    literal: bool,
    code: Option<String>,
    number_lines: bool,
    classes: Vec<String>,
    name: Option<String>,
    selection: Selection,
}

/// Parses a `.. include::`, returning the nodes its file contributed.
///
/// Returns a single degraded [`Node::Directive`] holding
/// [`rinx_ast::Directive::Malformed`] when the file could not be read
/// or the selection matched nothing, having reported why — the parser stays
/// resilient, and the `parse` subcommand turns the loader's recorded failure
/// into a failed build.
pub(in crate::directives) fn parse_include(
    argument: &str,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<Node> {
    let span = body_span(body_lines, ctx).or_else(|| ctx.line_span(0, ""));

    let path = argument.trim();
    if path.is_empty() {
        return vec![malformed(
            argument,
            body_lines,
            DiagnosticCode::IncludeMissingPath,
            format!("{DIRECTIVE}: needs the path of a file to include"),
            span,
            diagnostics,
        )];
    }

    let unindented = unindent_body_lines(body_lines);
    let (option_lines, _) = scan_option_lines(&unindented);
    let (options, unclaimed) = parse_options(&option_lines, diagnostics, ctx);
    report_unknown_options(
        &unclaimed,
        DIRECTIVE,
        DiagnosticCode::IncludeUnknownOption,
        diagnostics,
        ctx,
    );

    if !check_encoding(options.encoding.as_deref(), DIRECTIVE, diagnostics, span) {
        return vec![contributed_nothing(argument, body_lines)];
    }
    if ctx.include_stack().len() >= MAX_DEPTH {
        return vec![malformed(
            argument,
            body_lines,
            DiagnosticCode::IncludeDepthExceeded,
            format!(
                "{DIRECTIVE}: includes nested more than {MAX_DEPTH} deep; \
                 '{path}' was not read"
            ),
            span,
            diagnostics,
        )];
    }

    let file = match ctx.files.load(path, ctx.current_file()) {
        Ok(file) => file,
        Err(message) => {
            return vec![malformed(
                argument,
                body_lines,
                DiagnosticCode::IncludeFileUnreadable,
                format!("{DIRECTIVE}: {message}"),
                span,
                diagnostics,
            )];
        }
    };

    if let Some(chain) = closes_a_cycle(ctx, &file.id) {
        return vec![malformed(
            argument,
            body_lines,
            DiagnosticCode::IncludeCycle,
            format!("{DIRECTIVE}: '{path}' includes itself: {chain}"),
            span,
            diagnostics,
        )];
    }

    let text = options
        .tab_width
        .map_or_else(|| file.text.clone(), |width| expand_tabs(&file.text, width));
    let Some(text) = select(&options, &text, diagnostics, span) else {
        return vec![contributed_nothing(argument, body_lines)];
    };

    if options.literal || options.code.is_some() {
        return vec![Node::Directive(as_code_block(&options, &text, span))];
    }

    // The included text is parsed under a context that starts again at line 1
    // and stamps this file onto every span it builds, so a diagnostic from
    // inside the fragment names the fragment. `adornment_order` is *not*
    // reset: an included section heading takes its level from the document's
    // own adornment sequence, which is what makes transclusion transparent.
    let file_id = diagnostics.intern_source_file(&file.id);
    let mut stack: Vec<String> = ctx.include_stack().to_vec();
    stack.push(file.id.clone());
    let included_ctx = ctx.included(file_id, &file.id, &stack);
    let lines: Vec<&str> = text.lines().collect();
    parse_blocks(&lines, adornment_order, diagnostics, &included_ctx)
}

/// The include chain ending at `id` when `id` is already being included, or
/// `None` when it is not.
///
/// The chain is rendered for the message because the offending edit may be in
/// any link of it, and an author looking at the innermost file has no way to
/// see how they got there.
fn closes_a_cycle(ctx: &ParseCtx<'_>, id: &str) -> Option<String> {
    let stack = ctx.include_stack();
    if !stack.iter().any(|open| open == id) {
        return None;
    }
    let mut chain: Vec<&str> = stack.iter().map(String::as_str).collect();
    chain.push(id);
    Some(chain.join(" -> "))
}

/// Applies the selection options, or returns the whole file when none were
/// written.
fn select(
    options: &IncludeOptions,
    text: &str,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> Option<String> {
    if options.selection.is_empty() {
        return Some(text.to_string());
    }
    options
        .selection
        .apply(text, DIRECTIVE, diagnostics, span)
        .map(|selected| selected.text)
}

/// The node a `:literal:` or `:code:` include produces: the file's text shown
/// verbatim rather than parsed as reStructuredText.
///
/// Both lower to the same [`Directive::CodeBlock`] `.. literalinclude::` does,
/// which is what they are — docutils simply spells the same intent as an
/// option on `.. include::` rather than as a directive of its own.
fn as_code_block(options: &IncludeOptions, content: &str, span: Option<Span>) -> Directive {
    Directive::CodeBlock(CodeBlock {
        source: CodeBlockSource::LiteralInclude,
        language: options
            .code
            .as_deref()
            .filter(|name| !name.is_empty())
            .map_or(CodeLanguage::Inherit, CodeLanguage::parse),
        content: content.trim_end_matches('\n').to_string(),
        caption: None,
        name: options.name.as_deref().map(rinx_ast::TargetName::new),
        classes: options.classes.clone(),
        linenos: options.number_lines,
        lineno_start: None,
        emphasize_lines: Vec::new(),
        force: false,
        span,
    })
}

/// Consumes the options this directive knows, returning them with the lines it
/// did not recognize.
fn parse_options<'a>(
    option_lines: &'a [OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (IncludeOptions, Vec<&'a OptionLine>) {
    let mut options = IncludeOptions::default();
    let mut unclaimed = Vec::new();

    for line in option_lines {
        match line.name.as_str() {
            "encoding" => options.encoding = Some(line.value.clone()),
            "literal" => options.literal = true,
            "code" => options.code = Some(line.value.clone()),
            "number-lines" => options.number_lines = true,
            "class" => {
                options.classes = line.value.split_whitespace().map(str::to_string).collect();
            }
            "name" => options.name = Some(line.value.clone()),
            "start-after" => options.selection.start_after = Some(line.value.clone()),
            "end-before" => options.selection.end_before = Some(line.value.clone()),
            "start-line" => {
                options.selection.start_line = parse_index(line, "start-line", diagnostics, ctx);
            }
            "end-line" => {
                options.selection.end_line = parse_index(line, "end-line", diagnostics, ctx);
            }
            "tab-width" => {
                let Ok(width) = line.value.trim().parse::<i32>() else {
                    diagnostics.push(Diagnostic::at(
                        DiagnosticCode::IncludeInvalidTabWidth,
                        format!(
                            "{DIRECTIVE}: :tab-width: needs an integer, got '{}'",
                            line.value
                        ),
                        ctx.line_span(line.line_index, &line.raw),
                    ));
                    continue;
                };
                options.tab_width = Some(width);
            }
            _ => unclaimed.push(line),
        }
    }
    (options, unclaimed)
}

/// Reads a `:start-line:`/`:end-line:` value: a **0-based** index, which is
/// docutils' choice and the one place in reST that counts from zero.
fn parse_index(
    line: &OptionLine,
    option: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<usize> {
    let Ok(index) = line.value.trim().parse::<usize>() else {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::IncludeInvalidLineRange,
            format!(
                "{DIRECTIVE}: :{option}: needs a non-negative line index, got '{}'",
                line.value
            ),
            ctx.line_span(line.line_index, &line.raw),
        ));
        return None;
    };
    Some(index)
}

/// The degraded node for an include that produced nothing, reporting `code`
/// and `message` as the reason.
fn malformed(
    argument: &str,
    body_lines: &[&str],
    code: DiagnosticCode,
    message: String,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) -> Node {
    Node::Directive(malformed_directive(
        DIRECTIVE,
        argument,
        body_lines,
        code,
        message,
        span,
        diagnostics,
    ))
}

/// The degraded node for an include whose reason a shared checker
/// (`check_encoding`, `select`) has already reported — see
/// [`malformed_node`]'s own doc comment for why this second spelling exists.
fn contributed_nothing(argument: &str, body_lines: &[&str]) -> Node {
    Node::Directive(malformed_node(
        DIRECTIVE,
        argument,
        body_lines,
        format!("{DIRECTIVE}: '{}' contributed no content", argument.trim()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{LoadedFile, ParseFileLoader};
    use rinx_ast::{Domain, FileId, InlineNode};
    use std::collections::HashMap;

    /// An in-memory stand-in for the worker's filesystem loader.
    ///
    /// Resolution is deliberately trivial — the written path *is* the id — so
    /// these tests exercise this module's logic rather than the loader's path
    /// arithmetic, which `parse_files.rs` covers on its own.
    struct FakeFiles(HashMap<String, String>);

    impl FakeFiles {
        fn with(files: &[(&str, &str)]) -> Self {
            Self(
                files
                    .iter()
                    .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
                    .collect(),
            )
        }
    }

    impl ParseFileLoader for FakeFiles {
        fn load(&self, path: &str, _relative_to: Option<&str>) -> Result<LoadedFile, String> {
            self.0
                .get(path)
                .map(|text| LoadedFile {
                    id: path.to_string(),
                    text: text.clone(),
                })
                .ok_or_else(|| format!("cannot read '{path}': no such file"))
        }
    }

    /// Parses a whole document through the public entry point, so the
    /// multi-node splice is exercised the way the block loop drives it.
    fn parse_document(files: &[(&str, &str)], rst: &str) -> rinx_ast::Document {
        let loader = FakeFiles::with(files);
        let ctx = ParseCtx::new(Domain::Py, &loader);
        crate::parse_with_ctx("guide.rst", rst, &ctx)
    }

    fn codes(document: &rinx_ast::Document) -> Vec<DiagnosticCode> {
        document.diagnostics.iter().map(|d| d.code).collect()
    }

    /// The text of every paragraph in `nodes`, for asserting on spliced prose.
    fn paragraphs(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .filter_map(|node| match node {
                Node::Paragraph(inline) => Some(
                    inline
                        .iter()
                        .map(|part| match part {
                            InlineNode::Text(text) => text.clone(),
                            other => format!("{other:?}"),
                        })
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_an_included_paragraph_becomes_a_node_of_the_including_document() {
        // Given
        let document = parse_document(
            &[("shared.rst", "From the fragment.\n")],
            "Before.\n\n.. include:: shared.rst\n\nAfter.\n",
        );

        // Then — spliced in place, not wrapped in a container
        assert_eq!(
            paragraphs(&document.nodes),
            vec![
                "Before.".to_string(),
                "From the fragment.".to_string(),
                "After.".to_string()
            ]
        );
        assert!(document.diagnostics.is_empty());
    }

    #[test]
    fn test_an_included_file_contributes_several_nodes() {
        // Given a fragment with two paragraphs
        let document = parse_document(
            &[("shared.rst", "One.\n\nTwo.\n")],
            ".. include:: shared.rst\n",
        );

        // Then — both, side by side at the top level
        assert_eq!(
            paragraphs(&document.nodes),
            vec!["One.".to_string(), "Two.".to_string()]
        );
    }

    #[test]
    fn test_an_included_heading_joins_the_documents_own_section_structure() {
        // Given a fragment holding a section
        let document = parse_document(
            &[("shared.rst", "Section\n=======\n\nBody.\n")],
            "Title\n=====\n\n.. include:: shared.rst\n",
        );

        // Then — the fragment's heading takes its level from the document's
        // own adornment order, which is what makes transclusion transparent
        let levels: Vec<u8> = document
            .nodes
            .iter()
            .filter_map(|node| match node {
                Node::Heading { level, .. } => Some(*level),
                _ => None,
            })
            .collect();
        assert_eq!(levels, vec![1, 1]);
    }

    #[test]
    fn test_the_included_file_is_recorded_on_the_document() {
        // Given
        let document = parse_document(&[("shared.rst", "Text.\n")], ".. include:: shared.rst\n");

        // Then — the table a span's `FileId` indexes
        assert_eq!(document.source_files, vec!["shared.rst".to_string()]);
    }

    #[test]
    fn test_a_file_included_twice_is_recorded_once() {
        // Given
        let document = parse_document(
            &[("shared.rst", "Text.\n")],
            ".. include:: shared.rst\n\n.. include:: shared.rst\n",
        );

        // Then — one entry, so spans in the two copies agree on their file
        assert_eq!(document.source_files, vec!["shared.rst".to_string()]);
    }

    #[test]
    fn test_a_diagnostic_from_inside_a_fragment_names_that_fragment() {
        // Given a fragment whose second line is a broken grid table
        let document = parse_document(
            &[("shared.rst", "Text.\n\n+---+\n")],
            "Before.\n\n.. include:: shared.rst\n",
        );

        // Then — the span points into the fragment, not into `guide.rst`
        let diagnostic = document
            .diagnostics
            .first()
            .expect("the malformed table should be reported");
        let span = diagnostic.span.expect("a positioned diagnostic");
        assert_eq!(span.file, Some(FileId::new(0)));
        assert_eq!(document.span_path(Some(span)), Some("shared.rst"));
    }

    #[test]
    fn test_a_diagnostic_from_the_document_itself_carries_no_file() {
        // Given a document that includes something and also has its own fault
        let document = parse_document(
            &[("shared.rst", "Fine.\n")],
            ".. include:: shared.rst\n\n+---+\n",
        );

        // Then — `None` still means "this document"
        let diagnostic = document
            .diagnostics
            .first()
            .expect("the malformed table should be reported");
        assert_eq!(diagnostic.span.expect("a positioned diagnostic").file, None);
        assert_eq!(document.span_path(diagnostic.span), Some("guide.rst"));
    }

    #[test]
    fn test_a_missing_argument_is_reported() {
        // Given / When
        let document = parse_document(&[], ".. include::\n");

        // Then
        assert_eq!(codes(&document), vec![DiagnosticCode::IncludeMissingPath]);
    }

    #[test]
    fn test_an_unreadable_file_is_reported_and_degrades_the_directive() {
        // Given — what an undeclared `parse_data` entry looks like
        let document = parse_document(&[], ".. include:: nowhere.rst\n");

        // Then
        assert_eq!(
            codes(&document),
            vec![DiagnosticCode::IncludeFileUnreadable]
        );
        assert!(matches!(
            document.nodes.first(),
            Some(Node::Directive(Directive::Malformed { .. }))
        ));
    }

    #[test]
    fn test_a_file_that_includes_itself_is_reported_as_a_cycle() {
        // Given
        let document = parse_document(
            &[("loop.rst", "Text.\n\n.. include:: loop.rst\n")],
            ".. include:: loop.rst\n",
        );

        // Then — reported once, at the point the chain closes
        assert_eq!(codes(&document), vec![DiagnosticCode::IncludeCycle]);
    }

    #[test]
    fn test_a_cycle_message_names_the_whole_chain() {
        // Given two files including each other
        let document = parse_document(
            &[
                ("a.rst", ".. include:: b.rst\n"),
                ("b.rst", ".. include:: a.rst\n"),
            ],
            ".. include:: a.rst\n",
        );

        // Then — the offending edit may be in either link, so both are named
        let message = &document.diagnostics[0].message;
        assert!(message.contains("a.rst -> b.rst -> a.rst"), "{message}");
    }

    #[test]
    fn test_including_the_same_file_twice_side_by_side_is_not_a_cycle() {
        // Given — a fragment used twice is ordinary, and only *nesting* loops
        let document = parse_document(
            &[("shared.rst", "Text.\n")],
            ".. include:: shared.rst\n\n.. include:: shared.rst\n",
        );

        // Then
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        assert_eq!(paragraphs(&document.nodes).len(), 2);
    }

    #[test]
    fn test_a_nested_include_is_spliced_too() {
        // Given a fragment that includes a fragment
        let document = parse_document(
            &[
                ("outer.rst", "Outer.\n\n.. include:: inner.rst\n"),
                ("inner.rst", "Inner.\n"),
            ],
            ".. include:: outer.rst\n",
        );

        // Then
        assert_eq!(
            paragraphs(&document.nodes),
            vec!["Outer.".to_string(), "Inner.".to_string()]
        );
    }

    #[test]
    fn test_a_nested_fragments_diagnostics_name_the_inner_file() {
        // Given a fault two levels down
        let document = parse_document(
            &[
                ("outer.rst", ".. include:: inner.rst\n"),
                ("inner.rst", "+---+\n"),
            ],
            ".. include:: outer.rst\n",
        );

        // Then — the innermost file is the one to open
        let span = document.diagnostics[0]
            .span
            .expect("a positioned diagnostic");
        assert_eq!(document.span_path(Some(span)), Some("inner.rst"));
    }

    #[test]
    fn test_start_after_and_end_before_select_part_of_a_fragment() {
        // Given a fragment with markers around the part worth reusing
        let document = parse_document(
            &[("shared.rst", "skip\n\nBEGIN\n\nKeep me.\n\nEND\n")],
            ".. include:: shared.rst\n   :start-after: BEGIN\n   :end-before: END\n",
        );

        // Then
        assert_eq!(paragraphs(&document.nodes), vec!["Keep me.".to_string()]);
    }

    #[test]
    fn test_start_line_and_end_line_are_zero_based() {
        // Given docutils' 0-based, end-exclusive pair
        let document = parse_document(
            &[("shared.rst", "one\n\ntwo\n\nthree\n")],
            ".. include:: shared.rst\n   :start-line: 2\n   :end-line: 3\n",
        );

        // Then — index 2 is the third line, which holds `two`
        assert_eq!(paragraphs(&document.nodes), vec!["two".to_string()]);
    }

    #[test]
    fn test_an_unmatched_start_after_is_reported() {
        // Given / When
        let document = parse_document(
            &[("shared.rst", "Text.\n")],
            ".. include:: shared.rst\n   :start-after: nowhere\n",
        );

        // Then
        assert_eq!(codes(&document), vec![DiagnosticCode::IncludeTextNotFound]);
    }

    #[test]
    fn test_a_literal_include_shows_the_text_verbatim() {
        // Given a fragment whose markup should not be interpreted
        let document = parse_document(
            &[("shared.rst", "*not emphasis*\n")],
            ".. include:: shared.rst\n   :literal:\n",
        );

        // Then — one code block, not a parsed paragraph
        let Some(Node::Directive(Directive::CodeBlock(block))) = document.nodes.first() else {
            panic!("expected a code block, got {:?}", document.nodes);
        };
        assert_eq!(block.content, "*not emphasis*");
    }

    #[test]
    fn test_a_code_include_names_the_language_it_was_given() {
        // Given / When
        let document = parse_document(
            &[("example.py", "x = 1\n")],
            ".. include:: example.py\n   :code: python\n",
        );

        // Then
        let Some(Node::Directive(Directive::CodeBlock(block))) = document.nodes.first() else {
            panic!("expected a code block, got {:?}", document.nodes);
        };
        assert_eq!(block.language, CodeLanguage::parse("python"));
    }

    #[test]
    fn test_number_lines_on_a_literal_include_asks_for_line_numbers() {
        // Given / When
        let document = parse_document(
            &[("shared.rst", "text\n")],
            ".. include:: shared.rst\n   :literal:\n   :number-lines:\n",
        );

        // Then
        let Some(Node::Directive(Directive::CodeBlock(block))) = document.nodes.first() else {
            panic!("expected a code block, got {:?}", document.nodes);
        };
        assert!(block.linenos);
    }

    #[test]
    fn test_a_non_utf8_encoding_is_refused() {
        // Given / When
        let document = parse_document(
            &[("shared.rst", "Text.\n")],
            ".. include:: shared.rst\n   :encoding: latin-1\n",
        );

        // Then
        assert_eq!(
            codes(&document),
            vec![DiagnosticCode::IncludeEncodingUnsupported]
        );
    }

    #[test]
    fn test_tab_width_expands_tabs_before_the_text_is_parsed() {
        // Given a fragment indented with a tab, which would otherwise make a
        // block quote of an unpredictable depth
        let document = parse_document(
            &[("shared.rst", "Text.\n")],
            ".. include:: shared.rst\n   :tab-width: 4\n",
        );

        // Then
        assert_eq!(paragraphs(&document.nodes), vec!["Text.".to_string()]);
    }

    #[test]
    fn test_a_non_numeric_tab_width_is_reported() {
        // Given / When
        let document = parse_document(
            &[("shared.rst", "Text.\n")],
            ".. include:: shared.rst\n   :tab-width: wide\n",
        );

        // Then
        assert_eq!(
            codes(&document),
            vec![DiagnosticCode::IncludeInvalidTabWidth]
        );
    }

    #[test]
    fn test_a_non_numeric_start_line_is_reported() {
        // Given / When
        let document = parse_document(
            &[("shared.rst", "Text.\n")],
            ".. include:: shared.rst\n   :start-line: top\n",
        );

        // Then
        assert_eq!(
            codes(&document),
            vec![DiagnosticCode::IncludeInvalidLineRange]
        );
    }

    #[test]
    fn test_an_unknown_option_is_reported() {
        // Given / When
        let document = parse_document(
            &[("shared.rst", "Text.\n")],
            ".. include:: shared.rst\n   :nonsense: 1\n",
        );

        // Then
        assert_eq!(codes(&document), vec![DiagnosticCode::IncludeUnknownOption]);
    }

    #[test]
    fn test_closes_a_cycle_says_no_for_a_file_not_being_included() {
        // Given a context with nothing open
        let loader = FakeFiles::with(&[]);
        let ctx = ParseCtx::new(Domain::Py, &loader);

        // When / Then
        assert_eq!(closes_a_cycle(&ctx, "shared.rst"), None);
    }
}
