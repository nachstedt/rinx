//! Structural block-level node rendering: lists, tables, definition
//! lists, and the directive dispatcher — everything [`render_nodes`] reaches
//! while walking a document's node tree.

use rusty_sphinx_ast::{Directive, Enumerator, InlineNode, ListItem, Node};
use std::fmt::Write as _;

use super::admonitions::{render_admonition, render_seealso, render_version_change};
use super::data_table::{DataTableParams, render_data_table};
use super::doctest::{render_bare_doctest_block, render_doctest_block};
use super::domain_object::render_domain_object;
use super::glossary::{render_glossary, render_index_anchor};
use super::nav::{find_nav_entry, render_nav_entry};
use super::scope_directives::apply_scope_directive;
use crate::RenderCtx;
use crate::inline::render_inline;

/// Renders the `<li>` elements shared by both list kinds.
///
/// The two kinds differ only in their wrapper element — docutils models both
/// with one `list_item` node, and so does [`rusty_sphinx_ast::ListItem`].
fn render_list_items(html: &mut String, items: &[ListItem], ctx: &mut RenderCtx) {
    for item in items {
        let _ = write!(html, "<li>");
        render_nodes(html, &item.nodes, ctx);
        let _ = writeln!(html, "</li>");
    }
}

/// Builds the opening `<ol>` tag for an enumerated list.
///
/// Sphinx emits only the sequence class here — its HTML writers carry a
/// literal `@@@ To do: prefix, suffix.` and drop the punctuation, so `(a)`,
/// `a)` and `a.` all render identically. rusty-sphinx keeps the distinction:
/// the format class lets the stylesheet reproduce the parentheses the author
/// actually wrote. Do not "simplify" this back to Sphinx's output.
///
/// `start` is emitted only when the list does not begin at 1, matching
/// docutils. For the parenthesised formats the browser's own marker is
/// suppressed in CSS, so the starting point is additionally handed to the
/// counter as an inline `counter-reset`; the `start` attribute is still
/// present so the list numbers correctly without the stylesheet.
fn open_enumerated_list_tag(start: Enumerator) -> String {
    let mut tag = format!(
        "<ol class=\"{} {}\"",
        start.sequence().css_class(),
        start.format().css_class()
    );
    if start.ordinal() != 1 {
        let _ = write!(tag, " start=\"{}\"", start.ordinal());
    }
    if !start.format().is_native_marker() {
        let _ = write!(
            tag,
            " style=\"counter-reset: rsl {}\"",
            i64::from(start.ordinal()) - 1
        );
    }
    tag.push('>');
    tag
}

/// Collects the URIs of anonymous hyperlink targets, in document order.
///
/// The match is deliberately exhaustive: every block-level container has to be
/// descended into, or an anonymous target written inside one silently fails to
/// pair with its reference. Leaving a `_` arm here is what let list, table and
/// definition-list bodies go unvisited for as long as they did.
pub(crate) fn collect_anonymous_targets(nodes: &[Node], targets: &mut Vec<String>) {
    for node in nodes {
        match node {
            Node::AnonymousTarget { uri } => targets.push(uri.clone()),
            Node::Directive(
                Directive::Admonition { body, .. }
                | Directive::VersionChange { body, .. }
                | Directive::SeeAlso { body },
            ) => {
                collect_anonymous_targets(body, targets);
            }
            Node::Directive(Directive::DomainObject(obj)) => {
                collect_anonymous_targets(obj.body(), targets);
            }
            Node::Directive(Directive::Glossary { entries, .. }) => {
                for entry in entries {
                    collect_anonymous_targets(&entry.definition, targets);
                }
            }
            Node::Directive(Directive::DataTable { rows, .. }) => {
                for row in rows {
                    for cell in &row.cells {
                        collect_anonymous_targets(&cell.content, targets);
                    }
                }
            }
            Node::BulletList { items, .. } | Node::EnumeratedList { items, .. } => {
                for item in items {
                    collect_anonymous_targets(&item.nodes, targets);
                }
            }
            Node::DefinitionList { items } => {
                for item in items {
                    collect_anonymous_targets(&item.definition, targets);
                }
            }
            Node::Table {
                header_rows,
                body_rows,
            } => {
                for row in header_rows.iter().chain(body_rows) {
                    for cell in &row.cells {
                        collect_anonymous_targets(&cell.content, targets);
                    }
                }
            }
            // Every other directive's payload is inline or verbatim, and the
            // remaining node kinds have no block-level children at all.
            Node::Directive(_)
            | Node::Heading { .. }
            | Node::Paragraph(_)
            | Node::Target { .. }
            | Node::LiteralBlock { .. }
            | Node::DoctestBlock(_)
            | Node::Comment
            | Node::Transition => {}
        }
    }
}

