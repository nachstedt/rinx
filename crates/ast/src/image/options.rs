//! The option set `.. image::` and `.. figure::` share.
//!
//! Both directives take exactly these nine; `.. figure::` then adds
//! `:figwidth:` and `:figclass:` of its own. Bundling them into one struct is
//! what lets one parser and one renderer serve both, in the same way
//! `directives::table_options` serves all three table directives.

use serde::{Deserialize, Serialize};

use crate::span::Span;
use crate::target_name::TargetName;

use super::align::ImageAlign;
use super::length::{Length, LengthOrPercentage};
use super::uri::{ImageTarget, ImageUri};

/// How an image's bytes reach the page — docutils' `:loading:` option.
///
/// An enum rather than a pair of flags because the three are mutually
/// exclusive answers to one question, and only one of them (`Lazy`) has any
/// counterpart in the rendered HTML's own vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ImageLoading {
    /// `:loading: link` — an ordinary `src` pointing at the bundled file.
    /// docutils' default, and so this type's.
    #[default]
    Link,
    /// `:loading: embed` — the file's bytes go into the page itself, as a
    /// `data:` URI. Requires the file to be read during the build, which is
    /// why an embedding document gets an asset sidecar (see the worker's
    /// `embed_assets` command).
    Embed,
    /// `:loading: lazy` — an ordinary `src`, plus HTML's own `loading="lazy"`
    /// attribute, which lets the browser defer fetching an off-screen image.
    Lazy,
}

impl ImageLoading {
    /// Every value, in docutils' own declaration order.
    pub const ALL: &'static [Self] = &[Self::Embed, Self::Link, Self::Lazy];

    /// The name this value is written as.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Embed => "embed",
            Self::Link => "link",
            Self::Lazy => "lazy",
        }
    }

    /// Reads a `:loading:` value, ignoring case and surrounding whitespace.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|loading| loading.as_str() == normalized)
    }
}

