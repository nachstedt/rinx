//! `:numref:` rendering: a link to a labelled figure, table, code block or
//! section, showing its number in the site's (or the role's own) format.
//!
//! Resolution follows Sphinx's order, which decides which problem an author
//! is told about when several apply: a label naming nothing numbered at all
//! is a broken link; then `numfig` being off (for anything but a section);
//! then an element with no number; then a `{name}` with no caption to fill it.
//! Every refusal is shown as the text the role was written with, unlinked, as
//! Sphinx shows it.

use std::fmt::Write as _;

use rinx_ast::{EnumerableKind, InlineNode, NumberFormat, Span, TargetName};
use rinx_index::{NumrefSubject, ProjectIndex, join_number};

use super::reference::label_href;
use crate::RenderCtx;
use crate::numbering::Numbering;
use crate::{BrokenLink, BrokenLinkKind, ReferenceTarget};

/// A `:numref:` as the author wrote it.
#[derive(Debug, Clone, Copy)]
pub(super) struct NumRef<'a> {
    /// The explicit title's format, if one was written.
    pub title: Option<&'a NumberFormat>,
    /// The label as written — or, for the `!` form, the text shown.
    pub target: &'a str,
    /// `false` for the `!` form, which is never looked up.
    pub link: bool,
    pub span: Option<Span>,
}

impl NumRef<'_> {
    /// What Sphinx shows for a reference it could not resolve: the explicit
    /// title as written, else the label.
    fn written_text(&self) -> &str {
        self.title.map_or(self.target, NumberFormat::as_str)
    }
}

/// Renders `inline`, which must be an [`InlineNode::NumberReference`], with
/// what `ctx` holds — the dispatcher's entry point.
pub(super) fn render_number_reference_node(
    html: &mut String,
    inline: &InlineNode,
    ctx: &mut RenderCtx<'_>,
) {
    let InlineNode::NumberReference {
        title,
        target,
        link,
        span,
    } = inline
    else {
        return;
    };
    render_inline_number_reference(
        html,
        NumRef {
            title: title.as_ref(),
            target,
            link: *link,
            span: *span,
        },
        ctx.index,
        ctx.numbering,
        ctx.doc_path,
        ctx.broken_links,
    );
}

/// Renders a `:numref:`.
pub(super) fn render_inline_number_reference(
    html: &mut String,
    reference: NumRef<'_>,
    index: &ProjectIndex,
    numbering: &Numbering<'_>,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
    if !reference.link {
        write_unlinked(html, reference.target);
        return;
    }
    match resolve(reference, index, numbering) {
        Ok(resolved) => {
            let href = label_href(index, &resolved.label, resolved.doc_path, doc_path);
            let _ = write!(
                html,
                "<a class=\"reference internal\" href=\"{}\">\
                 <span class=\"std std-numref\">{}</span></a>",
                html_escape::encode_double_quoted_attribute(&href),
                html_escape::encode_text(&resolved.text)
            );
        }
        Err(kind) => {
            let text = reference.written_text();
            if matches!(
                kind,
                BrokenLinkKind::NumberReference | BrokenLinkKind::AmbiguousTarget { .. }
            ) {
                let _ = write!(
                    html,
                    "<a href=\"#\" class=\"broken-link\">{}</a>",
                    html_escape::encode_text(text)
                );
            } else {
                write_unlinked(html, text);
            }
            broken_links.push(BrokenLink {
                kind,
                target: reference.target.to_string(),
                span: reference.span,
            });
        }
    }
}

/// Where `reference` leads — `None` for the `!` form and whenever
/// [`render_inline_number_reference`] would show it unlinked or broken.
pub(super) fn number_reference_target(
    reference: NumRef<'_>,
    index: &ProjectIndex,
    numbering: &Numbering<'_>,
) -> Option<ReferenceTarget> {
    if !reference.link {
        return None;
    }
    let resolved = resolve(reference, index, numbering).ok()?;
    Some(ReferenceTarget::in_document(
        resolved.text,
        resolved.doc_path,
        Some(index.target_anchor(&resolved.label).to_string()),
    ))
}

/// A `:numref:` that resolved, ready to be drawn.
struct Resolved<'a> {
    label: TargetName,
    doc_path: &'a str,
    text: String,
}

