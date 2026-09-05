//! Recognizing a directive at the head of a block and routing it to the
//! parser for its kind — the domain-object family via [`super::domains`],
//! everything else by directive name.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::indent_width;

use super::admonitions::{parse_admonition, parse_seealso, parse_version_change};
use super::body::{collect_argument_continuation_lines, collect_directive_body, join_body_lines};
use super::code_block::{parse_code_block, parse_highlight, parse_literal_include};
use super::data_table::{parse_csv_table, parse_list_table};
use super::doctest::{DocTestDirectiveKind, parse_doctest_directive};
use super::domains::object_type::{DirectiveObjectType, resolve_domain_object_type};
use super::domains::{DirectiveSignatures, parse_domain_object};
use super::glossary::parse_glossary;
use super::image::{parse_figure_directive, parse_image_directive};
use super::include::parse_include;
use super::index_directive::parse_index_directive;
use super::math::parse_math_directive;
use super::scope::try_parse_scope_directive;
use super::substitution::{parse_substitution_definition, split_substitution_marker};
use super::table::parse_table_directive;
use super::toctree::parse_toctree;
use rusty_sphinx_ast::{CodeBlockSource, Directive, Node};

/// The indentation every directive-body parser strips before parsing, so a
/// `ParseCtx` can be shifted by the same amount.
///
/// Mirrors [`crate::indent::unindent_body_lines`]'s rule exactly — the first
/// non-blank line's indent — because that is the function whose effect this
/// compensates for. Without it every position inside a directive body would
/// be short by the body's indent.
fn body_indent(body_lines: &[&str]) -> usize {
    body_lines
        .iter()
        .find(|line| !line.trim().is_empty())
        .map_or(0, |line| indent_width(line))
}

pub(crate) fn try_parse_directive(
    lines: &[&str],
    i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Vec<Node>)> {
    let line = lines[i].trim_end();
    if !(line.trim().starts_with(".. ") && line.contains("::")) {
        return None;
    }

    let trimmed = line.trim();
    let (name_part, arg_part) = trimmed.split_once("::")?;
    let name = name_part.strip_prefix(".. ")?.trim().to_string();
    let argument = arg_part.trim().to_string();

    let min_indent = indent_width(line);

    // Checked before the generic body collection below, because a domain
    // object splits the lines after its marker differently: any further
    // argument lines are peeled off as extra signatures first, and only what
    // remains is its body. No other directive name can reach this branch —
    // `resolve_domain_object_type` matches a disjoint set of names from the
    // ones handled afterwards.
    if let Some(object_type) = resolve_domain_object_type(&name, ctx.default_domain) {
        let (continuations_consumed, continuations) =
            if object_type_supports_multiple_signatures(object_type) {
                collect_argument_continuation_lines(lines, i + 1, min_indent)
            } else {
                (0, Vec::new())
            };
        let body = collect_directive_body(lines, i + 1 + continuations_consumed, min_indent);
        let body_ctx = ctx.nested(
            i + 1 + continuations_consumed + body.first_line_offset,
            body_indent(&body.lines),
        );
        let domain_object = parse_domain_object(
            object_type,
            DirectiveSignatures {
                argument,
                continuations,
                span: ctx.line_span(i, line),
            },
            &body.lines,
            adornment_order,
            diagnostics,
            &body_ctx,
        );
        return Some((
            1 + continuations_consumed + body.consumed,
            vec![Node::Directive(Directive::DomainObject(domain_object))],
        ));
    }

    let body = collect_directive_body(lines, i + 1, min_indent);
    // Every directive parser below receives a context already positioned at
    // its own body's first line *and* column, so none of them has to know
    // where in the document the directive was written. The column shift
    // matters because the parsers unindent the body before parsing it.
    //
    // The directive's *own* line is therefore no longer reachable from that
    // context, so it is captured here and passed alongside: a directive whose
    // content can still fail after parsing (an image whose bytes never reach
    // the renderer) needs a position to report against, and a bare
    // `.. image:: logo.png` has no body line to borrow one from.
    let directive_span = ctx.line_span(i, line);
    let body_ctx = ctx.nested(i + 1 + body.first_line_offset, body_indent(&body.lines));
    let (consumed_lines, body_lines) = (body.consumed, body.lines);

    // `.. include::` is the one directive that contributes *several* nodes:
    // it splices a file's blocks in where it stands, so the fragment's
    // sections and targets belong to this document rather than nesting inside
    // a container of their own. Every other directive answers with exactly
    // one node, wrapped here.
    if name == "include" {
        let nodes = parse_include(
            &argument,
            &body_lines,
            adornment_order,
            diagnostics,
            &body_ctx,
        );
        return Some((1 + consumed_lines, nodes));
    }

    let node = parse_body_directive(
        name,
        argument,
        directive_span,
        &body_lines,
        adornment_order,
        diagnostics,
        &body_ctx,
    );
    Some((1 + consumed_lines, vec![node]))
}