/// The nine options both image directives accept.
///
/// `scale` is kept as written rather than folded into `width`/`height` at
/// parse time, because the parser cannot know the image's own size and the
/// renderer must be able to tell "scaled to 50%" from "written as 50%".
/// [`Self::rendered_width`] and [`Self::rendered_height`] are where the two
/// are finally combined.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageOptions {
    /// The directive argument — what to display.
    pub uri: ImageUri,
    /// `:alt:` — replacement text, emitted as the `alt` attribute.
    pub alt: Option<String>,
    /// `:height:` — a length; docutils accepts no percentage here.
    pub height: Option<Length>,
    /// `:width:` — a length or a percentage of the available width.
    pub width: Option<LengthOrPercentage>,
    /// `:scale:` — a percentage the given `:width:`/`:height:` are multiplied
    /// by. Stored as written; see the struct docs for why it is not applied
    /// here.
    pub scale: Option<u32>,
    /// `:align:` — horizontal placement.
    pub align: Option<ImageAlign>,
    /// `:target:` — makes the image a link.
    pub target: Option<ImageTarget>,
    /// `:class:` — space-separated class names, already split.
    pub classes: Vec<String>,
    /// `:name:` — reuses [`TargetName`] like the table and math directives do,
    /// so it registers in `ProjectIndex::targets` with no conversion.
    pub name: Option<TargetName>,
    /// `:loading:` — how the bytes reach the page.
    pub loading: ImageLoading,
    /// Where the directive was written.
    ///
    /// Carried for the same reason [`crate::Directive::Math`] carries one: an
    /// image can still fail *after* parsing succeeds — a `:loading: embed`
    /// whose bytes never reached the renderer is only discoverable there, and
    /// the renderer would otherwise have no position to report it against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl ImageOptions {
    /// The options of a bare `.. image:: uri` with nothing else written.
    #[must_use]
    pub fn new(uri: ImageUri) -> Self {
        Self {
            uri,
            alt: None,
            height: None,
            width: None,
            scale: None,
            align: None,
            target: None,
            classes: Vec::new(),
            name: None,
            loading: ImageLoading::default(),
            span: None,
        }
    }

    /// The width to render with, `:scale:` applied.
    ///
    /// `None` when no `:width:` was given — including when a `:scale:` was,
    /// since scaling an unknown width would need the image's own dimensions,
    /// which no phase of this build reads (see `docs/decisions/007-image-assets.md`).
    #[must_use]
    pub fn rendered_width(&self) -> Option<LengthOrPercentage> {
        let width = self.width.as_ref()?;
        Some(match self.scale {
            Some(scale) => width.scaled(scale),
            None => width.clone(),
        })
    }

    /// The height to render with, `:scale:` applied. See
    /// [`Self::rendered_width`] for the `None` cases.
    #[must_use]
    pub fn rendered_height(&self) -> Option<Length> {
        let height = self.height.as_ref()?;
        Some(match self.scale {
            Some(scale) => height.scaled(scale),
            None => height.clone(),
        })
    }

    /// Whether a `:scale:` was written that nothing can be applied to.
    ///
    /// docutils would read the image file's own dimensions in this case. This
    /// build never opens the file while rendering, so the option is dropped —
    /// and dropping an author's explicit instruction silently is exactly what
    /// the diagnostics exist to prevent, so the parser reports this.
    #[must_use]
    pub const fn has_unusable_scale(&self) -> bool {
        self.scale.is_some() && self.width.is_none() && self.height.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options_with(width: Option<&str>, height: Option<&str>, scale: Option<u32>) -> ImageOptions {
        let mut options = ImageOptions::new(ImageUri::new("logo.png"));
        options.width = width.map(|raw| LengthOrPercentage::new(raw).expect("valid width"));
        options.height = height.map(|raw| Length::new(raw).expect("valid height"));
        options.scale = scale;
        options
    }

    #[test]
    fn test_loading_parse_reads_every_value() {
        for loading in ImageLoading::ALL {
            // Given
            let raw = loading.as_str();

            // When
            let parsed = ImageLoading::parse(raw);

            // Then
            assert_eq!(parsed, Some(*loading));
        }
    }

    #[test]
    fn test_loading_parse_ignores_case_and_whitespace() {
        // Given
        let raw = "  LAZY ";

        // When
        let parsed = ImageLoading::parse(raw);

        // Then
        assert_eq!(parsed, Some(ImageLoading::Lazy));
    }

    #[test]
    fn test_loading_parse_rejects_an_unknown_value() {
        // Given
        let raw = "eager";

        // When
        let parsed = ImageLoading::parse(raw);

        // Then
        assert_eq!(parsed, None);
    }

    #[test]
    fn test_loading_defaults_to_link() {
        // Given / When
        let loading = ImageLoading::default();

        // Then
        assert_eq!(loading, ImageLoading::Link);
    }

    #[test]
    fn test_new_leaves_every_option_unset() {
        // Given
        let uri = ImageUri::new("logo.png");

        // When
        let options = ImageOptions::new(uri.clone());

        // Then
        assert_eq!(options.uri, uri);
        assert_eq!(options.alt, None);
        assert_eq!(options.height, None);
        assert_eq!(options.width, None);
        assert_eq!(options.scale, None);
        assert_eq!(options.align, None);
        assert_eq!(options.target, None);
        assert!(options.classes.is_empty());
        assert_eq!(options.name, None);
        assert_eq!(options.loading, ImageLoading::Link);
    }

    #[test]
    fn test_rendered_width_returns_the_written_width_without_a_scale() {
        // Given
        let options = options_with(Some("200px"), None, None);

        // When
        let width = options.rendered_width();

        // Then
        assert_eq!(width.map(|width| width.to_string()), Some("200px".into()));
    }

    #[test]
    fn test_rendered_width_applies_the_scale() {
        // Given
        let options = options_with(Some("200px"), None, Some(50));

        // When
        let width = options.rendered_width();

        // Then
        assert_eq!(width.map(|width| width.to_string()), Some("100px".into()));
    }

    #[test]
    fn test_rendered_height_applies_the_scale() {
        // Given
        let options = options_with(None, Some("4cm"), Some(25));

        // When
        let height = options.rendered_height();

        // Then
        assert_eq!(height.map(|height| height.to_string()), Some("1cm".into()));
    }

    #[test]
    fn test_rendered_dimensions_are_none_when_unwritten() {
        // Given
        let options = options_with(None, None, Some(50));

        // When / Then
        assert_eq!(options.rendered_width(), None);
        assert_eq!(options.rendered_height(), None);
    }

    #[test]
    fn test_has_unusable_scale_when_no_dimension_was_written() {
        // Given
        let options = options_with(None, None, Some(50));

        // When / Then
        assert!(options.has_unusable_scale());
    }

    #[test]
    fn test_has_no_unusable_scale_when_a_width_was_written() {
        // Given
        let options = options_with(Some("10px"), None, Some(50));

        // When / Then
        assert!(!options.has_unusable_scale());
    }

    #[test]
    fn test_has_no_unusable_scale_without_a_scale() {
        // Given
        let options = options_with(None, None, None);

        // When / Then
        assert!(!options.has_unusable_scale());
    }

    #[test]
    fn test_serialization_round_trips() {
        // Given
        let mut options = options_with(Some("50%"), Some("3em"), Some(80));
        options.alt = Some("A logo".to_string());
        options.align = Some(ImageAlign::Center);
        options.target = Some(ImageTarget::new("https://example.com"));
        options.classes = vec!["fancy".to_string()];
        options.name = Some(TargetName::new("the logo"));
        options.loading = ImageLoading::Embed;

        // When
        let json = serde_json::to_string(&options).expect("should serialize");
        let restored: ImageOptions = serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, options);
    }
}
