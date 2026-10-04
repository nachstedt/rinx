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

mod asset_href;
mod blocks;
mod broken_link;
mod chart;
pub mod config;
mod embedded_assets;
mod empty_listing_error;
mod highlight;
mod hyperlink_target;
mod image_error;
mod inline;
mod math;
mod nav;
mod numbering;
mod numfig_format;
mod octicon;
mod page;
mod registry_base_url;
mod reported;
mod resolution;
#[cfg(test)]
mod test_support;
mod uml_error;

pub use blocks::EntityTemplates;
pub use broken_link::{BrokenLink, BrokenLinkKind, ObjectTypeMismatch};
pub use embedded_assets::EmbeddedAssets;
pub use empty_listing_error::EmptyListingError;
pub use highlight::{HighlightError, HighlightErrorKind, HighlightedConstruct};
pub use image_error::ImageError;
pub use math::MathError;
pub use nav::{PageLink, ResolvedNavEntry};
pub use page::{
    MODINDEX_PATH, MODINDEX_TITLE, PageMeta, css_relative_path, render_genindex, render_modindex,
    render_page,
};
pub use uml_error::{DiagramError, DiagramFailure};

use rinx_ast::HashedContent;

use blocks::{EntityUpdateVisibility, collect_anonymous_targets, render_nodes};
use highlight::Highlighter;
use math::MathRenderer;
use resolution::{DomainObjectResolver, OptionResolver};
use rinx_ast::{Document, ResolvedLanguage};
use rinx_index::ProjectIndex;
use rinx_scope::DocumentScopes;

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
    /// Listing directives whose filter matched nothing. Only reportable here:
    /// whether a filter selects anything depends on the whole project.
    pub empty_listing_errors: Vec<EmptyListingError>,
    /// Diagrams whose template could not be expanded against the project.
    pub diagram_errors: Vec<DiagramError>,
    /// The `PlantUML` text of every diagram on the page, one entry per
    /// distinct hash, in document order.
    ///
    /// Handed back rather than written, because this crate performs no I/O:
    /// the render subcommand writes each as `<hash>.puml` for the compile
    /// action. The page's `<img>` and the file it names therefore come from
    /// this one render, which is what makes them agree by construction.
    pub diagram_sources: Vec<HashedContent>,
    pub highlight_errors: Vec<HighlightError>,
    /// Images whose `:loading: embed` could not be honoured because the bytes
    /// never reached this render. Every other image problem is decidable from
    /// the source alone and is reported by the parser.
    pub image_errors: Vec<ImageError>,
    /// Entity templates a type named but the site could not use. A build
    /// misconfiguration rather than a document fault, so it is reported
    /// separately from the broken links and never fails `--strict-links`.
    pub entity_template_errors: Vec<String>,
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
    /// Resolves entity roles against `index`, under the project's schema.
    pub entity_resolver: &'a crate::resolution::EntityResolver<'a>,
    /// The project's entity meta-model, for the labels and the relation
    /// vocabulary the default rendering reads.
    pub schema: &'a rinx_entity::EntitySchema,
    /// Per-type entity templates the site supplied, keyed by the name a type
    /// refers to. Empty for a site that supplies none, which is when every
    /// entity uses the built-in rendering.
    pub entity_templates: &'a blocks::EntityTemplates,
    /// Templates that were named but could not be used. A build
    /// misconfiguration rather than a document fault, so it is collected here
    /// and reported by the caller, not turned into a broken link.
    pub entity_template_errors: &'a mut Vec<String>,
    pub doc_path: &'a str,
    pub anon_targets: &'a [rinx_ast::LinkDestination],
    pub anon_index: &'a mut usize,
    /// The targets a `` `name`_ `` written in this document may reach, which
    /// docutils keeps local to it — see [`hyperlink_target`].
    pub hyperlink_targets: &'a hyperlink_target::DocumentHyperlinkTargets,
    pub original_doc_path: &'a str,
    pub broken_links: &'a mut Vec<BrokenLink>,
    pub object_type_mismatches: &'a mut Vec<ObjectTypeMismatch>,
    pub math_errors: &'a mut Vec<MathError>,
    pub empty_listing_errors: &'a mut Vec<EmptyListingError>,
    /// Where a diagram whose template failed to expand is recorded.
    pub diagram_errors: &'a mut Vec<DiagramError>,
    /// Where each diagram's expanded text is recorded for the compile action.
    pub diagram_sources: &'a mut Vec<HashedContent>,
    /// Converts LaTeX to `MathML`. Held for the whole document so the backend's
    /// per-converter setup happens once per page rather than once per equation.
    pub math: &'a MathRenderer,
    pub highlight_errors: &'a mut Vec<HighlightError>,
    pub image_errors: &'a mut Vec<ImageError>,
    /// The `data:` URIs for this document's `:loading: embed` images, supplied
    /// by the build. Empty whenever nothing was embedded — and for every
    /// caller that renders without an asset sidecar, which then reports each
    /// embed request as unavailable rather than silently linking.
    pub embedded_assets: &'a EmbeddedAssets,
    /// Turns source text into classed HTML. Held for the whole document so
    /// the grammar set is resolved once per page, not once per code block.
    pub highlighter: &'a Highlighter,
    /// The language a code block with no argument of its own inherits.
    ///
    /// Document-order state: it starts at the site's configured
    /// `highlight_language` and every `.. highlight::` replaces it for the
    /// blocks that follow. Never restored on leaving a nested body — Sphinx
    /// scopes this to the enclosing container, and the narrower whole-document
    /// rule is a deliberate simplification recorded in `docs/compatibility.rst`.
    pub highlight_language: ResolvedLanguage,
    /// Whether the built-in entity rendering folds its detail behind a
    /// disclosure. From the site config; see [`config::SiteConfig`].
    pub collapse_entities: bool,
    /// Whether `.. entity-update::`/`.. needextend::` renders its own visible
    /// box. From the site config; see [`config::SiteConfig::show_entity_updates`].
    ///
    /// A two-variant enum rather than a `bool` — unlike every sibling flag
    /// here, which predates it — specifically so this field does not push
    /// `RenderCtx` over clippy's `struct_excessive_bools` threshold; the
    /// config file itself stays a plain `bool`, which is the natural TOML
    /// shape, and is converted once where this context is built.
    pub show_entity_updates: EntityUpdateVisibility,
    /// The site's named `PlantUML` preambles, for a diagram's `:config:`.
    pub uml_configs: &'a std::collections::BTreeMap<String, String>,
    /// The PEP index a `:pep:` links into, from the site config.
    pub pep_base_url: &'a config::PepBaseUrl,
    /// The RFC index an `:rfc:` links into, from the site config.
    pub rfc_base_url: &'a config::RfcBaseUrl,
    /// `:linenothreshold:` from the `.. highlight::` in force: a block at
    /// least this many lines long gets line numbers without asking for them.
    pub linenothreshold: Option<std::num::NonZeroU32>,
    /// `:force:` from the `.. highlight::` in force, which suppresses the
    /// highlighting diagnostics for every block inheriting it.
    pub highlight_force: bool,
    /// The `id` of each top-level heading, keyed by its index in the
    /// document's node list, from [`rinx_ast::allocate_section_ids`].
    /// The analyzer builds its document outline from that same function, so a
    /// section link in the navigation and the anchor it lands on cannot drift.
    pub section_ids: &'a std::collections::BTreeMap<usize, rinx_ast::SectionId>,
    /// Whether the node list being rendered is the document's own top level.
    /// Only there is a heading a *section* with an id; a heading nested in a
    /// directive body is not one. `render_nodes` clears this for the duration
    /// of any nested list and restores it afterwards.
    pub at_top_level: bool,
    /// Where a heading should link back to, for a `.. contents::` covering it
    /// with `:backlinks:` other than `none`, keyed by the heading's own id.
    ///
    /// Populated while rendering a `.. contents::` directive (see
    /// `blocks::contents`) and consulted by `render_heading`, which normally
    /// runs *after* it since a table of contents is almost always written
    /// before the sections it lists — a `.. contents::` placed after its own
    /// sections will not backlink them, a deliberate document-order
    /// limitation of this single left-to-right rendering pass.
    pub contents_backlinks: &'a mut std::collections::HashMap<rinx_ast::SectionId, String>,
    /// Hands out a self-anchor id to a `.. contents::` with no explicit
    /// `:name:`.
    ///
    /// Seeded from every id in `section_ids` before rendering starts, so an
    /// auto-generated contents anchor can never collide with a heading's —
    /// and kept as one allocator for the whole document, rather than a fresh
    /// one per directive, so two un-named `.. contents::` blocks (both
    /// defaulting to the title "Contents") are disambiguated against each
    /// other too, exactly as docutils' single shared id registry would.
    pub contents_id_allocator: &'a mut rinx_ast::SectionIdAllocator,
    /// `numfig`'s settings and this page's caption numbers.
    pub numbering: &'a numbering::Numbering<'a>,
    /// The names every domain object on the page is qualified to, and the
    /// scope in force at every reference that resolves against one —
    /// computed once for the document, the same answer the analyzer indexed
    /// it with, so a domain object's anchor `id` always matches its index key.
    pub scopes: &'a DocumentScopes<'a>,
}