/// Renders a sequence of inline nodes in order, sharing the same `ctx`
/// (index/anon-target state) across calls. Used for both heading text and
/// paragraph content, which are both just a `Vec<InlineNode>`.
fn render_inlines(html: &mut String, inlines: &[InlineNode], ctx: &mut RenderCtx<'_>) {
    for inline in inlines {
        render_inline(html, inline, ctx);
    }
}

pub(crate) fn render_nodes(html: &mut String, nodes: &[Node], ctx: &mut RenderCtx<'_>) {
    for node in nodes {
        match node {
            Node::Heading { level, text } => {
                let tag = format!("h{}", (*level).clamp(1, 6));
                let _ = write!(html, "<{tag}>");
                render_inlines(html, text, ctx);
                let _ = writeln!(html, "</{tag}>");
            }
            Node::Paragraph(inlines) => {
                let _ = write!(html, "<p>");
                render_inlines(html, inlines, ctx);
                let _ = writeln!(html, "</p>");
            }
            Node::Target { name, uri } => {
                if uri.is_none() {
                    let escaped_name = html_escape::encode_text(name.as_str());
                    let _ = writeln!(html, "<a id=\"{escaped_name}\"></a>");
                }
            }
            // Anonymous targets and comments produce no HTML output.
            Node::AnonymousTarget { .. } | Node::Comment => {}
            Node::Transition => {
                let _ = writeln!(html, "<hr />");
            }
            Node::Directive(directive) => render_directive(html, directive, ctx),
            Node::BulletList { items, .. } => {
                let _ = writeln!(html, "<ul>");
                render_list_items(html, items, ctx);
                let _ = writeln!(html, "</ul>");
            }
            Node::EnumeratedList { start, items } => {
                let _ = writeln!(html, "{}", open_enumerated_list_tag(*start));
                render_list_items(html, items, ctx);
                let _ = writeln!(html, "</ol>");
            }
            Node::DefinitionList { items } => {
                let _ = writeln!(html, "<dl>");
                for item in items {
                    let _ = write!(html, "<dt>");
                    render_inlines(html, &item.term, ctx);
                    let _ = writeln!(html, "</dt>");
                    let _ = write!(html, "<dd>");
                    render_nodes(html, &item.definition, ctx);
                    let _ = writeln!(html, "</dd>");
                }
                let _ = writeln!(html, "</dl>");
            }
            Node::Table {
                header_rows,
                body_rows,
            } => {
                let _ = writeln!(html, "<table>");
                if !header_rows.is_empty() {
                    let _ = writeln!(html, "<thead>");
                    for row in header_rows {
                        super::tables::render_table_row(html, row, "th", ctx);
                    }
                    let _ = writeln!(html, "</thead>");
                }
                let _ = writeln!(html, "<tbody>");
                for row in body_rows {
                    super::tables::render_table_row(html, row, "td", ctx);
                }
                let _ = writeln!(html, "</tbody>");
                let _ = writeln!(html, "</table>");
            }
            Node::LiteralBlock { language, content } => {
                let escaped = html_escape::encode_text(content);
                if let Some(lang) = language {
                    let lang_attr = html_escape::encode_double_quoted_attribute(lang);
                    let _ = writeln!(
                        html,
                        "<pre><code class=\"language-{lang_attr}\">{escaped}</code></pre>"
                    );
                } else {
                    let _ = writeln!(html, "<pre><code>{escaped}</code></pre>");
                }
            }
            // A bare `>>>` block. Rendered like the `.. doctest::` directive
            // form, which is what Sphinx does — and, unlike the literal block
            // above, this one is also executed.
            Node::DoctestBlock(content) => {
                html.push_str(&render_bare_doctest_block(content));
            }
        }
    }
}

