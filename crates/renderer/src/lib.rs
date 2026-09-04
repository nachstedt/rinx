//! The renderer module converts the AST and `ProjectIndex` into HTML.
//!
//! `blocks` renders a document's body — it owns the node dispatcher and one
//! module per block construct — and hands inline markup to `inline`. Both
//! trees resolve cross-references through `resolution`, which is why that
//! one sits flat here rather than under either of them. `page` wraps a
//! rendered body in the templated page chrome and renders the general index;
//! `config` is the site metadata those pages read, and `broken_link` the
//! diagnostics a render reports alongside its HTML. `math` sits flat beside
//! them for the same reason `resolution` does — both `blocks` and `inline`
//! render equations — and is the only module that knows which math backend
//! is in use.

mod blocks;
mod broken_link;
pub mod config;
mod inline;
mod math;
mod nav;
mod page;
mod resolution;

pub use broken_link::{BrokenLink, BrokenLinkKind, ObjectTypeMismatch};
pub use math::MathError;
pub use nav::{PageLink, ResolvedNavEntry};
pub use page::{PageMeta, css_relative_path, render_genindex, render_page};

use blocks::{collect_anonymous_targets, render_nodes};
use math::MathRenderer;
use resolution::{DomainObjectResolver, OptionResolver};
use rusty_sphinx_ast::Document;
use rusty_sphinx_index::ProjectIndex;
use rusty_sphinx_scope::Scope;

/// The result of rendering a document: the body HTML, any cross-references
/// that failed to resolve against the [`ProjectIndex`], any domain-object
/// references that resolved only via an object-type fallback, and any
/// equations the math backend rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOutput {
    pub html: String,
    pub broken_links: Vec<BrokenLink>,
    pub object_type_mismatches: Vec<ObjectTypeMismatch>,
    pub math_errors: Vec<MathError>,
}

/// Shared rendering state threaded through the node traversal.
pub(crate) struct RenderCtx<'a> {
    pub index: &'a ProjectIndex,
    /// Resolves domain-object references against `index`. Held for the whole
    /// document so its derived suffix index is built at most once per page.
    pub domain_resolver: &'a DomainObjectResolver<'a>,
    /// Resolves `:option:` references against `index` — a separate resolver
    /// from `domain_resolver` since the search it performs has no scope
    /// tiers or object-type aliasing (see `resolution::option`'s doc comment).
    pub option_resolver: &'a OptionResolver<'a>,
    pub doc_path: &'a str,
    pub anon_targets: &'a [String],
    pub anon_index: &'a mut usize,
    pub original_doc_path: &'a str,
    pub broken_links: &'a mut Vec<BrokenLink>,
    pub object_type_mismatches: &'a mut Vec<ObjectTypeMismatch>,
    pub math_errors: &'a mut Vec<MathError>,
    /// Converts LaTeX to `MathML`. Held for the whole document so the backend's
    /// per-converter setup happens once per page rather than once per equation.
    pub math: &'a MathRenderer,
    /// The `id` of each top-level heading, keyed by its index in the
    /// document's node list, from [`rusty_sphinx_ast::allocate_section_ids`].
    /// The analyzer builds its document outline from that same function, so a
    /// section link in the navigation and the anchor it lands on cannot drift.
    pub section_ids: &'a std::collections::BTreeMap<usize, rusty_sphinx_ast::SectionId>,
    /// Whether the node list being rendered is the document's own top level.
    /// Only there is a heading a *section* with an id; a heading nested in a
    /// directive body is not one. `render_nodes` clears this for the duration
    /// of any nested list and restores it afterwards.
    pub at_top_level: bool,
    /// The enclosing scope for both domains, mirroring the analyzer's
    /// `index_nodes`/`index_domain_object` scope so a domain object's anchor
    /// `id` always matches the qualified key the analyzer indexed it under.
    /// `.python` carries the enclosing `py:class`/`py:exception` stack and
    /// current `py:module`; `.c` carries the enclosing `c:struct`/`c:union`
    /// stack — see [`rusty_sphinx_scope::Scope`]'s doc comment for why they
    /// stay separate fields rather than being unified further. Both are
    /// pushed/popped by `render_domain_object` around a nested body; the
    /// module component of `.python` is document-order state, never popped.
    pub scope: Scope,
}

