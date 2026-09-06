//! `.. image::` rendering — and the `<img>` element a `.. figure::` reuses.
//!
//! Three of the nine options do not become attributes on the `<img>` at all:
//! `:target:` wraps it in a link, `:align:` becomes a class on whichever
//! element is the block (the `<img>` here, the `<figure>` there), and
//! `:loading:` decides what `src` even holds. The rest are attributes, and
//! `:scale:` has already been folded into `:width:`/`:height:` by
//! [`rusty_sphinx_ast::ImageOptions::rendered_width`].

use std::fmt::Write as _;

use rusty_sphinx_ast::{ImageAlign, ImageLoading, ImageOptions, ImageTarget, ImageUri};
use rusty_sphinx_index::TargetLocation;

use crate::RenderCtx;
use crate::doc_href::relative_doc_href;
use crate::image_error::ImageError;
use crate::{BrokenLink, BrokenLinkKind};

use super::asset_href::relative_asset_href;

/// The `src` an image renders with, and whether it is a `data:` URI.
///
/// An external URL is passed through untouched; a project file becomes a path
/// into the site's `_images/` directory, unless the author asked for the bytes
/// themselves and the build supplied them.
fn image_src(options: &ImageOptions, ctx: &mut RenderCtx) -> String {
    let ImageUri::Document(written) = &options.uri else {
        // An external URL. `:loading: embed` on one is already reported by the
        // parser — fetching it would make the build depend on the network — so
        // there is nothing left to say here and it simply links.
        return options.uri.as_written().to_string();
    };

    let Some(resolved) = options.uri.resolve(ctx.original_doc_path) else {
        // Unreachable for a `Document` URI, but falling back to the written
        // text keeps a `src` pointing somewhere plausible either way.
        return written.clone();
    };

    if options.loading == ImageLoading::Embed {
        if let Some(data_uri) = ctx.embedded_assets.get(&resolved) {
            return data_uri.to_string();
        }
        ctx.image_errors.push(ImageError {
            uri: written.clone(),
            message: format!(
                "the bytes for '{written}' were not available to embed, so it is linked instead \
                 — in a Bazel build, add the file to the library's images attribute"
            ),
            span: options.span,
        });
    }

    relative_asset_href(&resolved, ctx.doc_path)
}

/// The `style` attribute value for an image's own dimensions, if it has any.
///
/// A `style` rather than the `width`/`height` attributes docutils' legacy
/// HTML4 writer used, because those accept only pixels while `:width:` may be
/// a percentage or any CSS unit.
fn dimension_style(options: &ImageOptions) -> Option<String> {
    let mut declarations = Vec::new();
    if let Some(width) = options.rendered_width() {
        declarations.push(format!("width: {width}"));
    }
    if let Some(height) = options.rendered_height() {
        declarations.push(format!("height: {height}"));
    }
    if declarations.is_empty() {
        None
    } else {
        Some(declarations.join("; "))
    }
}

/// Writes the `<img>` element itself, with `extra_classes` merged into its
/// `:class:` list.
///
/// `extra_classes` is how a standalone `.. image::` gets its `align-*` class;
/// a figure passes none, because there the alignment belongs on the `<figure>`.
///
/// `pub(crate)` rather than `pub(super)`: `crate::inline::image` reuses it
/// verbatim for an `InlineImage` node, since a substitution-defined image's
/// `<img>` element is built exactly the same way as a standalone one's.
pub(crate) fn render_image_element(
    html: &mut String,
    options: &ImageOptions,
    extra_classes: &[String],
    ctx: &mut RenderCtx,
) {
    let src = image_src(options, ctx);
    let src_attr = html_escape::encode_double_quoted_attribute(&src);

    // docutils falls back to the URI as the alt text rather than omitting the
    // attribute, and Sphinx's rendered output does the same. An author who
    // wants a genuinely empty alt — the accessible spelling for a decorative
    // image — writes `:alt:` with no value, which is kept as `Some("")`.
    let alt = options.alt.clone().unwrap_or_else(|| match &options.uri {
        ImageUri::Document(written) => written.clone(),
        ImageUri::External(uri) => uri.clone(),
    });
    let alt_attr = html_escape::encode_double_quoted_attribute(&alt);

    let _ = write!(html, "<img src=\"{src_attr}\" alt=\"{alt_attr}\"");

    let classes: Vec<&str> = extra_classes
        .iter()
        .chain(&options.classes)
        .map(String::as_str)
        .collect();
    if !classes.is_empty() {
        let joined = classes.join(" ");
        let class_attr = html_escape::encode_double_quoted_attribute(&joined);
        let _ = write!(html, " class=\"{class_attr}\"");
    }

    if let Some(style) = dimension_style(options) {
        let style_attr = html_escape::encode_double_quoted_attribute(&style);
        let _ = write!(html, " style=\"{style_attr}\"");
    }

    // Only `lazy` has a counterpart in HTML's own vocabulary: `embed` and
    // `link` are already expressed by what `src` holds.
    if options.loading == ImageLoading::Lazy {
        let _ = write!(html, " loading=\"lazy\"");
    }

    let _ = write!(html, " />");
}

