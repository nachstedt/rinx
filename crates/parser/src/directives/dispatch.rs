//! Recognizing a directive at the head of a block and routing it to the
//! parser for its kind — the domain-object family via [`super::domains`],
//! everything else by directive name.

use crate::context::ParseCtx;
use crate::headings::Adornment;
use crate::indent::{indent_width, strip_common_indent};

use super::admonitions::{parse_admonition, parse_seealso, parse_version_change};
use super::body::{collect_argument_continuation_lines, collect_directive_body, join_body_lines};
use super::data_table::{parse_csv_table, parse_list_table};
use super::doctest::{DocTestDirectiveKind, parse_doctest_directive};
use super::domains::object_type::{DirectiveObjectType, resolve_domain_object_type};
use super::domains::parse_domain_object;
use super::glossary::parse_glossary;
use super::index_directive::parse_index_directive;
use super::scope::try_parse_scope_directive;
use super::toctree::parse_toctree;
use rusty_sphinx_ast::{Directive, Node};

pub(crate) fn try_parse_directive(
    lines: &[&str],
    i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Node)> {
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
        let (consumed_lines, body_lines) =
            collect_directive_body(lines, i + 1 + continuations_consumed, min_indent);
        let domain_object = parse_domain_object(
            object_type,
            argument,
            continuations,
            &body_lines,
            adornment_order,
            diagnostics,
            ctx,
        );
        return Some((
            1 + continuations_consumed + consumed_lines,
            Node::Directive(Directive::DomainObject(domain_object)),
        ));
    }

    let (consumed_lines, body_lines) = collect_directive_body(lines, i + 1, min_indent);
    let node = parse_body_directive(
        name,
        argument,
        &body_lines,
        adornment_order,
        diagnostics,
        ctx,
    );
    Some((1 + consumed_lines, node))
}

/// Dispatches every directive whose body is collected the ordinary way — that
/// is, all of them except domain objects, which peel extra signature lines off
/// the argument before their body starts and so are handled by the caller.
///
/// Falls back to [`Directive::Unknown`], which is what the benchmark counts as
/// an unsupported directive.
fn parse_body_directive(
    name: String,
    argument: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    ctx: &ParseCtx<'_>,
) -> Node {
    if name == "toctree" {
        let directive = parse_toctree(body_lines, diagnostics);
        return Node::Directive(directive);
    }
    if name == "plantuml" {
        let directive = Directive::PlantUml(rusty_sphinx_ast::HashedContent::new(join_body_lines(
            body_lines,
        )));
        return Node::Directive(directive);
    }
    if name == "code-block" {
        let node = parse_code_block(argument, body_lines);
        return node;
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
    if name == "index" {
        let directive = parse_index_directive(&argument, body_lines, diagnostics);
        return Node::Directive(directive);
    }
    if let Some(kind) = DocTestDirectiveKind::from_name(&name) {
        let directive = parse_doctest_directive(kind, &argument, body_lines, diagnostics);
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

/// Parses a `.. code-block::` directive's argument (the language, if any)
/// and body into a [`Node::LiteralBlock`], stripping the common leading
/// indentation from `body_lines` (collected by [`collect_directive_body`])
/// to preserve relative indentation within the block (RST spec behaviour).
fn parse_code_block(argument: String, body_lines: &[&str]) -> Node {
    let language = if argument.is_empty() {
        None
    } else {
        Some(argument)
    };
    let content = strip_common_indent(body_lines);
    Node::LiteralBlock { language, content }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::Domain;
    use rusty_sphinx_ast::HashedContent;

    /// Dispatches `name`/`argument`/`body` through [`parse_body_directive`]
    /// with throwaway state, returning the node and any diagnostics.
    fn dispatch(name: &str, argument: &str, body: &[&str]) -> (Node, Vec<String>) {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();
        let node = parse_body_directive(
            name.to_string(),
            argument.to_string(),
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
    fn test_parse_body_directive_returns_a_literal_block_for_code_block() {
        // Given — the one dispatch arm that yields a non-directive node.
        let body = ["   print(1)"];

        // When
        let (node, _) = dispatch("code-block", "python", &body);

        // Then
        assert!(matches!(node, Node::LiteralBlock { .. }));
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
            Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
                maxdepth: None,
                ignored_options: vec![],
            })
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
            Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
                maxdepth: Some(2),
                ignored_options: vec![],
            })
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
        assert_eq!(
            doc.nodes[0],
            Node::LiteralBlock {
                language: Some("rust".to_string()),
                content: "let x = 1;\n\nlet y = 2;".to_string(),
            }
        );
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
        if let Node::LiteralBlock { language, content } = &doc.nodes[0] {
            assert_eq!(language.as_deref(), Some("python"));
            assert_eq!(content, "x = 1");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_code_block_directive_without_language() {
        // Given
        let input = ".. code-block::

    x = 1
";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::LiteralBlock { language, content } = &doc.nodes[0] {
            assert!(language.is_none());
            assert_eq!(content, "x = 1");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[0]);
        }
    }
}