/// Looks `reference` up, returning the problem to report when it cannot be
/// shown with a number.
fn resolve<'a>(
    reference: NumRef<'_>,
    index: &'a ProjectIndex,
    numbering: &Numbering<'_>,
) -> Result<Resolved<'a>, BrokenLinkKind> {
    let label = TargetName::new(reference.target);
    let target = index.numref_targets.get(&label).ok_or_else(|| {
        BrokenLinkKind::NumberReference
            .unless_contested(index.ambiguous_definitions.targets.get(&label))
    })?;
    let kind = target.subject.kind();
    if kind != EnumerableKind::Section && !numbering.is_enabled() {
        return Err(BrokenLinkKind::NumberingDisabled);
    }
    let number = match &target.subject {
        NumrefSubject::Element { ordinal, .. } => index
            .element_numbers
            .get(&target.doc_path)
            .and_then(|numbers| numbers.get(*ordinal)),
        NumrefSubject::Section(id) => index
            .section_numbers
            .get(&target.doc_path)
            .and_then(|numbers| numbers.number_at(Some(id))),
    }
    .ok_or(BrokenLinkKind::UnnumberedReference)?;
    let format = reference
        .title
        .unwrap_or_else(|| numbering.formats().for_kind(kind));
    let name = index.target_titles.get(&label).map(String::as_str);
    if format.requires_name() && name.is_none() {
        return Err(BrokenLinkKind::UncaptionedReference);
    }
    Ok(Resolved {
        text: format.apply(&join_number(number), name),
        doc_path: &target.doc_path,
        label,
    })
}

