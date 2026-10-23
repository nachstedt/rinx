//! Recognizing a directive at the head of a block and routing it to the
//! parser for its kind — the domain-object family via [`super::domains`],
//! an entity type via the schema, every directive the table in
//! [`super::spec`] spells through its entry there, and what is left by the
//! vocabularies that depend on more than a name.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::indent_width;

use super::body::{MarkerLine, MarkerText, body_indent, collect_directive_body};
use super::domains::object_type::{object_spec, resolve_domain_object_type};
use super::domains::parse_domain_object;
use super::entity::{EntityDirective, parse_entity};
use super::entity_section::{EntitySectionSite, try_parse_entity_section};
use super::entity_spec::EntitySpec;
use super::error_node::unknown_directive;
use super::invocation::ParseState;
use super::reader::read_directive_block;
use super::scope::{parse_scope_directive, resolve_scope_directive};
use super::spec::registered_directive;
use super::substitution::{parse_substitution_definition, split_substitution_marker};
use rinx_ast::{Directive, Node};

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

    let site = DirectiveSite {
        lines,
        index: i,
        line,
        min_indent,
    };

    // A domain object is a *vocabulary* lookup: whether a name is one depends
    // on the default domain. No other directive name can reach this branch —
    // `resolve_domain_object_type` matches a disjoint set of names from the
    // ones handled afterwards.
    if let Some(result) =
        try_parse_domain_object_directive(&name, &site, adornment_order, diagnostics, ctx)
    {
        return Some(result);
    }

    // Checked here rather than through the directive table because an
    // entity type is a *vocabulary* lookup, like a domain object's, not a
    // fixed name — and because the entity parser needs the directive's own
    // line to discriminate a generated id.
    if let Some(result) =
        try_parse_entity_directive(&name, &site, adornment_order, diagnostics, ctx)
    {
        return Some(result);
    }

    let body = collect_directive_body(lines, i + 1, min_indent);
    // The directive's *own* line is reported against by any directive whose
    // content can still fail after parsing (an image whose bytes never reach
    // the renderer), and a bare `.. image:: logo.png` has no body line to
    // borrow a position from.
    let directive_span = ctx.line_span(i, line);
    let (body_lines, first_line_offset) = body.for_directive();
    let marker = MarkerLine {
        index: i,
        span: directive_span,
        argument: MarkerText::after_marker(line),
    };
    let mut state = ParseState {
        adornment_order,
        diagnostics,
    };
    let nodes = if let Some(directive) = registered_directive(&name) {
        let invocation = read_directive_block(
            &name,
            directive.spec,
            &marker,
            &body,
            &body_lines,
            state.diagnostics,
            ctx,
        );
        (directive.interpret)(&invocation, &mut state)
    } else if let Some(scope) = resolve_scope_directive(&name, ctx.default_domain) {
        let invocation = read_directive_block(
            &name,
            scope.spec(),
            &marker,
            &body,
            &body_lines,
            state.diagnostics,
            ctx,
        );
        vec![Node::Directive(parse_scope_directive(scope, &invocation))]
    } else {
        // The names whose meaning depends on more than the name still read
        // their own body, under a context positioned at its first line and
        // column, taking no section titles — docutils' default for a nested
        // parse.
        let body_ctx = ctx
            .nested(i + 1 + first_line_offset, body_indent(&body.lines))
            .without_section_titles();
        vec![parse_vocabulary_directive(
            &VocabularyDirective {
                name: &name,
                argument: &argument,
                span: directive_span,
                body_lines: &body_lines,
                ctx: &body_ctx,
            },
            &mut state,
        )]
    };
    Some((1 + body.consumed, nodes))
}

/// A directive the table has no entry for, as written.
struct VocabularyDirective<'a, 'c> {
    /// The name, as written before the `::`.
    name: &'a str,
    /// What follows the `::`, trimmed.
    argument: &'a str,
    /// The whole marker line.
    span: Option<rinx_ast::Span>,
    /// The body's lines, their indentation intact.
    body_lines: &'a [&'a str],
    /// The context positioned at the body's first line and column.
    ctx: &'a ParseCtx<'c>,
}

