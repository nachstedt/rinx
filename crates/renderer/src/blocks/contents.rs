//! Rendering `.. contents::`: the local, single-document table of contents
//! built from a document's own heading tree.
//!
//! Unlike an in-page `.. toctree::` (see `super::dispatch::render_toctree_directive`,
//! which expands the project-wide document/section graph), this directive
//! only ever lists the *current* document's own [`OutlineSection`] tree, and
//! that tree is already sitting in `ctx.index.document_outlines` — built once
//! by the analyzer from the exact same `allocate_section_ids` call this
//! render pass itself used for heading anchors (see `docs/decisions/005-toctree-model.md`
//! §3). So no new indexing pass is needed; the only render-time work is
//! finding the directive's *own* enclosing section for `:local:`, which is a
//! plain backward scan over `nodes` — see [`enclosing_heading_id`].
//!
//! `:backlinks:` entries are written with their own dedicated `<ul>`/`<li>`
//! writer ([`write_contents_list`]) rather than through `crate::nav`'s
//! `ResolvedNavEntry`/`write_nav_list`: those are shaped for the cross-document
//! toctree case and carry no notion of a per-entry backlink anchor, which is
//! exactly what this directive needs to hand each entry its own `toc-entry-N`
//! id and record, into `ctx.contents_backlinks`, where the heading it points
//! at should link back to.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use rusty_sphinx_ast::{Contents, ContentsBacklinks, Node, SectionId};
use rusty_sphinx_index::OutlineSection;

use crate::RenderCtx;

/// Whether an entry at `depth` (root entries are depth 1) may still be shown,
/// under a `:depth:` of `limit`. `None` is unlimited, mirroring
/// `crate::nav::expand`'s identical `:maxdepth:` rule.
const fn within_depth(depth: usize, limit: Option<usize>) -> bool {
    match limit {
        Some(limit) => depth <= limit,
        None => true,
    }
}

/// The id of the top-level heading most closely preceding `nodes[before]`,
/// which is that node's innermost enclosing section — headings nest by
/// level, so the nearest preceding one at *any* level is always the
/// innermost. `None` when nothing precedes it, i.e. the directive appears
/// before the document's first heading.
fn enclosing_heading_id(
    nodes: &[Node],
    before: usize,
    section_ids: &BTreeMap<usize, SectionId>,
) -> Option<SectionId> {
    nodes[..before]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, node)| matches!(node, Node::Heading { .. }))
        .and_then(|(heading_index, _)| section_ids.get(&heading_index))
        .cloned()
}

/// Finds the `OutlineSection` with the given `id` anywhere in `sections`,
/// searching depth-first.
fn find_section<'a>(sections: &'a [OutlineSection], id: &SectionId) -> Option<&'a OutlineSection> {
    for section in sections {
        if &section.id == id {
            return Some(section);
        }
        if let Some(found) = find_section(&section.children, id) {
            return Some(found);
        }
    }
    None
}

/// This directive's own anchor id: the explicit `:name:` verbatim — matching
/// `.. toctree::`'s own `:name:`, which is likewise used as-is rather than
/// slugified (see `render_toctree_directive`) — or, absent one, a slug
/// derived from `title` through `ctx`'s document-wide allocator, so it can
/// never collide with a heading id or with another un-named `.. contents::`
/// in the same document.
fn resolve_anchor_id(contents: &Contents, title: &str, ctx: &mut RenderCtx<'_>) -> String {
    match &contents.options.name {
        Some(name) => name.as_str().to_string(),
        None => ctx
            .contents_id_allocator
            .allocate(title)
            .as_str()
            .to_string(),
    }
}

/// Per-render state threaded through [`write_contents_list`]'s recursion:
/// everything about *how* to write an entry, as opposed to *which* section it
/// is.
struct ContentsWalk<'a> {
    /// `:depth:` — `None` is unlimited.
    limit: Option<usize>,
    backlinks: ContentsBacklinks,
    /// This table of contents' own anchor id, the link target for
    /// `:backlinks: top`.
    block_anchor: &'a str,
    /// How many entries have been written so far, for `toc-entry-N` ids —
    /// unique within one render, which is all `:backlinks:` needs.
    next_entry: usize,
}

