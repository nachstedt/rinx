//! Structural block-level node rendering: lists, tables, definition
//! lists, and the directive dispatcher — everything [`render_nodes`] reaches
//! while walking a document's node tree.

use rusty_sphinx_ast::{
    Directive, Enumerator, HashedContent, InlineNode, ListItem, Node, Toctree, ToctreeFlag,
};
use std::fmt::Write as _;

use super::admonitions::{render_admonition, render_seealso, render_version_change};
use super::block_quote::render_block_quote;
use super::data_table::{DataTableParams, render_data_table};
use super::doctest::{render_bare_doctest_block, render_doctest_block};
use super::domain_object::render_domain_object;
use super::figure::render_figure_directive;
use super::glossary::{render_glossary, render_index_anchor};
use super::image::render_image_directive;
use super::line_block::render_line_block;
use super::math::render_math;
use super::option_list::render_option_list;
use super::scope_directives::apply_scope_directive;
use super::table_directive::{TableDirectiveParams, render_table_directive};
use crate::RenderCtx;
use crate::inline::render_inline;
use crate::nav::{expand_toctree_entries, write_nav_list};

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
            Node::OptionList { items } => {
                for item in items {
                    collect_anonymous_targets(&item.description, targets);
                }
            }
            // A bare grid/simple table and a `.. table::`-wrapped one share
            // the same header/body row shape, so one arm covers both.
            Node::Table {
                header_rows,
                body_rows,
            }
            | Node::Directive(Directive::Table {
                header_rows,
                body_rows,
                ..
            }) => {
                for row in header_rows.iter().chain(body_rows) {
                    for cell in &row.cells {
                        collect_anonymous_targets(&cell.content, targets);
                    }
                }
            }
            Node::BlockQuote { content, .. } => {
                collect_anonymous_targets(content, targets);
            }
            // Every other directive's payload is inline or verbatim, and the
            // remaining node kinds have no block-level children at all. A
            // line block's content is `InlineNode` only, so it joins this
            // group too.
            Node::Directive(_)
            | Node::Heading { .. }
            | Node::Paragraph(_)
            | Node::Target { .. }
            | Node::LiteralBlock { .. }
            | Node::DoctestBlock(_)
            | Node::Comment
            | Node::Transition
            | Node::LineBlock(_) => {}
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
    // Only the document's own top level holds sections, so only there does a
    // heading carry an `id`. Clearing the flag for the duration of this list's
    // nested bodies — and restoring it after — keeps a heading inside an
    // admonition or a list item from claiming a section anchor that belongs to
    // the top-level heading at the same index.
    let at_top_level = std::mem::replace(&mut ctx.at_top_level, false);
    // A document's first heading is its *title*, which the outline unwraps —
    // so its number is the document's own rather than a section's.
    let title_index = nodes
        .iter()
        .position(|node| matches!(node, Node::Heading { .. }))
        .unwrap_or(usize::MAX);
    for (index, node) in nodes.iter().enumerate() {
        match node {
            Node::Heading { level, text } => {
                let id = at_top_level
                    .then(|| ctx.section_ids.get(&index))
                    .flatten()
                    .cloned();
                render_heading(html, *level, text, id.as_ref(), index == title_index, ctx);
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
            Node::OptionList { items } => {
                render_option_list(html, items, ctx);
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
            } => super::tables::render_table(html, header_rows, body_rows, ctx),
            // A `::` block carries no options and names no language of its
            // own, but it is still highlighted — with whatever language the
            // enclosing `.. highlight::` set, exactly as Sphinx does.
            Node::LiteralBlock { language, content } => {
                let resolved = language.resolve(&ctx.highlight_language);
                let force = ctx.highlight_force;
                super::code_block::render_code(
                    html,
                    content,
                    &resolved,
                    &super::code_block::CodeLayout::plain(),
                    force,
                    None,
                    ctx,
                );
            }
            // A bare `>>>` block. Rendered like the `.. doctest::` directive
            // form, which is what Sphinx does — and, unlike the literal block
            // above, this one is also executed.
            Node::DoctestBlock(content) => {
                let rendered = render_bare_doctest_block(content, ctx);
                html.push_str(&rendered);
            }
            Node::LineBlock(items) => render_line_block(html, items, ctx),
            Node::BlockQuote {
                content,
                attribution,
            } => {
                render_block_quote(html, content, attribution.as_deref(), ctx);
            }
        }
    }
    ctx.at_top_level = at_top_level;
}