/// Resolves an image's `:target:` into an href, reporting a named target that
/// resolves to nothing exactly as a broken `:ref:` is reported.
fn target_href(target: &ImageTarget, options: &ImageOptions, ctx: &mut RenderCtx) -> String {
    match target {
        ImageTarget::Uri(uri) => uri.clone(),
        ImageTarget::Reference(name) => {
            if let Some(TargetLocation::Internal(target_path)) = ctx.index.targets.get(name) {
                format!(
                    "{}#{}",
                    relative_doc_href(target_path, ctx.doc_path),
                    name.as_str()
                )
            } else {
                ctx.broken_links.push(BrokenLink {
                    kind: BrokenLinkKind::Hyperlink,
                    target: name.as_str().to_string(),
                    span: options.span,
                });
                format!("#{}", name.as_str())
            }
        }
    }
}

/// Wraps `body` in the link an image's `:target:` asks for, if it has one.
///
/// Shared with the figure renderer, where docutils links the image inside the
/// `<figure>` rather than the figure itself, and with `crate::inline::image`
/// for the same reason [`render_image_element`] is `pub(crate)`.
pub(crate) fn render_linked_image(
    html: &mut String,
    options: &ImageOptions,
    extra_classes: &[String],
    ctx: &mut RenderCtx,
) {
    let Some(target) = options.target.clone() else {
        render_image_element(html, options, extra_classes, ctx);
        return;
    };

    let href = target_href(&target, options, ctx);
    let href_attr = html_escape::encode_double_quoted_attribute(&href);
    let broken = matches!(target, ImageTarget::Reference(ref name)
        if !matches!(ctx.index.targets.get(name), Some(TargetLocation::Internal(_))));
    if broken {
        let _ = write!(html, "<a href=\"{href_attr}\" class=\"broken-link\">");
    } else {
        let _ = write!(html, "<a href=\"{href_attr}\">");
    }
    render_image_element(html, options, extra_classes, ctx);
    let _ = write!(html, "</a>");
}