/// Writes text a `:numref:` shows without a link.
fn write_unlinked(html: &mut String, text: &str) {
    let _ = write!(
        html,
        "<span class=\"xref std std-numref\">{}</span>",
        html_escape::encode_text(text)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{
        AssetUri, Directive, Document, Figure, ImageOptions, InlineNode, Node, SectionId,
    };
    use rinx_index::{DocumentNumbers, ElementNumbers, NumrefTarget};

    use crate::config::SiteConfig;

    /// A site whose `guide.rst` holds figure `fig-a` (numbered 2.1), an
    /// unnumbered figure `fig-orphan`, and section `usage` (numbered 2.3).
    fn index() -> ProjectIndex {
        let mut index = ProjectIndex::default();
        let element = |ordinal| NumrefTarget {
            doc_path: "guide.rst".to_string(),
            subject: NumrefSubject::Element {
                ordinal,
                kind: EnumerableKind::Figure,
            },
        };
        index
            .numref_targets
            .insert(TargetName::new("fig-a"), element(0));
        index
            .numref_targets
            .insert(TargetName::new("fig-orphan"), element(1));
        index.numref_targets.insert(
            TargetName::new("usage"),
            NumrefTarget {
                doc_path: "guide.rst".to_string(),
                subject: NumrefSubject::Section(SectionId::from_title("Usage")),
            },
        );
        for label in ["fig-a", "fig-orphan", "usage"] {
            index
                .targets
                .insert(TargetName::new(label), "guide.rst".to_string());
        }
        index
            .target_titles
            .insert(TargetName::new("fig-a"), "A caption".to_string());
        index
            .target_titles
            .insert(TargetName::new("usage"), "Usage".to_string());
        let mut numbers = ElementNumbers::default();
        numbers.set(0, vec![2, 1]);
        index
            .element_numbers
            .insert("guide.rst".to_string(), numbers);
        let mut sections = DocumentNumbers::default();
        sections.set_section(&SectionId::from_title("Usage"), vec![2, 3]);
        index
            .section_numbers
            .insert("guide.rst".to_string(), sections);
        index
    }

    fn config(numfig: bool) -> SiteConfig {
        SiteConfig {
            numfig,
            ..SiteConfig::default()
        }
    }

    fn render_with(
        reference: NumRef<'_>,
        config: &SiteConfig,
        index: &ProjectIndex,
    ) -> (String, Vec<BrokenLink>) {
        let doc = Document::new("index.rst".to_string(), Vec::new());
        let numbering = Numbering::new(&doc, index, config);
        let mut html = String::new();
        let mut broken = Vec::new();
        render_inline_number_reference(
            &mut html,
            reference,
            index,
            &numbering,
            "index.rst",
            &mut broken,
        );
        (html, broken)
    }

    fn render(reference: NumRef<'_>, numfig: bool) -> (String, Vec<BrokenLink>) {
        render_with(reference, &config(numfig), &index())
    }

    fn numref<'a>(title: Option<&'a NumberFormat>, target: &'a str) -> NumRef<'a> {
        NumRef {
            title,
            target,
            link: true,
            span: None,
        }
    }

    fn kinds(broken: &[BrokenLink]) -> Vec<BrokenLinkKind> {
        broken.iter().map(|link| link.kind.clone()).collect()
    }

    #[test]
    fn test_links_a_figure_showing_its_number_in_the_site_format() {
        // Given / When
        let (html, broken) = render(numref(None, "FIG-A"), true);

        // Then — the label is case-insensitive, as a `:ref:`'s is
        assert_eq!(
            html,
            "<a class=\"reference internal\" href=\"guide.html#fig-a\">\
             <span class=\"std std-numref\">Fig. 2.1</span></a>"
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn test_an_explicit_title_is_the_format() {
        // Given
        let title = NumberFormat::parse("Figure {number} ({name})").expect("valid");

        // When
        let (html, _) = render(numref(Some(&title), "fig-a"), true);

        // Then
        assert!(html.contains(">Figure 2.1 (A caption)<"), "{html}");
    }

    #[test]
    fn test_a_section_is_numbered_even_with_numfig_off() {
        // Given / When
        let (html, broken) = render(numref(None, "usage"), false);

        // Then
        assert!(html.contains(">Section 2.3<"), "{html}");
        assert!(broken.is_empty());
    }

    #[test]
    fn test_a_figure_with_numfig_off_is_reported_and_shown_unlinked() {
        // Given / When
        let (html, broken) = render(numref(None, "fig-a"), false);

        // Then
        assert_eq!(html, "<span class=\"xref std std-numref\">fig-a</span>");
        assert_eq!(kinds(&broken), vec![BrokenLinkKind::NumberingDisabled]);
    }

    #[test]
    fn test_an_unknown_label_is_a_broken_link() {
        // Given
        let title = NumberFormat::parse("Fig. %s").expect("valid");

        // When
        let (html, broken) = render(numref(Some(&title), "missing"), true);

        // Then — shown as the title written, as Sphinx shows it
        assert_eq!(html, "<a href=\"#\" class=\"broken-link\">Fig. %s</a>");
        assert_eq!(kinds(&broken), vec![BrokenLinkKind::NumberReference]);
        assert_eq!(broken[0].target, "missing");
    }

    #[test]
    fn test_an_element_without_a_number_is_reported() {
        // Given / When
        let (html, broken) = render(numref(None, "fig-orphan"), true);

        // Then
        assert_eq!(
            html,
            "<span class=\"xref std std-numref\">fig-orphan</span>"
        );
        assert_eq!(kinds(&broken), vec![BrokenLinkKind::UnnumberedReference]);
    }

    #[test]
    fn test_a_name_field_without_a_caption_is_reported() {
        // Given the numbered figure, with its caption taken away
        let mut index = index();
        index.target_titles.remove(&TargetName::new("fig-a"));
        let title = NumberFormat::parse("{name}").expect("valid");

        // When
        let (html, broken) = render_with(numref(Some(&title), "fig-a"), &config(true), &index);

        // Then
        assert_eq!(html, "<span class=\"xref std std-numref\">{name}</span>");
        assert_eq!(kinds(&broken), vec![BrokenLinkKind::UncaptionedReference]);
    }

    #[test]
    fn test_the_bang_form_is_never_looked_up() {
        // Given
        let reference = NumRef {
            title: None,
            target: "Tit <fig-a>",
            link: false,
            span: None,
        };

        // When
        let (html, broken) = render(reference, true);

        // Then
        assert_eq!(
            html,
            "<span class=\"xref std std-numref\">Tit &lt;fig-a&gt;</span>"
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn test_a_caption_number_and_a_numref_agree() {
        // Given a page holding the figure the index numbered
        let mut figure = Figure::new(ImageOptions::new(AssetUri::new("a.png")));
        figure.caption = Some(vec![InlineNode::Text("A caption".to_string())]);
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Directive(Directive::Figure(Box::new(figure)))],
        );
        let index = index();
        let config = config(true);

        // When
        let numbering = Numbering::new(&doc, &index, &config);
        let (html, _) = render(numref(None, "fig-a"), true);

        // Then
        let Node::Directive(directive) = &doc.nodes[0] else {
            unreachable!()
        };
        assert_eq!(numbering.caption_number(directive), Some("Fig. 2.1"));
        assert!(html.contains(">Fig. 2.1<"), "{html}");
    }

    #[test]
    fn test_render_number_reference_node_renders_through_the_page_dispatcher() {
        // Given a page on which a paragraph holds the `:numref:`
        let doc = Document::new(
            "index.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::NumberReference {
                title: None,
                target: "usage".to_string(),
                link: true,
                span: None,
            }])],
        );

        // When
        let output = crate::render_with_config(&doc, &index(), "index.rst", &config(false));

        // Then
        assert!(
            output.html.contains(">Section 2.3</span></a>"),
            "{}",
            output.html
        );
        assert!(output.broken_links.is_empty());
    }
}