/// Parses a directive the table has no entry for: one whose meaning depends
/// on more than its name — a substitution definition, an entity section the
/// schema declares — or else an unknown one.
///
/// Falls back to [`Directive::Unknown`] — reported as `directive.unknown` and
/// drawn as a visible error block, and what the benchmark counts as an
/// unsupported directive.
fn parse_vocabulary_directive(
    directive: &VocabularyDirective<'_, '_>,
    state: &mut ParseState<'_>,
) -> Node {
    let &VocabularyDirective {
        name,
        argument,
        span,
        body_lines,
        ctx: body_ctx,
    } = directive;
    if let Some(node) = try_parse_substitution_definition(
        name,
        argument,
        span,
        body_lines,
        state.diagnostics,
        body_ctx,
    ) {
        return node;
    }
    if let Some(node) = try_parse_entity_section(
        name,
        argument,
        &EntitySectionSite { span, body_lines },
        state.adornment_order,
        state.diagnostics,
        body_ctx,
    ) {
        return node;
    }
    let directive = unknown_directive(
        name.to_string(),
        argument.to_string(),
        body_lines,
        span,
        state.diagnostics,
    );
    Node::Directive(directive)
}

/// Recognizes and parses a substitution definition's marker, `.. |name|
/// inner::`, which is not a directive name at all.
fn try_parse_substitution_definition(
    name: &str,
    argument: &str,
    directive_span: Option<rinx_ast::Span>,
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

/// Where in the source a directive marker sits, and the lines it may claim.
///
/// One value rather than four parameters, because every one of them is read
/// only to answer the same question — how far this directive's body reaches
/// and where its diagnostics point.
struct DirectiveSite<'a> {
    /// The whole line slice being parsed.
    lines: &'a [&'a str],
    /// The marker's index within it.
    index: usize,
    /// The marker line itself.
    line: &'a str,
    /// The indent the body must exceed.
    min_indent: usize,
}

/// Parses `.. <object type>::` when `name` resolves to a domain object under
/// the default domain.
///
/// Split out of [`try_parse_directive`] purely to keep that function's line
/// count readable.
fn try_parse_domain_object_directive(
    name: &str,
    site: &DirectiveSite<'_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Vec<Node>)> {
    let object_type = resolve_domain_object_type(name, ctx.default_domain)?;
    let &DirectiveSite {
        lines,
        index: i,
        line,
        min_indent,
    } = site;
    let body = collect_directive_body(lines, i + 1, min_indent);
    let (body_lines, _) = body.for_directive();
    let marker = MarkerLine {
        index: i,
        span: ctx.line_span(i, line),
        argument: MarkerText::after_marker(line),
    };
    let invocation = read_directive_block(
        name,
        object_spec(object_type),
        &marker,
        &body,
        &body_lines,
        diagnostics,
        ctx,
    );
    let domain_object = parse_domain_object(object_type, &invocation, adornment_order, diagnostics);
    Some((
        1 + body.consumed,
        vec![Node::Directive(Directive::DomainObject(domain_object))],
    ))
}