fn render_directive(html: &mut String, directive: &Directive, ctx: &mut RenderCtx<'_>) {
    match directive {
        Directive::Toctree { maxdepth, .. } => {
            let _ = writeln!(html, "<ul>");
            let current_dir = std::path::Path::new(ctx.doc_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""));
            if let Some(current_entry) = find_nav_entry(&ctx.index.nav_tree, ctx.original_doc_path)
            {
                for child in &current_entry.children {
                    render_nav_entry(html, child, ctx.index, current_dir, 1, *maxdepth);
                }
            }
            let _ = writeln!(html, "</ul>");
        }
        Directive::PlantUml(content) => {
            let escaped_hash = html_escape::encode_text(content.hash());

            let current_dir = std::path::Path::new(ctx.doc_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""));
            let image_path = std::path::Path::new("_images").join(format!("{escaped_hash}.svg"));
            let relative_path =
                pathdiff::diff_paths(&image_path, current_dir).unwrap_or(image_path);
            let src = relative_path.display();

            let _ = writeln!(html, "<div class=\"plantuml-diagram\">");
            let _ = writeln!(html, "  <img src=\"{src}\" alt=\"PlantUML Diagram\" />");
            let _ = writeln!(html, "</div>");
        }
        Directive::Admonition {
            kind,
            title,
            collapsible,
            body,
        } => render_admonition(html, *kind, title.as_deref(), *collapsible, body, ctx),
        Directive::VersionChange {
            kind,
            version,
            body,
        } => render_version_change(html, *kind, version, body, ctx),
        Directive::SeeAlso { body } => render_seealso(html, body, ctx),
        Directive::Glossary { entries, .. } => render_glossary(html, entries, ctx),
        Directive::Index { id, .. } => render_index_anchor(html, id),
        Directive::DomainObject(obj) => render_domain_object(html, obj, ctx),
        Directive::DataTable {
            source,
            title,
            header_rows,
            stub_columns,
            widths,
            width,
            align,
            classes,
            name,
            rows,
        } => render_data_table(
            html,
            DataTableParams {
                source: *source,
                title: title.as_deref(),
                header_rows: *header_rows,
                stub_columns: *stub_columns,
                widths: widths.as_ref(),
                width: width.as_deref(),
                align: *align,
                classes,
                name: name.as_ref(),
                rows,
            },
            ctx,
        ),
        // Scope-mutating directives render no HTML of their own — mirrored
        // from the analyzer's `index_nodes` so anchor `id`s never drift from
        // the index keys it built.
        Directive::PyCurrentModule { .. }
        | Directive::CNamespace { .. }
        | Directive::CNamespacePush { .. }
        | Directive::CNamespacePop
        | Directive::StdProgram { .. } => apply_scope_directive(directive, ctx),
        // Presentation only — whether this block's code passes, fails, or is
        // never run is decided by a separate, opt-in test target, and cannot
        // influence the HTML.
        Directive::DocTest(block) => {
            if let Some(rendered) = render_doctest_block(block) {
                html.push_str(&rendered);
            }
        }
        Directive::Unknown { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render;
    use rusty_sphinx_ast::TargetSearchOrder;
    use rusty_sphinx_ast::{Document, EnumeratorFormat, EnumeratorSequence};
    use rusty_sphinx_index::ProjectIndex;

    /// Builds a two-item enumerated list starting at `ordinal`.
    fn enumerated_list(
        sequence: EnumeratorSequence,
        format: EnumeratorFormat,
        ordinal: u32,
    ) -> Node {
        Node::EnumeratedList {
            start: Enumerator::new(sequence, format, ordinal).expect("valid ordinal"),
            items: vec![
                ListItem {
                    nodes: vec![Node::Paragraph(vec![InlineNode::Text("One".to_string())])],
                },
                ListItem {
                    nodes: vec![Node::Paragraph(vec![InlineNode::Text("Two".to_string())])],
                },
            ],
        }
    }

    #[test]
    fn test_render_bullet_list() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::BulletList {
                bullet: '*',
                items: vec![
                    rusty_sphinx_ast::ListItem {
                        nodes: vec![Node::Paragraph(vec![InlineNode::Text(
                            "Item 1".to_string(),
                        )])],
                    },
                    rusty_sphinx_ast::ListItem {
                        nodes: vec![Node::Paragraph(vec![InlineNode::Text(
                            "Item 2".to_string(),
                        )])],
                    },
                ],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<ul>\n<li><p>Item 1</p>\n</li>\n<li><p>Item 2</p>\n</li>\n</ul>\n"
        );
    }
    #[test]
    fn test_render_bullet_list_nested() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::BulletList {
                bullet: '*',
                items: vec![rusty_sphinx_ast::ListItem {
                    nodes: vec![
                        Node::Paragraph(vec![InlineNode::Text("Parent".to_string())]),
                        Node::BulletList {
                            bullet: '-',
                            items: vec![rusty_sphinx_ast::ListItem {
                                nodes: vec![Node::Paragraph(vec![InlineNode::Text(
                                    "Child".to_string(),
                                )])],
                            }],
                        },
                    ],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains(
            "<ul>\n<li><p>Parent</p>\n<ul>\n<li><p>Child</p>\n</li>\n</ul>\n</li>\n</ul>"
        ));
    }
    #[test]
    fn test_render_bullet_list_multi_paragraph() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::BulletList {
                bullet: '*',
                items: vec![rusty_sphinx_ast::ListItem {
                    nodes: vec![
                        Node::Paragraph(vec![InlineNode::Text("Para 1".to_string())]),
                        Node::Paragraph(vec![InlineNode::Text("Para 2".to_string())]),
                    ],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains("<li><p>Para 1</p>\n<p>Para 2</p>\n</li>"));
    }
    #[test]
    fn test_render_enumerated_list() {
        // Given a plain arabic list starting at one
        let doc = Document::new(
            "test.rst".to_string(),
            vec![enumerated_list(
                EnumeratorSequence::Arabic,
                EnumeratorFormat::Period,
                1,
            )],
        );
        let index = ProjectIndex::default();

        // When rendering it
        let result = render(&doc, &index, &doc.path).html;

        // Then it becomes an `<ol>` carrying its sequence and format classes,
        // with no `start` attribute and no counter styling
        assert_eq!(
            result,
            "<ol class=\"arabic period\">\n<li><p>One</p>\n</li>\n<li><p>Two</p>\n</li>\n</ol>\n"
        );
    }
    #[test]
    fn test_render_enumerated_list_carries_each_sequence_class() {
        // Given each enumeration sequence
        let expected = [
            (EnumeratorSequence::Arabic, "arabic"),
            (EnumeratorSequence::LowerAlpha, "loweralpha"),
            (EnumeratorSequence::UpperAlpha, "upperalpha"),
            (EnumeratorSequence::LowerRoman, "lowerroman"),
            (EnumeratorSequence::UpperRoman, "upperroman"),
        ];
        let index = ProjectIndex::default();

        for (sequence, class) in expected {
            let doc = Document::new(
                "test.rst".to_string(),
                vec![enumerated_list(sequence, EnumeratorFormat::Period, 1)],
            );

            // When rendering it
            let result = render(&doc, &index, &doc.path).html;

            // Then the class names the sequence, matching Sphinx's vocabulary
            assert!(
                result.starts_with(&format!("<ol class=\"{class} period\">")),
                "{sequence:?}: {result}"
            );
        }
    }
    #[test]
    fn test_render_enumerated_list_distinguishes_the_punctuation_formats() {
        // Given the same list in each format
        let expected = [
            (EnumeratorFormat::Period, "period"),
            (EnumeratorFormat::RightParen, "rparen"),
            (EnumeratorFormat::Parens, "parens"),
        ];
        let index = ProjectIndex::default();

        for (format, class) in expected {
            let doc = Document::new(
                "test.rst".to_string(),
                vec![enumerated_list(EnumeratorSequence::Arabic, format, 1)],
            );

            // When rendering it
            let result = render(&doc, &index, &doc.path).html;

            // Then the format reaches the HTML. Sphinx drops it, so `(1)` and
            // `1.` are indistinguishable in its output; keeping the class is
            // what lets the stylesheet reproduce the author's punctuation.
            assert!(
                result.contains(&format!("class=\"arabic {class}\"")),
                "{format:?}: {result}"
            );
        }
    }
    #[test]
    fn test_render_enumerated_list_emits_a_start_attribute_only_when_it_is_not_one() {
        // Given lists starting at one and at five
        let index = ProjectIndex::default();
        let from_one = Document::new(
            "test.rst".to_string(),
            vec![enumerated_list(
                EnumeratorSequence::Arabic,
                EnumeratorFormat::Period,
                1,
            )],
        );
        let from_five = Document::new(
            "test.rst".to_string(),
            vec![enumerated_list(
                EnumeratorSequence::Arabic,
                EnumeratorFormat::Period,
                5,
            )],
        );

        // When rendering both
        let default_start = render(&from_one, &index, &from_one.path).html;
        let shifted_start = render(&from_five, &index, &from_five.path).html;

        // Then only the shifted list carries the attribute
        assert!(!default_start.contains("start="), "{default_start}");
        assert!(shifted_start.contains("start=\"5\""), "{shifted_start}");
    }
    #[test]
    fn test_render_enumerated_list_seeds_the_counter_for_parenthesised_formats() {
        // Given a parenthesised list starting at five
        let doc = Document::new(
            "test.rst".to_string(),
            vec![enumerated_list(
                EnumeratorSequence::LowerAlpha,
                EnumeratorFormat::Parens,
                5,
            )],
        );
        let index = ProjectIndex::default();

        // When rendering it
        let result = render(&doc, &index, &doc.path).html;

        // Then the CSS counter is seeded one below the start, since the
        // stylesheet suppresses the browser's own marker for this format
        assert!(
            result.contains("style=\"counter-reset: rsl 4\""),
            "{result}"
        );
    }
    #[test]
    fn test_render_enumerated_list_does_not_seed_a_counter_for_the_period_format() {
        // Given a period-format list starting at five
        let doc = Document::new(
            "test.rst".to_string(),
            vec![enumerated_list(
                EnumeratorSequence::Arabic,
                EnumeratorFormat::Period,
                5,
            )],
        );
        let index = ProjectIndex::default();

        // When rendering it
        let result = render(&doc, &index, &doc.path).html;

        // Then no counter styling is emitted — the browser's own marker already
        // renders this format, and `start` alone positions it
        assert!(!result.contains("counter-reset"), "{result}");
    }
    #[test]
    fn test_render_enumerated_list_seeds_a_negative_counter_for_a_zero_start() {
        // Given a list starting at zero, which docutils accepts from `0.`
        let doc = Document::new(
            "test.rst".to_string(),
            vec![enumerated_list(
                EnumeratorSequence::Arabic,
                EnumeratorFormat::RightParen,
                0,
            )],
        );
        let index = ProjectIndex::default();

        // When rendering it
        let result = render(&doc, &index, &doc.path).html;

        // Then the counter is seeded below zero rather than clamped, so the
        // first item still shows `0)`
        assert!(
            result.contains("style=\"counter-reset: rsl -1\""),
            "{result}"
        );
    }
    #[test]
    fn test_render_enumerated_list_nested() {
        // Given an enumerated list whose first item contains another
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::EnumeratedList {
                start: Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 1)
                    .unwrap(),
                items: vec![ListItem {
                    nodes: vec![
                        Node::Paragraph(vec![InlineNode::Text("Parent".to_string())]),
                        enumerated_list(
                            EnumeratorSequence::LowerAlpha,
                            EnumeratorFormat::Parens,
                            1,
                        ),
                    ],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When rendering it
        let result = render(&doc, &index, &doc.path).html;

        // Then the inner list nests inside the outer item
        assert!(
            result.contains("<li><p>Parent</p>\n<ol class=\"loweralpha parens\""),
            "{result}"
        );
        assert!(result.contains("</ol>\n</li>\n</ol>\n"), "{result}");
    }
    #[test]
    fn test_render_definition_list() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::DefinitionList {
                items: vec![
                    rusty_sphinx_ast::DefinitionListItem {
                        term: vec![InlineNode::Text("Term 1".to_string())],
                        definition: vec![Node::Paragraph(vec![InlineNode::Text(
                            "Def 1".to_string(),
                        )])],
                    },
                    rusty_sphinx_ast::DefinitionListItem {
                        term: vec![InlineNode::Text("Term 2".to_string())],
                        definition: vec![Node::Paragraph(vec![InlineNode::Text(
                            "Def 2".to_string(),
                        )])],
                    },
                ],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<dl>\n<dt>Term 1</dt>\n<dd><p>Def 1</p>\n</dd>\n<dt>Term 2</dt>\n<dd><p>Def 2</p>\n</dd>\n</dl>\n"
        );
    }
    #[test]
    fn test_render_definition_list_escapes_and_renders_inline_markup_in_term() {
        // Given a term containing a domain-object reference, mirroring the
        // CPython benchmark's `seealso` definition-list content
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::DefinitionList {
                items: vec![rusty_sphinx_ast::DefinitionListItem {
                    term: vec![
                        InlineNode::Text("Module ".to_string()),
                        InlineNode::DomainObjectReference {
                            object_type: rusty_sphinx_ast::ObjectType::Py(
                                rusty_sphinx_ast::PyObjectType::Module,
                            ),
                            name: "curses.ascii".to_string(),
                            display: "curses.ascii".to_string(),
                            link: true,
                            search_order: TargetSearchOrder::LeastQualifiedFirst,
                        },
                    ],
                    definition: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Utilities for ASCII characters.".to_string(),
                    )])],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then the <dt> contains the rendered inline markup, not raw text
        assert!(result.starts_with("<dl>\n<dt>Module "));
        assert!(result.contains("curses.ascii"));
        assert!(result.contains("<dd><p>Utilities for ASCII characters.</p>\n</dd>"));
    }
}
