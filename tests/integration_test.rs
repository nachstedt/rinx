//! Integration tests for Rusty Sphinx public API.

use rusty_sphinx::greeting;
use rusty_sphinx::process_rst;
use rusty_sphinx::{analyzer, ast, parser, renderer};

#[test]
fn integration_greeting() {
    let result = greeting("Integration");
    assert_eq!(result, "Hello, Integration!");
}

#[test]
fn test_parser_step() {
    let input = "Title\n=====\n\nParagraph text here.\nMore text.";
    let ast = parser::parse(input);
    assert_eq!(ast.nodes.len(), 2);
    assert_eq!(ast.nodes[0], ast::Node::Heading("Title".to_string()));
    assert_eq!(
        ast.nodes[1],
        ast::Node::Paragraph("Paragraph text here.\nMore text.".to_string())
    );
}

#[test]
fn test_renderer_step() {
    let doc = ast::Document::new(vec![
        ast::Node::Heading("Section".to_string()),
        ast::Node::Paragraph("A line of text.".to_string()),
    ]);
    let index = analyzer::analyze(&doc);
    let html = renderer::render(&doc, &index);

    let expected = "<h1>Section</h1>\n<p>A line of text.</p>\n";
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

    let expected_html = "\
<h1>Overview</h1>
<p>This is a simple paragraph.\nIt spans multiple lines.</p>
<h1>Another Heading</h1>
<p>And another paragraph.</p>
";

    let result = process_rst(input);
    assert_eq!(result, expected_html);
}