/// Renders a Document into HTML, reporting any cross-references that failed to resolve.
#[must_use]
pub fn render(doc: &Document, index: &ProjectIndex, doc_path: &str) -> RenderOutput {
    render_with_config(doc, index, doc_path, &config::SiteConfig::default())
}

/// Renders a document under a fully specified [`config::SiteConfig`] — the
/// entry point for callers that have one, which is every caller that reads a
/// `rinx.toml`.
///
/// [`render`] is a thin wrapper defaulting the config, kept because most of
/// this crate's own tests have no site configuration to speak of and only the
/// site-wide `highlight_language` reads from it at all.
#[must_use]
pub fn render_with_config(
    doc: &Document,
    index: &ProjectIndex,
    doc_path: &str,
    config: &config::SiteConfig,
) -> RenderOutput {
    render_with_assets(
        doc,
        index,
        doc_path,
        config,
        &EmbeddedAssets::new(),
        rinx_entity::EntitySchema::empty_ref(),
        &blocks::EntityTemplates::new(),
    )
}

/// Renders a document with the `data:` URIs for its `:loading: embed` images
/// — the full form every other entry point defaults a piece of.
///
/// A separate entry point rather than a fifth parameter on
/// [`render_with_config`], because embedding is the one input that arrives
/// from a *build step* rather than from the document or its site config: only
/// the worker, holding the `embed_assets` sidecar, ever has one to pass.
#[must_use]
pub fn render_with_assets(
    doc: &Document,
    index: &ProjectIndex,
    doc_path: &str,
    config: &config::SiteConfig,
    embedded_assets: &EmbeddedAssets,
    schema: &rinx_entity::EntitySchema,
    entity_templates: &blocks::EntityTemplates,
) -> RenderOutput {
    let mut html = String::new();

    // Collect anonymous targets for local resolution recursively
    let mut anon_targets = Vec::new();
    collect_anonymous_targets(&doc.nodes, &mut anon_targets);
    let mut anon_index = 0;
    let mut broken_links = Vec::new();
    let mut object_type_mismatches = Vec::new();
    let mut math_errors = Vec::new();
    let mut empty_listing_errors = Vec::new();
    let mut diagram_errors = Vec::new();
    let mut diagram_sources = Vec::new();
    let mut highlight_errors = Vec::new();
    let mut image_errors = Vec::new();
    let mut contents_backlinks = std::collections::HashMap::new();

    let domain_resolver = DomainObjectResolver::new(index);
    let option_resolver = OptionResolver::new(index);
    let entity_resolver = crate::resolution::EntityResolver::new(index, schema);
    let mut entity_template_errors = Vec::new();
    let math = MathRenderer::new();
    let highlighter = Highlighter::new();
    let section_ids = rinx_ast::allocate_section_ids(&doc.nodes);
    let hyperlink_targets =
        hyperlink_target::DocumentHyperlinkTargets::collect(doc, index, &section_ids);
    let mut contents_id_allocator = rinx_ast::SectionIdAllocator::new();
    for id in section_ids.values() {
        contents_id_allocator.seed(id);
    }
    let numbering = numbering::Numbering::new(doc, index, config);
    let scopes = DocumentScopes::of(&doc.nodes);
    let mut ctx = RenderCtx {
        numbering: &numbering,
        index,
        domain_resolver: &domain_resolver,
        option_resolver: &option_resolver,
        entity_resolver: &entity_resolver,
        schema,
        entity_templates,
        entity_template_errors: &mut entity_template_errors,
        doc_path,
        anon_targets: &anon_targets,
        anon_index: &mut anon_index,
        hyperlink_targets: &hyperlink_targets,
        original_doc_path: &doc.path,
        broken_links: &mut broken_links,
        object_type_mismatches: &mut object_type_mismatches,
        math_errors: &mut math_errors,
        empty_listing_errors: &mut empty_listing_errors,
        diagram_errors: &mut diagram_errors,
        diagram_sources: &mut diagram_sources,
        math: &math,
        highlight_errors: &mut highlight_errors,
        image_errors: &mut image_errors,
        embedded_assets,
        highlighter: &highlighter,
        highlight_language: config.highlight_language.clone(),
        collapse_entities: config.collapse_entities,
        show_entity_updates: EntityUpdateVisibility::from_config(config.show_entity_updates),
        uml_configs: &config.uml_configs,
        pep_base_url: &config.pep_base_url,
        rfc_base_url: &config.rfc_base_url,
        linenothreshold: None,
        highlight_force: false,
        section_ids: &section_ids,
        at_top_level: true,
        contents_backlinks: &mut contents_backlinks,
        contents_id_allocator: &mut contents_id_allocator,
        scopes: &scopes,
    };

    render_nodes(&mut html, &doc.nodes, &mut ctx);

    RenderOutput {
        html,
        broken_links,
        object_type_mismatches,
        math_errors,
        empty_listing_errors,
        diagram_errors,
        diagram_sources,
        highlight_errors,
        image_errors,
        entity_template_errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{
        Directive, Enumerator, EnumeratorFormat, EnumeratorSequence, InlineNode, ListItem, Node,
        TargetName, TargetSearchOrder, Uml, UmlSource,
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
        let mut numbers = rinx_index::DocumentNumbers::default();
        numbers.set_document(vec![2]);
        numbers.set_section(&rinx_ast::SectionId::from_title("Install"), vec![2, 1]);
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
    fn test_render_wraps_a_sectnum_numbered_heading_in_its_prefix_and_suffix() {
        // Given — a `.. sectnum::`-numbered document (no document number, only
        // its section, with `:prefix:`/`:suffix:` set).
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Heading {
                level: 2,
                text: vec![InlineNode::Text("Install".to_string())],
            }],
        );
        let mut numbers = rinx_index::DocumentNumbers::default();
        numbers.set_section(&rinx_ast::SectionId::from_title("Install"), vec![1]);
        numbers.set_format("Appendix ".to_string(), ".".to_string());
        let mut index = ProjectIndex::default();
        index
            .section_numbers
            .insert("guide.rst".to_string(), numbers);

        // When
        let result = render(&doc, &index, "guide").html;

        // Then
        assert!(
            result.contains(
                "<h2 id=\"install\"><span class=\"section-number\">Appendix 1.. </span>Install</h2>"
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
                Node::Paragraph(vec![rinx_ast::InlineNode::Text("Paragraph".to_string())]),
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
                        object_type: rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Module),
                        name: "greetings".to_string(),
                        display: "greetings".to_string(),
                        link: true,
                        search_order: TargetSearchOrder::LeastQualifiedFirst,
                        span: None,
                        inventory: rinx_ast::InventorySelector::Any,
                    },
                    InlineNode::Text(" Module".to_string()),
                ],
            }],
        );
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Module),
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
                Node::Paragraph(vec![rinx_ast::InlineNode::Text("A & B > C".to_string())]),
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
    fn test_render_shows_an_unknown_directive_as_a_visible_error_block() {
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

        // Then the page says what it could not render, and quotes the source
        // rather than dropping it
        assert!(result.contains("class=\"directive-error\""));
        assert!(result.contains("unknown directive type 'some-unknown'"));
        assert!(result.contains(".. some-unknown:: arg\n   body"));
    }
    #[test]
    fn test_render_formats_target_node_as_html_anchor() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Target {
                name: TargetName::new("section-1"),
                destination: None,
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
                destination: Some(rinx_ast::LinkDestination::Uri(
                    "https://google.com".to_string(),
                )),
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
            vec![Node::Paragraph(vec![rinx_ast::InlineNode::Reference {
                display: None,
                target: "other-section".to_string(),
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }])],
        );
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("other-section"),
            "other_file.rst".to_string(),
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
            vec![Node::Paragraph(vec![rinx_ast::InlineNode::Reference {
                display: None,
                target: "other-section".to_string(),
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }])],
        );
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("other-section"),
            "other_file.rst".to_string(),
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
                rinx_ast::InlineNode::Reference {
                    display: None,
                    target: "missing-ref".to_string(),
                    span: None,
                    inventory: rinx_ast::InventorySelector::Any,
                },
                rinx_ast::InlineNode::TermReference {
                    display: "missing term".to_string(),
                    term: "missing-term".to_string(),
                    span: None,
                    inventory: rinx_ast::InventorySelector::Any,
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
            vec![Node::Paragraph(vec![rinx_ast::InlineNode::Reference {
                display: None,
                target: "target-in-a".to_string(),
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }])],
        );

        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("target-in-a"),
            "examples/team_a/index.rst".to_string(),
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
            vec![
                Node::Paragraph(vec![rinx_ast::InlineNode::Hyperlink {
                    text: "Python".to_string(),
                    target: rinx_ast::HyperlinkTarget::Reference(TargetName::new("Python")),
                    span: None,
                }]),
                Node::Target {
                    name: TargetName::new("Python"),
                    destination: Some(rinx_ast::LinkDestination::Uri(
                        "https://python.org".to_string(),
                    )),
                },
            ],
        );

        // When
        let result = render(&doc, &ProjectIndex::default(), &doc.path).html;

        // Then
        assert_eq!(result, "<p><a href=\"https://python.org\">Python</a></p>\n");
    }
    #[test]
    fn test_render_links_each_documents_own_external_target() {
        // Given — CPython's `Python Packaging User Guide`_, which two
        // documents point at two different pages.
        let page = |path: &str, url: &str| {
            Document::new(
                path.to_string(),
                vec![
                    Node::Paragraph(vec![rinx_ast::InlineNode::Hyperlink {
                        text: "guide".to_string(),
                        target: rinx_ast::HyperlinkTarget::Reference(TargetName::new(
                            "Python Packaging User Guide",
                        )),
                        span: None,
                    }]),
                    Node::Target {
                        name: TargetName::new("Python Packaging User Guide"),
                        destination: Some(rinx_ast::LinkDestination::Uri(url.to_string())),
                    },
                ],
            )
        };
        let distributing = page("distributing.rst", "https://packaging.python.org/");
        let mac = page("mac.rst", "https://packaging.python.org/tutorials/");
        let index = ProjectIndex::default();

        // When
        let distributing_html = render(&distributing, &index, &distributing.path).html;
        let mac_html = render(&mac, &index, &mac.path).html;

        // Then
        assert!(distributing_html.contains("href=\"https://packaging.python.org/\""));
        assert!(mac_html.contains("href=\"https://packaging.python.org/tutorials/\""));
    }
    #[test]
    fn test_render_formats_direct_uri_hyperlink() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![rinx_ast::InlineNode::Hyperlink {
                text: "Google".to_string(),
                target: rinx_ast::HyperlinkTarget::Embedded(rinx_ast::LinkDestination::Uri(
                    "https://google.com".to_string(),
                )),
                span: None,
            }])],
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
        let uml = Uml::new(UmlSource::PlantUml, "A -> B".to_string());
        let expected_hash = rinx_uml::expand(
            &uml,
            &rinx_uml::UmlContext::new(
                &ProjectIndex::default(),
                rinx_entity::EntitySchema::empty_ref(),
                "examples/team_b/index.rst",
            ),
        )
        .expect("expansion succeeds")
        .hash()
        .to_string();
        let doc = Document::new(
            "examples/team_b/index.rst".to_string(),
            vec![Node::Directive(Directive::Uml(Box::new(uml)))],
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
                            destination: rinx_ast::LinkDestination::Uri(
                                "https://example.com/".to_string(),
                            ),
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
                    destination: rinx_ast::LinkDestination::Uri("https://first.com".to_string()),
                },
                Node::AnonymousTarget {
                    destination: rinx_ast::LinkDestination::Uri("https://second.com".to_string()),
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
                        target: rinx_ast::LinkDestination::Uri("https://embedded.com".to_string()),
                    },
                    InlineNode::Text(" then ".to_string()),
                    InlineNode::AnonymousReference {
                        text: "Reference".to_string(),
                        span: None,
                    },
                ]),
                Node::AnonymousTarget {
                    destination: rinx_ast::LinkDestination::Uri("https://target.com".to_string()),
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