/// Dispatches every directive whose body is collected the ordinary way — that
/// is, all of them except domain objects, which peel extra signature lines off
/// the argument before their body starts and so are handled by the caller.
///
/// Falls back to [`Directive::Unknown`], which is what the benchmark counts as
/// an unsupported directive.
/// Recognizes and parses a substitution definition's marker, `.. |name|
/// inner::`, which is not a directive name at all — tried before every other
/// name below, which would otherwise match nothing and fall all the way to
/// [`Directive::Unknown`].
fn try_parse_substitution_definition(
    name: &str,
    argument: &str,
    directive_span: Option<rusty_sphinx_ast::Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Node> {
    let (sub_name, inner) = split_substitution_marker(name)?;
    let directive = parse_substitution_definition(
        sub_name,
        inner,
        argument,
        directive_span,
        body_lines,
        diagnostics,
        ctx,
    )?;
    Some(Node::Directive(directive))
}

fn parse_body_directive(
    name: String,
    argument: String,
    directive_span: Option<rusty_sphinx_ast::Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Node {
    if let Some(node) = try_parse_substitution_definition(
        &name,
        &argument,
        directive_span,
        body_lines,
        diagnostics,
        ctx,
    ) {
        return node;
    }
    if name == "toctree" {
        let directive = parse_toctree(body_lines, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if name == "plantuml" {
        let directive = Directive::PlantUml(rusty_sphinx_ast::HashedContent::new(join_body_lines(
            body_lines,
        )));
        return Node::Directive(directive);
    }
    if let Some(directive) = parse_code_family(&name, &argument, body_lines, diagnostics, ctx) {
        return Node::Directive(directive);
    }
    if let Ok(kind) = name.parse::<rusty_sphinx_ast::VersionChangeKind>() {
        let directive = parse_version_change(
            kind,
            argument,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        );
        return Node::Directive(directive);
    }
    if name == "seealso" {
        let directive = parse_seealso(body_lines, adornment_order, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if let Ok(kind) = name.parse::<rusty_sphinx_ast::AdmonitionKind>() {
        let directive = parse_admonition(
            kind,
            argument,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        );
        return Node::Directive(directive);
    }
    if name == "glossary" {
        let directive = parse_glossary(body_lines, adornment_order, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if name == "list-table" {
        let directive = parse_list_table(argument, body_lines, adornment_order, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if name == "csv-table" {
        let directive = parse_csv_table(argument, body_lines, adornment_order, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if let Some(directive) = parse_image_family(
        &name,
        &argument,
        directive_span,
        body_lines,
        adornment_order,
        diagnostics,
        ctx,
    ) {
        return Node::Directive(directive);
    }
    if name == "math" {
        let directive = parse_math_directive(argument, body_lines, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if name == "table" {
        let directive =
            parse_table_directive(argument, body_lines, adornment_order, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if name == "index" {
        let directive = parse_index_directive(&argument, body_lines, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if let Some(kind) = DocTestDirectiveKind::from_name(&name) {
        let directive = parse_doctest_directive(kind, &argument, body_lines, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if let Some(directive) = try_parse_scope_directive(&name, &argument, ctx.default_domain) {
        return Node::Directive(directive);
    }
    let directive = Directive::Unknown {
        name,
        argument,
        body: join_body_lines(body_lines),
    };
    Node::Directive(directive)
}

/// Whether a directive of this object type may declare more than one
/// signature, as several argument lines below its marker.
///
/// Every object type may except `py:module`: real Sphinx's `module`
/// directive takes exactly one argument, so a line below it is body content
/// even when it looks like a further name.
const fn object_type_supports_multiple_signatures(object_type: DirectiveObjectType) -> bool {
    !matches!(object_type, DirectiveObjectType::PyModule)
}

/// Parses `.. image::` or `.. figure::`, or `None` when `name` is neither.
///
/// Grouped the same way [`parse_code_family`] groups its four, purely to keep
/// [`parse_body_directive`]'s own dispatch chain from growing past a readable
/// length as directive families accumulate.
fn parse_image_family(
    name: &str,
    argument: &str,
    directive_span: Option<rusty_sphinx_ast::Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Directive> {
    match name {
        "image" => Some(parse_image_directive(
            argument,
            directive_span,
            body_lines,
            diagnostics,
            ctx,
        )),
        "figure" => Some(parse_figure_directive(
            argument,
            directive_span,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        )),
        _ => None,
    }
}

/// Parses the four directives [`super::code_block`] owns, or `None` when
/// `name` is not one of them.
///
/// Grouped into one branch of the dispatcher because they share a module and a
/// vocabulary, and because three of them produce the very same
/// [`Directive::CodeBlock`]: `.. code-block::`, docutils' `.. code::`, and
/// `.. literalinclude::`, which differ only in how their source spells the
/// block. `.. highlight::` rides along as the directive the first three
/// inherit their language from.
fn parse_code_family(
    name: &str,
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Directive> {
    if let Some(source) = code_block_source(name) {
        return Some(parse_code_block(
            source,
            argument,
            body_lines,
            diagnostics,
            ctx,
        ));
    }
    match name {
        "literalinclude" => Some(parse_literal_include(
            argument,
            body_lines,
            diagnostics,
            ctx,
        )),
        "highlight" => Some(parse_highlight(argument, body_lines, diagnostics, ctx)),
        _ => None,
    }
}

/// Which of the two inline code-block directives `name` spells, if either.
///
/// `.. code::` is docutils' name for the same construct; both lower to one
/// [`Directive::CodeBlock`] carrying a [`CodeBlockSource`].
fn code_block_source(name: &str) -> Option<CodeBlockSource> {
    match name {
        "code-block" => Some(CodeBlockSource::CodeBlock),
        "code" => Some(CodeBlockSource::Code),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::CodeLanguage;
    use rusty_sphinx_ast::Domain;
    use rusty_sphinx_ast::HashedContent;

    /// A plain toctree document entry spanning the whole of source line
    /// `line`, which is the shape `parse_toctree` produces for an unindented
    /// entry written under a top-level directive.
    fn toc_document_entry(docname: &str, line: u32) -> rusty_sphinx_ast::TocEntry {
        let column_end = u32::try_from(docname.chars().count()).unwrap_or(0) + 4;
        rusty_sphinx_ast::TocEntry::Document {
            title: None,
            docname: docname.to_string(),
            span: Some(rusty_sphinx_ast::Span::new(
                rusty_sphinx_ast::Position::new(line, 4),
                rusty_sphinx_ast::Position::new(line, column_end),
            )),
        }
    }

    /// Dispatches `name`/`argument`/`body` through [`parse_body_directive`]
    /// with throwaway state, returning the node and any diagnostics.
    fn dispatch(name: &str, argument: &str, body: &[&str]) -> (Node, Diagnostics) {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();
        let node = parse_body_directive(
            name.to_string(),
            argument.to_string(),
            None,
            body,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        (node, diagnostics)
    }

    #[test]
    fn test_parse_body_directive_dispatches_a_known_directive() {
        // Given
        let body = ["   Careful."];

        // When
        let (node, _) = dispatch("note", "", &body);

        // Then
        assert!(matches!(
            node,
            Node::Directive(Directive::Admonition { .. })
        ));
    }

    #[test]
    fn test_parse_body_directive_dispatches_the_doctest_family() {
        // Given
        let body = ["   >>> 1"];

        // When
        let (node, _) = dispatch("doctest", "", &body);

        // Then
        assert!(matches!(node, Node::Directive(Directive::DocTest(_))));
    }

    #[test]
    fn test_parse_body_directive_routes_both_code_block_spellings() {
        // Given — Sphinx's name and docutils' name for one construct
        let body = ["   print(1)"];

        // When
        let (sphinx_spelling, _) = dispatch("code-block", "python", &body);
        let (docutils_spelling, _) = dispatch("code", "python", &body);

        // Then — both reach the same variant, tagged with who wrote them
        let Node::Directive(Directive::CodeBlock(sphinx)) = sphinx_spelling else {
            panic!("Expected CodeBlock, got {sphinx_spelling:?}");
        };
        let Node::Directive(Directive::CodeBlock(docutils)) = docutils_spelling else {
            panic!("Expected CodeBlock, got {docutils_spelling:?}");
        };
        assert_eq!(sphinx.source, CodeBlockSource::CodeBlock);
        assert_eq!(docutils.source, CodeBlockSource::Code);
    }

    #[test]
    fn test_parse_body_directive_falls_back_to_unknown() {
        // Given — this is what the benchmark counts as unsupported.
        let body = ["   content"];

        // When
        let (node, _) = dispatch("not-a-real-directive", "arg", &body);

        // Then
        match node {
            Node::Directive(Directive::Unknown { name, argument, .. }) => {
                assert_eq!(name, "not-a-real-directive");
                assert_eq!(argument, "arg");
            }
            other => panic!("expected an unknown directive, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_body_directive_forwards_diagnostics() {
        // Given
        let body = ["   :bogus:", "", "   >>> 1"];

        // When
        let (_, diagnostics) = dispatch("doctest", "", &body);

        // Then
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_parse_creates_admonition() {
        // Given
        let input = ".. note::\n\n   This is a note.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition {
            kind, title, body, ..
        }) = &doc.nodes[0]
        {
            assert_eq!(kind, &rusty_sphinx_ast::AdmonitionKind::Note);
            assert_eq!(title, &None);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_admonition_with_title() {
        // Given
        let input = ".. admonition:: My Title\n\n   Custom content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { kind, title, .. }) = &doc.nodes[0] {
            assert_eq!(kind, &rusty_sphinx_ast::AdmonitionKind::Admonition);
            assert_eq!(title, &Some("My Title".to_string()));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_collapsible_admonition() {
        // Given
        let input = ".. note::\n   :collapsible:\n\n   Content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { collapsible, .. }) = &doc.nodes[0] {
            assert_eq!(collapsible, &Some(false));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_collapsible_open_admonition() {
        // Given
        let input = ".. note::\n   :collapsible: open\n\n   Content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { collapsible, .. }) = &doc.nodes[0] {
            assert_eq!(collapsible, &Some(true));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_directive() {
        // Given
        let input = ".. toctree::\n   \n   team_a/index\n   team_b/index\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Toctree(rusty_sphinx_ast::Toctree {
                entries: vec![
                    toc_document_entry("team_a/index", 3),
                    toc_document_entry("team_b/index", 4),
                ],
                options: rusty_sphinx_ast::ToctreeOptions::default(),
            }))
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                "Next Para".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_creates_directive_with_maxdepth() {
        // Given
        let input =
            ".. toctree::\n   :maxdepth: 2\n   \n   team_a/index\n   team_b/index\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Toctree(rusty_sphinx_ast::Toctree {
                entries: vec![
                    toc_document_entry("team_a/index", 4),
                    toc_document_entry("team_b/index", 5),
                ],
                options: rusty_sphinx_ast::ToctreeOptions {
                    maxdepth: std::num::NonZeroUsize::new(2),
                    ..rusty_sphinx_ast::ToctreeOptions::default()
                },
            }))
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                "Next Para".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_directive_with_argument_and_trailing_indents() {
        // Given
        let input = ".. code-block:: rust\n\n   let x = 1;\n   \n   let y = 2;\n\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        let Node::Directive(Directive::CodeBlock(block)) = &doc.nodes[0] else {
            panic!("Expected CodeBlock, got {:?}", doc.nodes[0]);
        };
        assert_eq!(block.language, CodeLanguage::parse("rust"));
        assert_eq!(block.content, "let x = 1;\n\nlet y = 2;");
    }

    #[test]
    fn test_parse_creates_index_directive() {
        // Given
        let input = ".. index:: single: execution\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        if let Node::Directive(Directive::Index { entries, .. }) = &doc.nodes[0] {
            assert_eq!(entries.len(), 1);
        } else {
            panic!("Expected Index directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_plantuml_directive_with_hash() {
        // Given
        let input = ".. plantuml::\n\n   A -> B\n   B -> C\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);

        let expected = HashedContent::new("A -> B\nB -> C".to_string());
        assert_eq!(doc.nodes[0], Node::Directive(Directive::PlantUml(expected)));
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                "Next Para".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_code_block_directive_with_language() {
        // Given
        let input = ".. code-block:: python

    x = 1
";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        let Node::Directive(Directive::CodeBlock(block)) = &doc.nodes[0] else {
            panic!("Expected CodeBlock, got {:?}", doc.nodes[0]);
        };
        assert_eq!(block.language, CodeLanguage::parse("python"));
        assert_eq!(block.content, "x = 1");
    }

    #[test]
    fn test_parse_code_block_directive_without_language() {
        // Given
        let input = ".. code-block::

    x = 1
";

        // When
        let doc = parse("test.rst", input);

        // Then — an argumentless block inherits rather than naming nothing
        assert_eq!(doc.nodes.len(), 1);
        let Node::Directive(Directive::CodeBlock(block)) = &doc.nodes[0] else {
            panic!("Expected CodeBlock, got {:?}", doc.nodes[0]);
        };
        assert_eq!(block.language, CodeLanguage::Inherit);
        assert_eq!(block.content, "x = 1");
    }
}