/// Renders one heading, with its section anchor and `:numbered:` number.
fn render_heading(
    html: &mut String,
    level: u8,
    text: &[InlineNode],
    id: Option<&rusty_sphinx_ast::SectionId>,
    is_title: bool,
    ctx: &mut RenderCtx<'_>,
) {
    let tag = format!("h{}", level.clamp(1, 6));
    match id {
        Some(id) => {
            let escaped = html_escape::encode_double_quoted_attribute(id.as_str());
            let _ = write!(html, "<{tag} id=\"{escaped}\">");
        }
        None => {
            let _ = write!(html, "<{tag}>");
        }
    }
    // `:numbered:` shows the same number here as in the navigation, read from
    // the one map the analyzer wrote — computing it a second time is how a
    // toctree saying "2.1." could come to link at a heading rendered "3.4.".
    if let Some(id) = id
        && let Some(number) = heading_secnumber(ctx, id, is_title)
    {
        let _ = write!(
            html,
            "<span class=\"section-number\">{} </span>",
            crate::nav::format_secnumber(&number)
        );
    }
    render_inlines(html, text, ctx);
    let _ = writeln!(html, "</{tag}>");
}

/// The `:numbered:` number shown on one heading.
///
/// A section's number is looked up by its id. The document's *title* heading
/// has no section number — the outline treats it as the document itself — so
/// it takes the document's own number instead.
fn heading_secnumber(
    ctx: &RenderCtx<'_>,
    id: &rusty_sphinx_ast::SectionId,
    is_title: bool,
) -> Option<Vec<usize>> {
    let numbers = ctx.index.section_numbers.get(ctx.original_doc_path)?;
    numbers
        .section(id)
        .or_else(|| is_title.then(|| numbers.document()).flatten())
        .map(<[usize]>::to_vec)
}

/// Renders a `.. toctree::` in a page's body.
///
/// The entries come from *this directive*, not from a flattened tree looked up
/// by document — which is what makes two toctrees in one document, with
/// different options, finally render differently. The sidebar walks the same
/// graph separately, through `page::layout`.
fn render_toctree_directive(html: &mut String, toctree: &Toctree, ctx: &mut RenderCtx<'_>) {
    // `:name:` makes the toctree a `:ref:` target, so its anchor is emitted
    // even for a `:hidden:` toctree — which otherwise renders nothing, and
    // would leave every reference to it dangling.
    if let Some(name) = &toctree.options.name {
        let escaped = html_escape::encode_double_quoted_attribute(name.as_str());
        let _ = writeln!(html, "<a id=\"{escaped}\"></a>");
    }

    if toctree.options.has(ToctreeFlag::Hidden) {
        return;
    }

    let entries = expand_toctree_entries(
        toctree,
        ctx.original_doc_path,
        ctx.index,
        ctx.doc_path,
        ctx.original_doc_path,
    );
    write_nav_list(html, &entries, toctree.options.caption.as_deref());
}

/// Renders a `.. plantuml::` diagram as an `<img>` pointing at the SVG a
/// separate build phase compiled from this block's hashed content (see
/// `extract_diagrams`/`validate_images` in the worker crate).
fn render_plantuml_directive(html: &mut String, content: &HashedContent, ctx: &RenderCtx<'_>) {
    let escaped_hash = html_escape::encode_text(content.hash());

    // The same `_images/` arithmetic an authored `.. image::` uses. The two
    // had private copies of it once, and only this one was right.
    let src = super::asset_href::relative_asset_href(
        std::path::Path::new(&format!("{escaped_hash}.svg")),
        ctx.doc_path,
    );

    let _ = writeln!(html, "<div class=\"plantuml-diagram\">");
    let _ = writeln!(html, "  <img src=\"{src}\" alt=\"PlantUML Diagram\" />");
    let _ = writeln!(html, "</div>");
}

fn render_directive(html: &mut String, directive: &Directive, ctx: &mut RenderCtx<'_>) {
    match directive {
        Directive::Toctree(toctree) => render_toctree_directive(html, toctree, ctx),
        Directive::CodeBlock(block) => {
            super::code_block::render_code_block_directive(html, block, ctx);
        }
        Directive::Highlight {
            language,
            linenothreshold,
            force,
        } => super::code_block::apply_highlight_directive(language, *linenothreshold, *force, ctx),
        Directive::PlantUml(content) => render_plantuml_directive(html, content, ctx),
        Directive::Image(options) => render_image_directive(html, options, ctx),
        Directive::Figure(figure) => render_figure_directive(html, figure, ctx),
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
        Directive::Math {
            parts,
            label,
            nowrap,
            classes,
            span,
        } => render_math(html, parts, label.as_ref(), *nowrap, classes, *span, ctx),
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
        Directive::Table {
            title,
            widths,
            width,
            align,
            classes,
            name,
            header_rows,
            body_rows,
        } => render_table_directive(
            html,
            TableDirectiveParams {
                title: title.as_deref(),
                widths: widths.as_ref(),
                width: width.as_deref(),
                align: *align,
                classes,
                name: name.as_ref(),
                header_rows,
                body_rows,
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
            if let Some(rendered) = render_doctest_block(block, ctx) {
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
                            span: None,
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