/// Parses `.. <type>::` when the schema declares an entity type of that name.
///
/// Split out of [`try_parse_directive`] purely to keep that function's line
/// count readable, the same reason [`try_parse_domain_object_directive`]
/// exists.
fn try_parse_entity_directive(
    name: &str,
    site: &DirectiveSite<'_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Vec<Node>)> {
    let entity_type = ctx.schema.entity_type(name)?;
    let &DirectiveSite {
        lines,
        index: i,
        line,
        min_indent,
    } = site;
    let body = collect_directive_body(lines, i + 1, min_indent);
    let (body_lines, _) = body.for_directive();
    let marker = MarkerLine {
        index: i,
        span: ctx.line_span(i, line),
        argument: MarkerText::after_marker(line),
    };
    // An entity's content takes no section titles, as sphinx-needs parses a
    // need's content with `match_titles=False` — the reader's default.
    let invocation = read_directive_block(
        name,
        &EntitySpec::new(entity_type).spec(),
        &marker,
        &body,
        &body_lines,
        diagnostics,
        ctx,
    );
    let discriminator = ctx.position(i, 0).map_or(0, |point| point.position.line);
    let directive = parse_entity(
        &EntityDirective {
            entity_type,
            invocation: &invocation,
            discriminator,
            doc_path: ctx.doc_path,
        },
        adornment_order,
        diagnostics,
    );
    Some((1 + body.consumed, vec![Node::Directive(directive)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::Domain;
    use rinx_ast::{CodeBlockSource, CodeLanguage};

    /// A plain toctree document entry spanning the whole of source line
    /// `line`, which is the shape `parse_toctree` produces for an unindented
    /// entry written under a top-level directive.
    fn toc_document_entry(docname: &str, line: u32) -> rinx_ast::TocEntry {
        let column_end = u32::try_from(docname.chars().count()).unwrap_or(0) + 4;
        rinx_ast::TocEntry::Document {
            title: None,
            docname: docname.to_string(),
            span: Some(rinx_ast::Span::new(
                rinx_ast::Position::new(line, 4),
                rinx_ast::Position::new(line, column_end),
            )),
        }
    }

    /// Dispatches `.. name:: argument` over the indented `body` through
    /// [`try_parse_directive`] with throwaway state, returning the one node and
    /// any diagnostics.
    fn dispatch(name: &str, argument: &str, body: &[&str]) -> (Node, Diagnostics) {
        let (nodes, diagnostics) = dispatch_all(name, argument, body);
        let [node] = <[Node; 1]>::try_from(nodes).expect("exactly one node");
        (node, diagnostics)
    }

    /// [`dispatch`] for a directive answering with any number of nodes.
    fn dispatch_all(name: &str, argument: &str, body: &[&str]) -> (Vec<Node>, Diagnostics) {
        let marker = format!(".. {name}:: {argument}");
        let lines: Vec<&str> = std::iter::once(marker.as_str())
            .chain(body.iter().copied())
            .collect();
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();
        let (_, nodes) = try_parse_directive(
            &lines,
            0,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        )
        .expect("a directive marker");
        (nodes, diagnostics)
    }

    #[test]
    fn test_try_parse_directive_reaches_a_parser_for_every_spelling_in_the_table() {
        // Given / When / Then — whatever each parser makes of a bare argument
        // and body, none of them may fall through to an unknown directive
        for spec in crate::directives::directive_specs() {
            for name in spec.names {
                let (nodes, _) = dispatch_all(name, "x", &["   content"]);
                assert!(
                    !nodes
                        .iter()
                        .any(|node| matches!(node, Node::Directive(Directive::Unknown { .. }))),
                    "`{name}` fell through to an unknown directive: {nodes:?}"
                );
            }
        }
    }

    #[test]
    fn test_try_parse_directive_reads_no_option_after_the_blank_line_below_the_directive() {
        // Given — a code block *showing* roles: its content starts after the
        // blank line, so docutils reads no option there
        let input = ".. code-block:: rst\n\n   :ref:`label`\n   :term:`word`\n";

        // When
        let doc = crate::parse("test.rst", input);

        // Then
        assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
        let [Node::Directive(Directive::CodeBlock(block))] = doc.nodes.as_slice() else {
            panic!("Expected one code block, got {:?}", doc.nodes);
        };
        assert_eq!(block.content, ":ref:`label`\n:term:`word`");
    }

    #[test]
    fn test_try_parse_directive_reads_an_option_directly_below_the_directive() {
        // Given
        let input = ".. code-block:: rst\n   :linenos:\n\n   :ref:`label`\n";

        // When
        let doc = crate::parse("test.rst", input);

        // Then
        assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
        let [Node::Directive(Directive::CodeBlock(block))] = doc.nodes.as_slice() else {
            panic!("Expected one code block, got {:?}", doc.nodes);
        };
        assert!(block.linenos);
        assert_eq!(block.content, ":ref:`label`");
    }

    #[test]
    fn test_try_parse_directive_dispatches_a_known_directive() {
        // Given
        let body = ["   term", "      Definition."];

        // When
        let (node, _) = dispatch("glossary", "", &body);

        // Then
        assert!(matches!(node, Node::Directive(Directive::Glossary { .. })));
    }

    #[test]
    fn test_try_parse_directive_dispatches_the_doctest_family() {
        // Given
        let body = ["", "   >>> 1"];

        // When
        let (node, _) = dispatch("doctest", "", &body);

        // Then
        assert!(matches!(node, Node::Directive(Directive::DocTest(_))));
    }

    #[test]
    fn test_try_parse_directive_routes_both_code_block_spellings() {
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
    fn test_try_parse_directive_falls_back_to_unknown() {
        // Given — this is what the benchmark counts as unsupported.
        let body = ["   content"];

        // When
        let (node, diagnostics) = dispatch("not-a-real-directive", "arg", &body);

        // Then — the name and body are kept for the visible error block the
        // renderer draws, and the fall-through is reported rather than silent
        match node {
            Node::Directive(Directive::Unknown {
                name,
                argument,
                body,
            }) => {
                assert_eq!(name, "not-a-real-directive");
                assert_eq!(argument, "arg");
                assert_eq!(body, "content");
            }
            other => panic!("expected an unknown directive, got {other:?}"),
        }
        let (found, _, _) = diagnostics.into_parts();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, rinx_ast::DiagnosticCode::DirectiveUnknown);
        assert_eq!(
            found[0].message,
            "unknown directive type 'not-a-real-directive'"
        );
    }

    #[test]
    fn test_an_unknown_domain_qualified_directive_names_itself_as_written() {
        // Given — a domain this build does not have
        let source = ".. cpp:class:: Widget\n";

        // When
        let document = crate::parse("index.rst", source);

        // Then — the prefix is part of the subject, since the name it qualifies
        // is meaningless without it
        let subjects: Vec<_> = document
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.subject.clone())
            .collect();
        assert_eq!(
            subjects,
            [Some(rinx_ast::DiagnosticSubject::Directive(
                "cpp:class".to_string()
            ))]
        );
    }

    #[test]
    fn test_try_parse_directive_routes_needsequence_to_the_sequence_parser() {
        // Given — no schema is in hand here, so the relation is unknown and the
        // diagram degrades; what matters is which parser said so
        let body = ["   :start: COMP_UI", "   :link_types: sends"];

        // When
        let (_, diagnostics) = dispatch("needsequence", "Startup", &body);

        // Then
        let (found, _, _) = diagnostics.into_parts();
        let codes: Vec<_> = found.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            [rinx_ast::DiagnosticCode::EntitySequenceUnknownRelation]
        );
    }

    #[test]
    fn test_try_parse_directive_refuses_needservice_by_name_rather_than_as_unknown() {
        // Given
        let body = ["   :query: repo:useblocks/sphinx-needs"];

        // When
        let (node, diagnostics) = dispatch("needservice", "github-issues", &body);

        // Then
        assert!(matches!(
            node,
            Node::Directive(Directive::Malformed { ref name, .. }) if name == "needservice"
        ));
        let (found, _, _) = diagnostics.into_parts();
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].code,
            rinx_ast::DiagnosticCode::NeedServiceUnsupported
        );
    }

    #[test]
    fn test_try_parse_directive_forwards_diagnostics() {
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
            assert_eq!(kind, &rinx_ast::AdmonitionKind::Note);
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
            assert_eq!(kind, &rinx_ast::AdmonitionKind::Admonition);
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
            Node::Directive(Directive::Toctree(rinx_ast::Toctree {
                entries: vec![
                    toc_document_entry("team_a/index", 3),
                    toc_document_entry("team_b/index", 4),
                ],
                options: rinx_ast::ToctreeOptions::default(),
            }))
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rinx_ast::InlineNode::Text("Next Para".to_string())])
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
            Node::Directive(Directive::Toctree(rinx_ast::Toctree {
                entries: vec![
                    toc_document_entry("team_a/index", 4),
                    toc_document_entry("team_b/index", 5),
                ],
                options: rinx_ast::ToctreeOptions {
                    maxdepth: std::num::NonZeroUsize::new(2),
                    ..rinx_ast::ToctreeOptions::default()
                },
            }))
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rinx_ast::InlineNode::Text("Next Para".to_string())])
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

        let Node::Directive(Directive::Uml(uml)) = &doc.nodes[0] else {
            panic!("expected a diagram, found {:?}", doc.nodes[0]);
        };
        assert_eq!(uml.source, rinx_ast::UmlSource::PlantUml);
        assert_eq!(uml.template, "A -> B\nB -> C");
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rinx_ast::InlineNode::Text("Next Para".to_string())])
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
