//! The renderer module converts the AST and `ProjectIndex` into HTML.

mod admonitions;
mod blocks;
pub mod config;
mod doctest;
mod domain_object;
mod domain_resolution;
mod genindex;
mod glossary;
mod inline;
mod list_table;
mod nav;
mod option_resolution;
mod page;
mod scope_directives;
mod tables;

pub use genindex::render_genindex;
pub use page::{PageMeta, css_relative_path, render_page};

use blocks::{collect_anonymous_targets, render_nodes};
use domain_resolution::DomainObjectResolver;
use option_resolution::OptionResolver;
use rusty_sphinx_ast::{Document, ObjectType};
use rusty_sphinx_index::ProjectIndex;
use rusty_sphinx_scope::Scope;

/// The kind of cross-reference role that produced a [`BrokenLink`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokenLinkKind {
    /// A `:ref:` role (`InlineNode::Reference`).
    Reference,
    /// A named hyperlink (`InlineNode::Hyperlink`).
    Hyperlink,
    /// An anonymous `__` reference with no matching anonymous target left.
    AnonymousReference,
    /// A `:term:` role (`InlineNode::TermReference`).
    TermReference,
    /// A `:option:` role (`InlineNode::OptionReference`).
    OptionReference,
    /// A domain-object role (`:func:`, `:py:func:`, etc.). Carries the object
    /// type the role asked for (e.g. `py:function`) — the "missed type", known
    /// at the point resolution failed and worth surfacing in diagnostics even
    /// though nothing resolved.
    DomainObjectReference(ObjectType),
    /// A dot-prefixed domain-object role whose suffix search matched several
    /// objects, so the target names no single one. Deliberately unresolved
    /// rather than linked to an arbitrary candidate (real Sphinx links the
    /// first) — the candidates are carried here so the author is told what to
    /// disambiguate between.
    AmbiguousDomainObjectReference {
        object_type: ObjectType,
        /// The qualified names that matched, in index order.
        candidates: Vec<String>,
    },
}

impl BrokenLinkKind {
    /// Returns a short, human-readable label for this kind, used in CLI diagnostics.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Reference => "ref",
            Self::Hyperlink => "hyperlink",
            Self::AnonymousReference => "anonymous reference",
            Self::TermReference => "term",
            Self::OptionReference => "option",
            Self::DomainObjectReference(_) => "domain object",
            Self::AmbiguousDomainObjectReference { .. } => "ambiguous domain object",
        }
    }
}

/// A cross-reference that failed to resolve while rendering a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenLink {
    pub kind: BrokenLinkKind,
    pub target: String,
}

/// A domain-object reference that *did* resolve, but only via
/// [`rusty_sphinx_ast::ObjectType::role_alias_candidates`]'s fallback — the
/// definition's own object type doesn't match the one the role asked for
/// (e.g. a `:exc:` role resolved against a `.. class::` definition, as
/// `CPython`'s `xmlrpc.client.rst` does with `Fault`). Deliberately not a
/// [`BrokenLink`]: the reference works and the build is never failed for it
/// (not even under `--strict-links`) — this only flags a source
/// inconsistency the author may want to clean up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectTypeMismatch {
    /// The qualified name the reference resolved against.
    pub name: String,
    /// The object type the role asked for (e.g. `exception`, from `:exc:`).
    pub requested_type: ObjectType,
    /// The object type the definition actually has (e.g. `class`).
    pub resolved_type: ObjectType,
}

/// The result of rendering a document: the body HTML, any cross-references
/// that failed to resolve against the [`ProjectIndex`], and any domain-object
/// references that resolved only via an object-type fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOutput {
    pub html: String,
    pub broken_links: Vec<BrokenLink>,
    pub object_type_mismatches: Vec<ObjectTypeMismatch>,
}

/// Shared rendering state threaded through the node traversal.
pub(crate) struct RenderCtx<'a> {
    pub index: &'a ProjectIndex,
    /// Resolves domain-object references against `index`. Held for the whole
    /// document so its derived suffix index is built at most once per page.
    pub domain_resolver: &'a DomainObjectResolver<'a>,
    /// Resolves `:option:` references against `index` — a separate resolver
    /// from `domain_resolver` since the search it performs has no scope
    /// tiers or object-type aliasing (see `option_resolution`'s doc comment).
    pub option_resolver: &'a OptionResolver<'a>,
    pub doc_path: &'a str,
    pub anon_targets: &'a [String],
    pub anon_index: &'a mut usize,
    pub original_doc_path: &'a str,
    pub broken_links: &'a mut Vec<BrokenLink>,
    pub object_type_mismatches: &'a mut Vec<ObjectTypeMismatch>,
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

    let domain_resolver = DomainObjectResolver::new(index);
    let option_resolver = OptionResolver::new(index);
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
        scope: Scope::default(),
    };

    render_nodes(&mut html, &doc.nodes, &mut ctx);

    RenderOutput {
        html,
        broken_links,
        object_type_mismatches,
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
            "<h1>Title</h1>\n<p>Paragraph</p>\n<h1>Another Heading</h1>\n"
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
        assert!(result.starts_with("<h1>The "));
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
            "<h1>Title &lt;script&gt;</h1>\n<p>A &amp; B &gt; C</p>\n"
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
        assert_eq!(result, "<h1>Top</h1>\n");
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
        assert_eq!(result, "<h2>Sub</h2>\n");
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
        assert_eq!(result, "<h6>Deep</h6>\n");
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
        assert_eq!(result, "<h6>VeryDeep</h6>\n");
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
                },
                rusty_sphinx_ast::InlineNode::TermReference {
                    display: "missing term".to_string(),
                    term: "missing-term".to_string(),
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
                },
                crate::BrokenLink {
                    kind: crate::BrokenLinkKind::TermReference,
                    target: "missing-term".to_string(),
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
                Node::Paragraph(vec![InlineNode::AnonymousReference("here".to_string())]),
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
                    InlineNode::AnonymousReference("First".to_string()),
                    InlineNode::Text(" and ".to_string()),
                    InlineNode::AnonymousReference("Second".to_string()),
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
                    InlineNode::AnonymousReference("Reference".to_string()),
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