/// Renders a Document into HTML, reporting any cross-references that failed to resolve.
#[must_use]
pub fn render(doc: &Document, index: &ProjectIndex, doc_path: &str) -> RenderOutput {
    let mut html = String::new();

    // Collect anonymous targets for local resolution recursively
    let mut anon_targets = Vec::new();
    collect_anonymous_targets(&doc.nodes, &mut anon_targets);
    let mut anon_index = 0;
    let mut broken_links = Vec::new();
    let mut object_type_mismatches = Vec::new();
    let mut math_errors = Vec::new();

    let domain_resolver = DomainObjectResolver::new(index);
    let option_resolver = OptionResolver::new(index);
    let math = MathRenderer::new();
    let section_ids = rusty_sphinx_ast::allocate_section_ids(&doc.nodes);
    let mut ctx = RenderCtx {
        index,
        domain_resolver: &domain_resolver,
        option_resolver: &option_resolver,
        doc_path,
        anon_targets: &anon_targets,
        anon_index: &mut anon_index,
        original_doc_path: &doc.path,
        broken_links: &mut broken_links,
        object_type_mismatches: &mut object_type_mismatches,
        math_errors: &mut math_errors,
        math: &math,
        section_ids: &section_ids,
        at_top_level: true,
        scope: Scope::default(),
    };

    render_nodes(&mut html, &doc.nodes, &mut ctx);

    RenderOutput {
        html,
        broken_links,
        object_type_mismatches,
        math_errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{
        Directive, Enumerator, EnumeratorFormat, EnumeratorSequence, HashedContent, InlineNode,
        ListItem, Node, TargetName, TargetSearchOrder,
    };

    #[test]
    fn test_render_prefixes_a_heading_with_its_section_number() {
        // Given — the analyzer numbered this document's section.
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![
                Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Guide".to_string())],
                },
                Node::Heading {
                    level: 2,
                    text: vec![InlineNode::Text("Install".to_string())],
                },
            ],
        );
        let mut numbers = rusty_sphinx_index::DocumentNumbers::default();
        numbers.set_document(vec![2]);
        numbers.set_section(
            &rusty_sphinx_ast::SectionId::from_title("Install"),
            vec![2, 1],
        );
        let mut index = ProjectIndex::default();
        index
            .section_numbers
            .insert("guide.rst".to_string(), numbers);

        // When
        let result = render(&doc, &index, "guide").html;

        // Then — the title takes the document's number, the section its own.
        assert!(
            result.contains("<h1 id=\"guide\"><span class=\"section-number\">2. </span>Guide</h1>"),
            "{result}"
        );
        assert!(
            result.contains(
                "<h2 id=\"install\"><span class=\"section-number\">2.1. </span>Install</h2>"
            ),
            "{result}"
        );
    }

    #[test]
    fn test_render_leaves_an_unnumbered_heading_unprefixed() {
        // Given — no `:numbered:` toctree reaches this document.
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Guide".to_string())],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, "guide").html;

        // Then
        assert_eq!(result, "<h1 id=\"guide\">Guide</h1>\n");
    }

    #[test]
    fn test_render_returns_empty_string_for_empty_document() {
        // Given
        let doc = Document::new("test.rst".to_string(), vec![]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "");
    }
    #[test]
    fn test_render_formats_heading_and_paragraph_nodes() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Title".to_string())],
                },
                Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                    "Paragraph".to_string(),
                )]),
                Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Another Heading".to_string())],
                },
            ],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<h1 id=\"title\">Title</h1>\n<p>Paragraph</p>\n<h1 id=\"another-heading\">Another Heading</h1>\n"
        );
    }
    #[test]
    fn test_render_heading_resolves_domain_object_reference_as_link() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![
                    InlineNode::Text("The ".to_string()),
                    InlineNode::DomainObjectReference {
                        object_type: rusty_sphinx_ast::ObjectType::Py(
                            rusty_sphinx_ast::PyObjectType::Module,
                        ),
                        name: "greetings".to_string(),
                        display: "greetings".to_string(),
                        link: true,
                        search_order: TargetSearchOrder::LeastQualifiedFirst,
                        span: None,
                    },
                    InlineNode::Text(" Module".to_string()),
                ],
            }],
        );
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            rusty_sphinx_ast::ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
            "greetings",
            "api.rst",
        );

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.starts_with("<h1 id=\"the-greetings-module\">The "));
        assert!(
            result
                .contains("<a class=\"reference internal\" href=\"api.html#py:module:greetings\">")
        );
        assert!(result.ends_with(" Module</h1>\n"));
    }
    #[test]
    fn test_render_escapes_html_special_characters() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Title <script>".to_string())],
                },
                Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                    "A & B > C".to_string(),
                )]),
            ],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<h1 id=\"title-script\">Title &lt;script&gt;</h1>\n<p>A &amp; B &gt; C</p>\n"
        );
    }
    #[test]
    fn test_render_formats_heading_level_1_as_h1() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Top".to_string())],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<h1 id=\"top\">Top</h1>\n");
    }
    #[test]
    fn test_render_formats_heading_level_2_as_h2() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 2,
                text: vec![InlineNode::Text("Sub".to_string())],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<h2 id=\"sub\">Sub</h2>\n");
    }
    #[test]
    fn test_render_formats_heading_level_6_as_h6() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 6,
                text: vec![InlineNode::Text("Deep".to_string())],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<h6 id=\"deep\">Deep</h6>\n");
    }
    #[test]
    fn test_render_clamps_heading_level_above_6_to_h6() {
        // Given — level 7 exceeds the HTML maximum of 6
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 7,
                text: vec![InlineNode::Text("VeryDeep".to_string())],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<h6 id=\"verydeep\">VeryDeep</h6>\n");
    }
    #[test]
    fn test_render_ignores_unknown_directive() {
        // Given a document with an unknown directive
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Unknown {
                name: "some-unknown".to_string(),
                argument: "arg".to_string(),
                body: "body".to_string(),
            })],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then the output should be empty, as unknown directives are ignored
        assert_eq!(result, "");
    }
    #[test]
    fn test_render_formats_target_node_as_html_anchor() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Target {
                name: TargetName::new("section-1"),
                uri: None,
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<a id=\"section-1\"></a>\n");
    }
    #[test]
    fn test_render_formats_transition_node_as_horizontal_rule() {
        // Given
        let doc = Document::new("test.rst".to_string(), vec![Node::Transition]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<hr />\n");
    }
    #[test]
    fn test_render_suppresses_anchor_for_external_target() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Target {
                name: TargetName::new("google"),
                uri: Some("https://google.com".to_string()),
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "");
    }
    #[test]
    fn test_render_formats_inline_reference_using_project_index() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Reference {
                    display: "other-section".to_string(),
                    target: "other-section".to_string(),
                    span: None,
                },
            ])],
        );
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("other-section"),
            rusty_sphinx_index::TargetLocation::Internal("other_file.rst".to_string()),
        );

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<p><a href=\"other_file.html#other-section\">other-section</a></p>\n"
        );
    }
    #[test]
    fn test_render_reports_no_broken_links_when_all_references_resolve() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Reference {
                    display: "other-section".to_string(),
                    target: "other-section".to_string(),
                    span: None,
                },
            ])],
        );
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("other-section"),
            rusty_sphinx_index::TargetLocation::Internal("other_file.rst".to_string()),
        );

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert!(result.broken_links.is_empty());
    }
    #[test]
    fn test_render_collects_broken_links_across_multiple_reference_kinds() {
        // Given a document with a broken :ref: and a broken :term:
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Reference {
                    display: "missing-ref".to_string(),
                    target: "missing-ref".to_string(),
                    span: None,
                },
                rusty_sphinx_ast::InlineNode::TermReference {
                    display: "missing term".to_string(),
                    term: "missing-term".to_string(),
                    span: None,
                },
            ])],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert_eq!(
            result.broken_links,
            vec![
                crate::BrokenLink {
                    kind: crate::BrokenLinkKind::Reference,
                    target: "missing-ref".to_string(),
                    span: None,
                },
                crate::BrokenLink {
                    kind: crate::BrokenLinkKind::TermReference,
                    target: "missing-term".to_string(),
                    span: None,
                },
            ]
        );
    }
    #[test]
    fn test_render_resolves_cross_directory_references_as_relative_links() {
        // Given a document in a subdirectory
        let doc = Document::new(
            "examples/team_b/index.rst".to_string(),
            vec![Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Reference {
                    display: "target-in-a".to_string(),
                    target: "target-in-a".to_string(),
                    span: None,
                },
            ])],
        );

        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("target-in-a"),
            rusty_sphinx_index::TargetLocation::Internal("examples/team_a/index.rst".to_string()),
        );

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then the link should point backwards up out of team_b/ and into team_a/
        assert_eq!(
            result,
            "<p><a href=\"../team_a/index.html#target-in-a\">target-in-a</a></p>\n"
        );
    }
    #[test]
    fn test_render_formats_external_hyperlink() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Hyperlink {
                    text: "Python".to_string(),
                    target: "Python".to_string(),
                    span: None,
                },
            ])],
        );
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("Python"),
            rusty_sphinx_index::TargetLocation::External("https://python.org".to_string()),
        );

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<p><a href=\"https://python.org\">Python</a></p>\n");
    }
    #[test]
    fn test_render_formats_direct_uri_hyperlink() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Hyperlink {
                    text: "Google".to_string(),
                    target: "https://google.com".to_string(),
                    span: None,
                },
            ])],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<p><a href=\"https://google.com\">Google</a></p>\n");
    }
    #[test]
    fn test_render_formats_plantuml_with_relative_path() {
        // Given a document in a subdirectory
        let content = HashedContent::new("A -> B".to_string());
        let expected_hash = content.hash().to_string();
        let doc = Document::new(
            "examples/team_b/index.rst".to_string(),
            vec![Node::Directive(Directive::PlantUml(content))],
        );

        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then the image src should point backwards up out of team_b/ and examples/ and into _images/
        let expected = format!(
            "<div class=\"plantuml-diagram\">\n  <img src=\"../../_images/{expected_hash}.svg\" alt=\"PlantUML Diagram\" />\n</div>\n"
        );
        assert_eq!(result, expected);
    }
    #[test]
    fn test_render_collects_an_anonymous_target_nested_in_a_list_item() {
        // Given an anonymous hyperlink target written inside a list item
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::EnumeratedList {
                    start: Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 1)
                        .unwrap(),
                    items: vec![ListItem {
                        nodes: vec![Node::AnonymousTarget {
                            uri: "https://example.com/".to_string(),
                        }],
                    }],
                },
                Node::Paragraph(vec![InlineNode::AnonymousReference {
                    text: "here".to_string(),
                    span: None,
                }]),
            ],
        );
        let index = ProjectIndex::default();

        // When rendering the document
        let result = render(&doc, &index, &doc.path).html;

        // Then the reference resolves: the collector descends into list items
        // rather than stopping at the container, as it once did
        assert!(result.contains("https://example.com/"), "{result}");
    }
    #[test]
    fn test_render_resolves_anonymous_links_in_order() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Paragraph(vec![
                    InlineNode::AnonymousReference {
                        text: "First".to_string(),
                        span: None,
                    },
                    InlineNode::Text(" and ".to_string()),
                    InlineNode::AnonymousReference {
                        text: "Second".to_string(),
                        span: None,
                    },
                ]),
                Node::AnonymousTarget {
                    uri: "https://first.com".to_string(),
                },
                Node::AnonymousTarget {
                    uri: "https://second.com".to_string(),
                },
            ],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains("<a href=\"https://first.com\">First</a>"));
        assert!(result.contains("<a href=\"https://second.com\">Second</a>"));
    }
    #[test]
    fn test_render_anonymous_hyperlink_with_embedded_uri_does_not_consume_targets() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Paragraph(vec![
                    InlineNode::AnonymousHyperlink {
                        text: "Embedded".to_string(),
                        target: "https://embedded.com".to_string(),
                    },
                    InlineNode::Text(" then ".to_string()),
                    InlineNode::AnonymousReference {
                        text: "Reference".to_string(),
                        span: None,
                    },
                ]),
                Node::AnonymousTarget {
                    uri: "https://target.com".to_string(),
                },
            ],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains("<a href=\"https://embedded.com\">Embedded</a>"));
        assert!(result.contains("<a href=\"https://target.com\">Reference</a>"));
    }
    #[test]
    fn test_render_comment_produces_no_html() {
        // Given
        let doc = Document::new("test.rst".to_string(), vec![Node::Comment]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "");
    }
}