/// Writes `sections` as a `<ul>`, recursively.
fn write_contents_list(
    html: &mut String,
    sections: &[OutlineSection],
    depth: usize,
    walk: &mut ContentsWalk<'_>,
    ctx: &mut RenderCtx<'_>,
) {
    let _ = writeln!(html, "<ul>");
    for section in sections {
        write_contents_entry(html, section, depth, walk, ctx);
    }
    let _ = writeln!(html, "</ul>");
}

/// Writes one entry and, recursively, whatever nests under it.
fn write_contents_entry(
    html: &mut String,
    section: &OutlineSection,
    depth: usize,
    walk: &mut ContentsWalk<'_>,
    ctx: &mut RenderCtx<'_>,
) {
    let href = format!("#{}", section.id.as_str());
    let href_attr = html_escape::encode_double_quoted_attribute(&href);
    let escaped_title = html_escape::encode_text(&section.title);

    match walk.backlinks {
        ContentsBacklinks::Off => {
            let _ = write!(html, "<li><a href=\"{href_attr}\">{escaped_title}</a>");
            // Two overlapping `.. contents::` blocks can both list the same
            // heading (e.g. two non-`:local:` ones on one page) — the later
            // one wins for every heading it lists, `:backlinks: none`
            // included, so it must actively clear a mapping an earlier block
            // left behind rather than merely not adding one.
            ctx.contents_backlinks.remove(&section.id);
        }
        ContentsBacklinks::Entry | ContentsBacklinks::Top => {
            // Each entry gets its own anchor id so the heading it points at
            // has something to link back to — docutils' `toc-entry-N`
            // convention (see `transforms/parts.py::Contents`).
            walk.next_entry += 1;
            let entry_id = format!("toc-entry-{}", walk.next_entry);
            let entry_id_attr = html_escape::encode_double_quoted_attribute(&entry_id);
            let _ = write!(
                html,
                "<li><a id=\"{entry_id_attr}\" href=\"{href_attr}\">{escaped_title}</a>"
            );
            let backlink_target = match walk.backlinks {
                ContentsBacklinks::Entry => format!("#{entry_id}"),
                ContentsBacklinks::Top => format!("#{}", walk.block_anchor),
                ContentsBacklinks::Off => unreachable!("matched above"),
            };
            ctx.contents_backlinks
                .insert(section.id.clone(), backlink_target);
        }
    }

    if within_depth(depth + 1, walk.limit) && !section.children.is_empty() {
        html.push('\n');
        write_contents_list(html, &section.children, depth + 1, walk, ctx);
        let _ = writeln!(html, "</li>");
    } else {
        let _ = writeln!(html, "</li>");
    }
}

/// Where a directive sits within its enclosing node list — what
/// `.. contents::` needs to find its own enclosing section for `:local:`.
/// Bundled the same way `DataTableParams`/`TableDirectiveParams` bundle their
/// directives' many fields, so passing it through `render_directive` costs
/// one parameter rather than three.
#[derive(Clone, Copy)]
pub(super) struct ContentsPlacement<'a> {
    pub nodes: &'a [Node],
    pub index: usize,
    /// The answer `render_nodes` already computed before clearing
    /// `ctx.at_top_level` for the duration of its node loop — see that
    /// function's doc comment for why `ctx.at_top_level` itself cannot be
    /// read here instead.
    pub at_top_level: bool,
}

