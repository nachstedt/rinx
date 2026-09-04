//! Smoke tests for the individual parse/render steps and the legacy
//! `process_rst` single-pass entry point.

use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_ast as ast;
use rusty_sphinx_parser as parser;
use rusty_sphinx_renderer as renderer;
use rusty_sphinx_worker::process_rst;

#[test]
fn test_parser_step() {
    let input = "Title\n=====\n\nParagraph text here.\nMore text.";
    let ast = parser::parse("test.rst", input);
    assert_eq!(ast.nodes.len(), 2);
    assert_eq!(
        ast.nodes[0],
        ast::Node::Heading {
            level: 1,
            text: vec![ast::InlineNode::Text("Title".to_string())]
        }
    );
    assert_eq!(
        ast.nodes[1],
        ast::Node::Paragraph(vec![ast::InlineNode::Text(
            "Paragraph text here.\nMore text.".to_string()
        )])
    );
}

#[test]
fn test_renderer_step() {
    let doc = ast::Document::new(
        "test.rst".to_string(),
        vec![
            ast::Node::Heading {
                level: 1,
                text: vec![ast::InlineNode::Text("Section".to_string())],
            },
            ast::Node::Paragraph(vec![ast::InlineNode::Text("A line of text.".to_string())]),
        ],
    );
    let index = analyzer::analyze(&doc);
    let html = renderer::render(&doc, &index, &doc.path).html;

    let expected = "<h1 id=\"section\">Section</h1>\n<p>A line of text.</p>\n";
    assert_eq!(html, expected);
}

#[test]
fn test_e2e_translation() {
    let input = "\
Overview
--------

This is a simple paragraph.
It spans multiple lines.

Another Heading
~~~~~~~~~~~~~~~

And another paragraph.
";

    // Overview uses '-' (first seen) -> h1
    // Another Heading uses '~' (second seen) -> h2
    let expected_html = "\
<h1 id=\"overview\">Overview</h1>
<p>This is a simple paragraph.\nIt spans multiple lines.</p>
<h2 id=\"another-heading\">Another Heading</h2>
<p>And another paragraph.</p>
";

    let result = process_rst("test.rst", input);
    assert_eq!(result, expected_html);
}

#[test]
fn test_e2e_multi_level_headings() {
    let input = "\
Level 1
=======

Text 1.

Level 2
-------

Text 2.

Level 3
~~~~~~~

Text 3.

Another Level 1
===============

Text 4.
";

    let expected_html = "\
<h1 id=\"level-1\">Level 1</h1>
<p>Text 1.</p>
<h2 id=\"level-2\">Level 2</h2>
<p>Text 2.</p>
<h3 id=\"level-3\">Level 3</h3>
<p>Text 3.</p>
<h1 id=\"another-level-1\">Another Level 1</h1>
<p>Text 4.</p>
";

    let result = process_rst("test.rst", input);
    assert_eq!(result, expected_html);
}