/// Renders a `.. image::` directive.
///
/// The `id` from `:name:` goes on a wrapper rather than on the `<img>` itself,
/// so a `:ref:` to the image lands above it rather than scrolling the picture
/// under the page header — and so an image that is also a link still has one
/// unambiguous anchor.
pub(super) fn render_image_directive(
    html: &mut String,
    options: &ImageOptions,
    ctx: &mut RenderCtx,
) {
    let align_class: Vec<String> = options
        .align
        .map(ImageAlign::css_class)
        .into_iter()
        .collect();

    if let Some(name) = &options.name {
        let id_attr = html_escape::encode_double_quoted_attribute(name.as_str());
        let _ = write!(html, "<span id=\"{id_attr}\"></span>");
    }
    render_linked_image(html, options, &align_class, ctx);
    let _ = writeln!(html);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::render_test_support::render_directive_html;
    use rusty_sphinx_ast::{Directive, ImageUri, TargetName};
    use rusty_sphinx_index::ProjectIndex;

    /// Renders a `.. image::` built from `options` on a page at `doc_path`.
    fn render_at(options: ImageOptions, doc_path: &str) -> String {
        render_directive_html(
            &Directive::Image(Box::new(options)),
            &ProjectIndex::default(),
            doc_path,
        )
    }

    fn render(options: ImageOptions) -> String {
        render_at(options, "index.rst")
    }

    fn image(uri: &str) -> ImageOptions {
        ImageOptions::new(ImageUri::new(uri))
    }

    #[test]
    fn test_renders_a_bare_image() {
        // Given
        let options = image("logo.png");

        // When
        let html = render(options);

        // Then — alt falls back to the URI, as docutils and Sphinx both do
        assert!(
            html.contains("<img src=\"_images/logo.png\" alt=\"logo.png\" />"),
            "unexpected html: {html}"
        );
    }

    #[test]
    fn test_renders_the_alt_text() {
        // Given
        let mut options = image("logo.png");
        options.alt = Some("A red circle".to_string());

        // When
        let html = render(options);

        // Then
        assert!(html.contains("alt=\"A red circle\""), "unexpected: {html}");
    }

    #[test]
    fn test_renders_a_deliberately_empty_alt() {
        // Given — the accessible spelling for a decorative image
        let mut options = image("logo.png");
        options.alt = Some(String::new());

        // When
        let html = render(options);

        // Then
        assert!(html.contains("alt=\"\""), "unexpected: {html}");
    }

    #[test]
    fn test_renders_dimensions_as_a_style() {
        // Given
        let mut options = image("logo.png");
        options.width = Some(rusty_sphinx_ast::LengthOrPercentage::new("50%").expect("valid"));
        options.height = Some(rusty_sphinx_ast::Length::new("3cm").expect("valid"));

        // When
        let html = render(options);

        // Then
        assert!(
            html.contains("style=\"width: 50%; height: 3cm\""),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_applies_the_scale_to_the_rendered_dimensions() {
        // Given
        let mut options = image("logo.png");
        options.width = Some(rusty_sphinx_ast::LengthOrPercentage::new("200px").expect("valid"));
        options.scale = Some(50);

        // When
        let html = render(options);

        // Then
        assert!(
            html.contains("style=\"width: 100px\""),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_renders_no_style_without_dimensions() {
        // Given
        let options = image("logo.png");

        // When
        let html = render(options);

        // Then
        assert!(!html.contains("style="), "unexpected: {html}");
    }

    #[test]
    fn test_renders_the_align_class_on_the_image() {
        // Given
        let mut options = image("logo.png");
        options.align = Some(ImageAlign::Center);

        // When
        let html = render(options);

        // Then
        assert!(
            html.contains("class=\"align-center\""),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_merges_the_align_class_with_the_authors_classes() {
        // Given
        let mut options = image("logo.png");
        options.align = Some(ImageAlign::Left);
        options.classes = vec!["fancy".to_string()];

        // When
        let html = render(options);

        // Then
        assert!(
            html.contains("class=\"align-left fancy\""),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_renders_lazy_loading_as_the_html_attribute() {
        // Given
        let mut options = image("logo.png");
        options.loading = ImageLoading::Lazy;

        // When
        let html = render(options);

        // Then
        assert!(html.contains("loading=\"lazy\""), "unexpected: {html}");
    }

    #[test]
    fn test_renders_no_loading_attribute_for_a_plain_link() {
        // Given
        let options = image("logo.png");

        // When
        let html = render(options);

        // Then
        assert!(!html.contains("loading="), "unexpected: {html}");
    }

    #[test]
    fn test_renders_an_external_url_untouched() {
        // Given
        let options = image("https://example.com/logo.png");

        // When
        let html = render(options);

        // Then
        assert!(
            html.contains("src=\"https://example.com/logo.png\""),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_resolves_the_asset_path_relative_to_the_page() {
        // Given
        let options = image("logo.png");

        // When — the page sits one directory down, so the href must climb out
        let html = render_at(options, "guide/intro.rst");

        // Then
        assert!(
            html.contains("src=\"../_images/guide/logo.png\""),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_renders_a_uri_target_as_a_link() {
        // Given
        let mut options = image("logo.png");
        options.target = Some(ImageTarget::new("https://example.com/"));

        // When
        let html = render(options);

        // Then
        assert!(
            html.contains("<a href=\"https://example.com/\"><img"),
            "unexpected: {html}"
        );
        assert!(html.contains("</a>"), "unexpected: {html}");
    }

    #[test]
    fn test_renders_a_resolved_reference_target_as_a_link() {
        // Given
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("some-section"),
            TargetLocation::Internal("other.rst".to_string()),
        );
        let mut options = image("logo.png");
        options.target = Some(ImageTarget::new("some-section_"));

        // When
        let html = render_directive_html(&Directive::Image(Box::new(options)), &index, "index.rst");

        // Then
        assert!(
            html.contains("<a href=\"other.html#some-section\">"),
            "unexpected: {html}"
        );
        assert!(!html.contains("broken-link"), "unexpected: {html}");
    }

    #[test]
    fn test_marks_an_unresolved_reference_target_as_broken() {
        // Given
        let mut options = image("logo.png");
        options.target = Some(ImageTarget::new("missing_"));

        // When
        let html = render(options);

        // Then
        assert!(html.contains("class=\"broken-link\""), "unexpected: {html}");
    }

    #[test]
    fn test_renders_the_name_as_an_anchor() {
        // Given
        let mut options = image("logo.png");
        options.name = Some(TargetName::new("the logo"));

        // When
        let html = render(options);

        // Then
        assert!(
            html.contains("<span id=\"the logo\"></span>"),
            "unexpected: {html}"
        );
    }

    /// Renders a one-image document through the crate's own entry point, so
    /// the reported `image_errors` are observable.
    fn render_document(
        options: ImageOptions,
        assets: &crate::EmbeddedAssets,
    ) -> crate::RenderOutput {
        let doc = rusty_sphinx_ast::Document::new(
            "index.rst".to_string(),
            vec![rusty_sphinx_ast::Node::Directive(Directive::Image(
                Box::new(options),
            ))],
        );
        crate::render_with_assets(
            &doc,
            &ProjectIndex::default(),
            "index.rst",
            &crate::config::SiteConfig::default(),
            assets,
            rusty_sphinx_entity::EntitySchema::empty_ref(),
            &crate::blocks::EntityTemplates::new(),
        )
    }

    #[test]
    fn test_embeds_the_data_uri_when_the_build_supplied_it() {
        // Given
        let mut options = image("logo.svg");
        options.loading = ImageLoading::Embed;
        let mut assets = crate::EmbeddedAssets::new();
        assets.insert(
            std::path::Path::new("logo.svg"),
            "data:image/svg+xml;base64,AAA".to_string(),
        );

        // When
        let output = render_document(options, &assets);

        // Then — the bytes are in the page, not a path to them
        assert!(
            output
                .html
                .contains("src=\"data:image/svg+xml;base64,AAA\""),
            "unexpected: {}",
            output.html
        );
        assert!(
            !output.html.contains("_images/"),
            "unexpected: {}",
            output.html
        );
        assert!(output.image_errors.is_empty());
    }

    #[test]
    fn test_reports_an_embed_whose_bytes_never_arrived() {
        // Given — no sidecar, which is what an undeclared image looks like here
        let mut options = image("logo.svg");
        options.loading = ImageLoading::Embed;

        // When
        let output = render_document(options, &crate::EmbeddedAssets::new());

        // Then — reported, and fallen back to an ordinary link rather than
        // rendering an image with no source at all
        assert_eq!(output.image_errors.len(), 1);
        assert_eq!(output.image_errors[0].uri, "logo.svg");
        assert_eq!(
            output.image_errors[0].code(),
            rusty_sphinx_ast::DiagnosticCode::ImageEmbedUnavailable
        );
        assert!(
            output.html.contains("src=\"_images/logo.svg\""),
            "unexpected: {}",
            output.html
        );
    }

    #[test]
    fn test_reports_nothing_for_a_link_with_no_sidecar() {
        // Given — the ordinary case: nothing asked to be embedded
        let options = image("logo.svg");

        // When
        let output = render_document(options, &crate::EmbeddedAssets::new());

        // Then
        assert!(output.image_errors.is_empty());
    }

    #[test]
    fn test_embedding_resolves_the_key_relative_to_the_document() {
        // Given — a page one directory down, so the asset key is prefixed
        let mut options = image("logo.svg");
        options.loading = ImageLoading::Embed;
        let mut assets = crate::EmbeddedAssets::new();
        assets.insert(
            std::path::Path::new("guide/logo.svg"),
            "data:image/svg+xml;base64,BBB".to_string(),
        );
        let doc = rusty_sphinx_ast::Document::new(
            "guide/intro.rst".to_string(),
            vec![rusty_sphinx_ast::Node::Directive(Directive::Image(
                Box::new(options),
            ))],
        );

        // When
        let output = crate::render_with_assets(
            &doc,
            &ProjectIndex::default(),
            "guide/intro.rst",
            &crate::config::SiteConfig::default(),
            &assets,
            rusty_sphinx_entity::EntitySchema::empty_ref(),
            &crate::blocks::EntityTemplates::new(),
        );

        // Then
        assert!(
            output
                .html
                .contains("src=\"data:image/svg+xml;base64,BBB\""),
            "unexpected: {}",
            output.html
        );
    }

    #[test]
    fn test_escapes_attribute_values() {
        // Given
        let mut options = image("logo.png");
        options.alt = Some("a \"quoted\" logo".to_string());

        // When
        let html = render(options);

        // Then
        assert!(!html.contains("a \"quoted\" logo"), "unexpected: {html}");
        assert!(html.contains("&quot;"), "unexpected: {html}");
    }
}