/// Renders a `.. contents::` at `placement.index` within `placement.nodes`.
///
/// Renders nothing when the document has no sections at all, or — for
/// `:local:` — when the enclosing section (if any) has no subsections: an
/// empty table of contents is not useful and docutils does not emit one
/// either.
pub(crate) fn render_contents_directive(
    html: &mut String,
    contents: &Contents,
    placement: ContentsPlacement<'_>,
    ctx: &mut RenderCtx<'_>,
) {
    let Some(outline) = ctx.index.document_outlines.get(ctx.original_doc_path) else {
        return;
    };

    // `:local:` lists only the subsections of the section this directive is
    // written in. Only a top-level directive has a meaningful "enclosing
    // section" at all — one nested in a list item or admonition body falls
    // back to the whole document, the same as a directive placed before the
    // document's first heading.
    let local_root = if contents.options.local && placement.at_top_level {
        enclosing_heading_id(placement.nodes, placement.index, ctx.section_ids)
            .and_then(|id| find_section(&outline.sections, &id))
    } else {
        None
    };

    let (root_sections, show_title) = match local_root {
        // `:local:` suppresses the *default* title, but an explicit one is
        // still shown — see the docutils spec for `:local:`.
        Some(section) => (section.children.as_slice(), contents.title.is_some()),
        None => (outline.sections.as_slice(), true),
    };

    if root_sections.is_empty() {
        return;
    }

    let title = contents
        .title
        .clone()
        .unwrap_or_else(|| "Contents".to_string());
    let anchor_id = resolve_anchor_id(contents, &title, ctx);

    let mut classes = String::from("contents topic");
    for class in &contents.options.classes {
        classes.push(' ');
        classes.push_str(class);
    }
    let classes_attr = html_escape::encode_double_quoted_attribute(&classes);
    let id_attr = html_escape::encode_double_quoted_attribute(&anchor_id);
    let _ = writeln!(html, "<div class=\"{classes_attr}\" id=\"{id_attr}\">");
    if show_title {
        let escaped_title = html_escape::encode_text(&title);
        let _ = writeln!(html, "<p class=\"topic-title\">{escaped_title}</p>");
    }

    let mut walk = ContentsWalk {
        limit: contents.options.depth.map(std::num::NonZeroUsize::get),
        backlinks: contents.options.backlinks,
        block_anchor: &anchor_id,
        next_entry: 0,
    };
    write_contents_list(html, root_sections, 1, &mut walk, ctx);

    let _ = writeln!(html, "</div>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{ContentsOptions, Directive, Document, InlineNode, TargetName};
    use rusty_sphinx_index::{DocumentOutline, ProjectIndex};

    fn heading(level: u8, title: &str) -> Node {
        Node::Heading {
            level,
            text: vec![InlineNode::Text(title.to_string())],
        }
    }

    fn section(title: &str, children: Vec<OutlineSection>) -> OutlineSection {
        OutlineSection {
            title: title.to_string(),
            id: SectionId::from_title(title),
            children,
        }
    }

    /// A `.. contents::` with `:backlinks: none`, for tests about entries and
    /// nesting rather than about backlinks — without it, every entry's `<a>`
    /// carries a `toc-entry-N` id that would otherwise have to be repeated in
    /// every unrelated assertion.
    fn contents_without_backlinks(options: ContentsOptions) -> Node {
        Node::Directive(Directive::Contents(Contents {
            title: None,
            options: ContentsOptions {
                backlinks: ContentsBacklinks::Off,
                ..options
            },
        }))
    }

    /// Renders `nodes` as a whole document (not just the one contents
    /// directive), so `enclosing_heading_id`'s backward scan and
    /// `heading`-rendered backlinks are exercised exactly as `render_nodes`
    /// runs them for real.
    fn render_document(nodes: Vec<Node>, outline: DocumentOutline) -> String {
        let mut index = ProjectIndex::default();
        index
            .document_outlines
            .insert("test.rst".to_string(), outline);
        let doc = Document::new("test.rst".to_string(), nodes);
        crate::render(&doc, &index, "test.rst").html
    }

    #[test]
    fn test_render_contents_lists_every_top_level_section() {
        // Given
        let nodes = vec![
            contents_without_backlinks(ContentsOptions::default()),
            heading(1, "Overview"),
            heading(1, "Reference"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Overview", vec![]), section("Reference", vec![])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(
            html.contains("<a href=\"#overview\">Overview</a>"),
            "{html}"
        );
        assert!(
            html.contains("<a href=\"#reference\">Reference</a>"),
            "{html}"
        );
    }

    #[test]
    fn test_render_contents_defaults_the_title_to_contents() {
        // Given
        let nodes = vec![
            Node::Directive(Directive::Contents(Contents::default())),
            heading(1, "Overview"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Overview", vec![])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(
            html.contains("<p class=\"topic-title\">Contents</p>"),
            "{html}"
        );
    }

    #[test]
    fn test_render_contents_uses_an_explicit_title() {
        // Given
        let nodes = vec![
            Node::Directive(Directive::Contents(Contents {
                title: Some("Table of Contents".to_string()),
                options: ContentsOptions::default(),
            })),
            heading(1, "Overview"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Overview", vec![])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(
            html.contains("<p class=\"topic-title\">Table of Contents</p>"),
            "{html}"
        );
    }

    #[test]
    fn test_render_contents_renders_nothing_for_an_empty_document() {
        // Given
        let nodes = vec![Node::Directive(Directive::Contents(Contents::default()))];
        let outline = DocumentOutline::default();

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(!html.contains("contents topic"), "{html}");
    }

    #[test]
    fn test_render_contents_nests_children_under_their_parent() {
        // Given
        let nodes = vec![
            contents_without_backlinks(ContentsOptions::default()),
            heading(1, "Guide"),
            heading(2, "Setup"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Guide", vec![section("Setup", vec![])])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(html.contains("<a href=\"#guide\">Guide</a>"), "{html}");
        assert!(html.contains("<a href=\"#setup\">Setup</a>"), "{html}");
        assert_eq!(html.matches("<ul>").count(), 2, "{html}");
    }

    #[test]
    fn test_render_contents_respects_depth() {
        // Given
        let nodes = vec![
            contents_without_backlinks(ContentsOptions {
                depth: std::num::NonZeroUsize::new(1),
                ..ContentsOptions::default()
            }),
            heading(1, "Guide"),
            heading(2, "Setup"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Guide", vec![section("Setup", vec![])])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(html.contains("<a href=\"#guide\">Guide</a>"), "{html}");
        assert!(!html.contains("#setup"), "{html}");
    }

    #[test]
    fn test_render_contents_local_lists_only_the_enclosing_sections_subsections() {
        // Given — the directive sits under "Advanced", so only "Details"
        // should be listed, not the sibling "Basics".
        let nodes = vec![
            heading(1, "Basics"),
            heading(1, "Advanced"),
            contents_without_backlinks(ContentsOptions {
                local: true,
                ..ContentsOptions::default()
            }),
            heading(2, "Details"),
        ];
        let outline = DocumentOutline {
            sections: vec![
                section("Basics", vec![]),
                section("Advanced", vec![section("Details", vec![])]),
            ],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(html.contains("<a href=\"#details\">Details</a>"), "{html}");
        assert!(!html.contains("#basics"), "{html}");
        // The enclosing section's own title is not listed, only its children —
        // checked as a link, since the heading itself legitimately says
        // "Advanced" too.
        assert!(!html.contains("href=\"#advanced\""), "{html}");
    }

    #[test]
    fn test_render_contents_local_omits_the_default_title() {
        // Given
        let nodes = vec![
            heading(1, "Advanced"),
            contents_without_backlinks(ContentsOptions {
                local: true,
                ..ContentsOptions::default()
            }),
            heading(2, "Details"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Advanced", vec![section("Details", vec![])])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(!html.contains("topic-title"), "{html}");
    }

    #[test]
    fn test_render_contents_local_keeps_an_explicit_title() {
        // Given
        let nodes = vec![
            heading(1, "Advanced"),
            Node::Directive(Directive::Contents(Contents {
                title: Some("In this section".to_string()),
                options: ContentsOptions {
                    local: true,
                    ..ContentsOptions::default()
                },
            })),
            heading(2, "Details"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Advanced", vec![section("Details", vec![])])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(
            html.contains("<p class=\"topic-title\">In this section</p>"),
            "{html}"
        );
    }

    #[test]
    fn test_render_contents_local_falls_back_to_the_whole_document_before_any_heading() {
        // Given — `:local:` with nothing preceding it.
        let nodes = vec![
            contents_without_backlinks(ContentsOptions {
                local: true,
                ..ContentsOptions::default()
            }),
            heading(1, "Overview"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Overview", vec![])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(
            html.contains("<a href=\"#overview\">Overview</a>"),
            "{html}"
        );
    }

    #[test]
    fn test_render_contents_renders_no_entry_ids_when_backlinks_is_off() {
        // Given
        let nodes = vec![
            Node::Directive(Directive::Contents(Contents {
                title: None,
                options: ContentsOptions {
                    backlinks: ContentsBacklinks::Off,
                    ..ContentsOptions::default()
                },
            })),
            heading(1, "Overview"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Overview", vec![])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(!html.contains("toc-entry"), "{html}");
        assert!(!html.contains("toc-backref"), "{html}");
    }

    #[test]
    fn test_render_contents_backlinks_none_clears_a_backlink_an_earlier_block_set() {
        // Given — two overlapping, non-`:local:` tables of contents on one
        // page (both list the whole document): the first sets a backlink for
        // "Overview", the second explicitly turns backlinks off. The second,
        // later block must win — a heading it also lists should end up with
        // no backlink at all, not the first block's stale one.
        let nodes = vec![
            Node::Directive(Directive::Contents(Contents::default())),
            Node::Directive(Directive::Contents(Contents {
                title: None,
                options: ContentsOptions {
                    backlinks: ContentsBacklinks::Off,
                    ..ContentsOptions::default()
                },
            })),
            heading(1, "Overview"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Overview", vec![])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(!html.contains("toc-backref"), "{html}");
    }

    #[test]
    fn test_render_contents_backlinks_entry_links_the_heading_back_to_its_own_entry() {
        // Given
        let nodes = vec![
            Node::Directive(Directive::Contents(Contents::default())),
            heading(1, "Overview"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Overview", vec![])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then — the entry carries an id...
        assert!(
            html.contains("<a id=\"toc-entry-1\" href=\"#overview\">"),
            "{html}"
        );
        // ...and the heading links back to exactly that id.
        assert!(
            html.contains("<a class=\"toc-backref\" href=\"#toc-entry-1\">Overview</a>"),
            "{html}"
        );
    }

    #[test]
    fn test_render_contents_backlinks_top_links_every_heading_back_to_the_block() {
        // Given
        let nodes = vec![
            Node::Directive(Directive::Contents(Contents {
                title: None,
                options: ContentsOptions {
                    backlinks: ContentsBacklinks::Top,
                    ..ContentsOptions::default()
                },
            })),
            heading(1, "Overview"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Overview", vec![])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(
            html.contains("<a class=\"toc-backref\" href=\"#contents\">Overview</a>"),
            "{html}"
        );
    }

    #[test]
    fn test_render_contents_uses_the_explicit_name_as_its_own_anchor() {
        // Given
        let nodes = vec![
            Node::Directive(Directive::Contents(Contents {
                title: None,
                options: ContentsOptions {
                    name: Some(TargetName::new("main-toc")),
                    ..ContentsOptions::default()
                },
            })),
            heading(1, "Overview"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Overview", vec![])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(html.contains("id=\"main-toc\""), "{html}");
    }

    #[test]
    fn test_render_contents_adds_extra_classes() {
        // Given
        let nodes = vec![
            Node::Directive(Directive::Contents(Contents {
                title: None,
                options: ContentsOptions {
                    classes: vec!["wide".to_string()],
                    ..ContentsOptions::default()
                },
            })),
            heading(1, "Overview"),
        ];
        let outline = DocumentOutline {
            sections: vec![section("Overview", vec![])],
        };

        // When
        let html = render_document(nodes, outline);

        // Then
        assert!(html.contains("class=\"contents topic wide\""), "{html}");
    }

    #[test]
    fn test_find_section_finds_a_nested_section() {
        // Given
        let sections = vec![section("Guide", vec![section("Setup", vec![])])];
        let target = SectionId::from_title("Setup");

        // When
        let found = find_section(&sections, &target);

        // Then
        assert_eq!(found.map(|s| s.title.as_str()), Some("Setup"));
    }

    #[test]
    fn test_find_section_returns_none_for_an_unknown_id() {
        // Given
        let sections = vec![section("Guide", vec![])];
        let target = SectionId::from_title("Missing");

        // When
        let found = find_section(&sections, &target);

        // Then
        assert!(found.is_none());
    }

    #[test]
    fn test_enclosing_heading_id_finds_the_nearest_preceding_heading() {
        // Given
        let nodes = vec![heading(1, "Guide"), heading(2, "Setup")];
        let mut section_ids = BTreeMap::new();
        section_ids.insert(0, SectionId::from_title("Guide"));
        section_ids.insert(1, SectionId::from_title("Setup"));

        // When
        let found = enclosing_heading_id(&nodes, 2, &section_ids);

        // Then
        assert_eq!(found, Some(SectionId::from_title("Setup")));
    }

    #[test]
    fn test_enclosing_heading_id_is_none_before_any_heading() {
        // Given
        let nodes = vec![heading(1, "Guide")];
        let section_ids = BTreeMap::new();

        // When
        let found = enclosing_heading_id(&nodes, 0, &section_ids);

        // Then
        assert_eq!(found, None);
    }

    #[test]
    fn test_within_depth_is_unlimited_when_no_limit_is_set() {
        // Given / When / Then
        assert!(within_depth(1, None));
        assert!(within_depth(100, None));
    }

    #[test]
    fn test_within_depth_stops_beyond_its_limit() {
        // Given / When / Then
        assert!(within_depth(2, Some(2)));
        assert!(!within_depth(3, Some(2)));
    }
}
