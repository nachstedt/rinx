//! The nine options `.. image::` and `.. figure::` share.
//!
//! Both directives open with a run of `:name: value` lines that
//! [`super::super::options::scan_option_lines`] splits off without judging,
//! and [`parse_common_image_options`] consumes the nine both spell the same
//! way — handing back whatever it did not recognize so `.. figure::` can layer
//! its own `:figwidth:`/`:figclass:` on top and diagnose the remainder.
//!
//! This is `directives::table_options`' shape, and exists for the same reason:
//! one shared vocabulary across sibling directives, extras per directive, and
//! wording that never offers an author an option their directive lacks.

use rinx_ast::{
    AssetUri, Diagnostic, DiagnosticCode, ImageAlign, ImageLoading, ImageOptions, ImageTarget,
    Length, LengthOrPercentage, TargetName, Written, is_vertical_name,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::option_value::{Classes, NonEmptyText, Percentage};
use crate::directives::options::{
    DirectiveOptions, OptionLine, OptionRead, choice_names, directive_options,
};

/// Which of the two places an image's options may be written in — the two
/// differ in which `:align:` values are valid and whether `:name:` is
/// allowed at all.
///
/// A block-level `.. image::`/`.. figure::` accepts only the three horizontal
/// alignments and does allow `:name:`; an image inside a substitution
/// definition (`.. |name| image:: ...`) additionally accepts the three
/// vertical alignments — docutils' own rule, since only there is there a text
/// baseline to align to — but refuses `:name:`, since a substitution may be
/// referenced more than once while a name must be unique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::directives) enum ImageContext {
    Standalone,
    Substitution,
}

/// The nine options both image directives accept, before a URI is attached.
///
/// Separate from [`ImageOptions`] itself only because the URI comes from the
/// directive's argument rather than from an option line; [`Self::with_uri`]
/// joins the two.
pub(in crate::directives) struct CommonImageOptions {
    pub alt: Option<String>,
    pub height: Option<Length>,
    pub width: Option<LengthOrPercentage>,
    pub scale: Option<u32>,
    pub align: Option<ImageAlign>,
    pub target: Option<ImageTarget>,
    pub classes: Vec<String>,
    pub name: Option<Written<TargetName>>,
    pub loading: ImageLoading,
}

impl CommonImageOptions {
    pub(in crate::directives) fn empty() -> Self {
        Self {
            alt: None,
            height: None,
            width: None,
            scale: None,
            align: None,
            target: None,
            classes: Vec::new(),
            name: None,
            loading: ImageLoading::default(),
        }
    }

    /// These options together with the URI the directive's argument named.
    pub(in crate::directives) fn with_uri(self, uri: AssetUri) -> ImageOptions {
        ImageOptions {
            uri,
            alt: self.alt,
            height: self.height,
            width: self.width,
            scale: self.scale,
            align: self.align,
            target: self.target,
            classes: self.classes,
            name: self.name,
            loading: self.loading,
            span: None,
        }
    }
}

directive_options! {
    /// The options every image directive takes: docutils' nine.
    pub(in crate::directives) enum ImageOption {
        /// `:alt:`.
        Alt(String) => (["alt"], "Text describing the picture, for readers who cannot see it."),
        /// `:height:`.
        Height(Length) => (["height"], ImageInvalidLength, "The picture's height."),
        /// `:width:`.
        Width(LengthOrPercentage) => (["width"], ImageInvalidLength, "The picture's width, or a percentage of the line."),
        /// `:scale:`.
        Scale(Percentage) => (["scale"], ImageInvalidScale, "The picture's size, as a percentage."),
        /// `:align:`.
        Align => (["align"], Choice(|| choice_names(ImageAlign::ALL, ImageAlign::as_str)), "Where the picture sits."),
        /// `:target:`.
        Target(NonEmptyText) => (["target"], "Where clicking the picture leads."),
        /// `:class:`.
        Class(Classes) => (["class"], "Classes for the picture."),
        /// `:name:`.
        Name(Written<TargetName>) => (["name"], "A label to reference the picture by."),
        /// `:loading:`.
        Loading(ImageLoading) => (["loading"], ImageInvalidLoading, "How the page loads the picture."),
    }
}

/// Records one of the options every image directive takes.
///
/// `directive` only shapes the diagnostics' wording, so a `.. image::` author
/// is never told about `:figwidth:`.
pub(in crate::directives) fn read_image_option(
    option: ImageOption,
    line: &OptionLine,
    options: &mut CommonImageOptions,
    image: ImageSite<'_>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let ImageSite { directive, context } = image;
    match option {
        // docutils' `unchanged`, so an empty `:alt:` is a deliberate
        // empty alt attribute — the accessible spelling for a purely
        // decorative image — rather than a mistake to report.
        ImageOption::Alt(alt) => options.alt = Some(alt),
        ImageOption::Height(height) => options.height = Some(height),
        ImageOption::Width(width) => options.width = Some(width),
        ImageOption::Scale(scale) => options.scale = Some(scale.0),
        // Which values are valid depends on where the picture stands, so the
        // value is read here.
        ImageOption::Align => {
            let parsed = match context {
                ImageContext::Standalone => ImageAlign::parse(&line.value),
                ImageContext::Substitution => ImageAlign::parse_any(&line.value),
            };
            match parsed {
                Some(align) => options.align = Some(align),
                None => report_invalid_align(line, directive, context, diagnostics, ctx),
            }
        }
        ImageOption::Target(target) => options.target = Some(ImageTarget::new(&target.0)),
        ImageOption::Class(classes) => options.classes = classes.0,
        ImageOption::Name(_) if context == ImageContext::Substitution => {
            report_name_not_allowed(line, directive, diagnostics, ctx);
        }
        ImageOption::Name(name) => options.name = Some(name),
        ImageOption::Loading(loading) => options.loading = loading,
    }
}

/// Which image directive an option is read for, and where it stands.
#[derive(Clone, Copy)]
pub(in crate::directives) struct ImageSite<'a> {
    /// The directive's name, for the diagnostics' wording.
    pub(in crate::directives) directive: &'a str,
    /// Whether it stands alone or in a substitution definition.
    pub(in crate::directives) context: ImageContext,
}

/// Consumes the nine shared options out of `option_lines`, returning them
/// together with the lines it did not recognize, in source order — for a
/// substitution definition's image, which reads its own block.
pub(in crate::directives) fn parse_common_image_options<'a>(
    option_lines: &[&'a OptionLine],
    directive: &str,
    context: ImageContext,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (CommonImageOptions, Vec<&'a OptionLine>) {
    let mut options = CommonImageOptions::empty();
    let mut unrecognized = Vec::new();
    let image = ImageSite { directive, context };
    for line in option_lines {
        match ImageOption::read(line, directive, diagnostics, ctx) {
            OptionRead::Read(option) => {
                read_image_option(option, line, &mut options, image, diagnostics, ctx);
            }
            OptionRead::Refused => {}
            OptionRead::NotTaken => unrecognized.push(*line),
        }
    }
    (options, unrecognized)
}

/// Reports an `:align:` value that is not valid in `context`.
///
/// In the standalone context, a vertical name gets its own sentence: docutils
/// *does* accept those, but only on an image inside a substitution
/// definition, so an author who writes one outside of one has hit a real
/// docutils rule rather than a typo, and listing the three horizontal values
/// alone would not explain why. Inside a substitution definition all six are
/// valid, so an unrecognized value there is simply unrecognized.
fn report_invalid_align(
    line: &OptionLine,
    directive: &str,
    context: ImageContext,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let candidates = match context {
        ImageContext::Standalone => ImageAlign::HORIZONTAL,
        ImageContext::Substitution => ImageAlign::ALL,
    };
    let valid = candidates
        .iter()
        .map(|align| align.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let message = if context == ImageContext::Standalone && is_vertical_name(&line.value) {
        format!(
            "{directive}: :align: '{}' aligns an image to the surrounding text baseline, \
             which is only meaningful inside a substitution definition; expected one of {valid}",
            line.value
        )
    } else {
        format!(
            "{directive}: :align: expects one of {valid}, found '{}'",
            line.value
        )
    };
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::ImageInvalidAlign,
        message,
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports a `:name:` written on an image inside a substitution definition,
/// where docutils refuses it: a substitution may be referenced more than
/// once, but a name must be unique, so the two cannot be reconciled.
fn report_name_not_allowed(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::SubstitutionImageNameNotAllowed,
        format!(
            "{directive}: :name: cannot be used on an image inside a substitution definition, \
             since a substitution may be referenced more than once but a name must be unique"
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directives::options::scan_option_lines;
    use rinx_ast::Domain;

    const DIRECTIVE: &str = "image";

    /// Scans `body` into option lines and runs the shared parser over them.
    fn parse(body: &[&str]) -> (CommonImageOptions, Vec<String>, Diagnostics) {
        let lines: Vec<String> = body.iter().map(|line| (*line).to_string()).collect();
        let (option_lines, _) = scan_option_lines(&lines);
        let refs: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let (options, unrecognized) = parse_common_image_options(
            &refs,
            DIRECTIVE,
            ImageContext::Standalone,
            &mut diagnostics,
            &ctx,
        );
        let leftovers = unrecognized
            .iter()
            .map(|line| line.name.clone())
            .collect::<Vec<_>>();
        (options, leftovers, diagnostics)
    }

    fn codes(diagnostics: &Diagnostics) -> Vec<DiagnosticCode> {
        diagnostics
            .entries()
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect()
    }

    #[test]
    fn test_parses_an_alt_text() {
        // Given
        let body = [":alt: A red circle"];

        // When
        let (options, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(options.alt.as_deref(), Some("A red circle"));
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_keeps_an_empty_alt_as_a_deliberate_empty_attribute() {
        // Given — the accessible spelling for a decorative image
        let body = [":alt:"];

        // When
        let (options, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(options.alt.as_deref(), Some(""));
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_parses_a_height_and_width() {
        // Given
        let body = [":height: 3cm", ":width: 50%"];

        // When
        let (options, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(
            options.height.map(|height| height.to_string()),
            Some("3cm".to_string())
        );
        assert_eq!(
            options.width.map(|width| width.to_string()),
            Some("50%".to_string())
        );
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_reports_an_invalid_height() {
        // Given
        let body = [":height: 3rem"];

        // When
        let (options, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(options.height, None);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::ImageInvalidLength]
        );
    }

    #[test]
    fn test_reports_a_percentage_height() {
        // Given — docutils accepts a percentage for :width: but not :height:
        let body = [":height: 50%"];

        // When
        let (options, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(options.height, None);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::ImageInvalidLength]
        );
    }

    #[test]
    fn test_parses_a_scale_with_and_without_a_percent_sign() {
        for raw in [":scale: 50", ":scale: 50%"] {
            // Given / When
            let (options, _, diagnostics) = parse(&[raw]);

            // Then
            assert_eq!(options.scale, Some(50));
            assert!(diagnostics.entries().is_empty());
        }
    }

    #[test]
    fn test_reports_a_negative_scale() {
        // Given
        let body = [":scale: -50"];

        // When
        let (options, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(options.scale, None);
        assert_eq!(codes(&diagnostics), vec![DiagnosticCode::ImageInvalidScale]);
    }

    #[test]
    fn test_reports_a_non_numeric_scale() {
        // Given
        let body = [":scale: half"];

        // When
        let (_, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(codes(&diagnostics), vec![DiagnosticCode::ImageInvalidScale]);
    }

    #[test]
    fn test_parses_every_horizontal_alignment() {
        for align in ImageAlign::HORIZONTAL {
            // Given
            let body = format!(":align: {}", align.as_str());

            // When
            let (options, _, diagnostics) = parse(&[&body]);

            // Then
            assert_eq!(options.align, Some(*align));
            assert!(diagnostics.entries().is_empty());
        }
    }

    #[test]
    fn test_reports_a_vertical_alignment_with_its_own_explanation() {
        // Given
        let body = [":align: middle"];

        // When
        let (options, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(options.align, None);
        assert_eq!(codes(&diagnostics), vec![DiagnosticCode::ImageInvalidAlign]);
        assert!(
            diagnostics.entries()[0]
                .message
                .contains("substitution definition"),
            "expected the substitution-definition explanation, got: {}",
            diagnostics.entries()[0].message
        );
    }

    #[test]
    fn test_reports_an_unknown_alignment_with_the_valid_list() {
        // Given
        let body = [":align: sideways"];

        // When
        let (_, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(codes(&diagnostics), vec![DiagnosticCode::ImageInvalidAlign]);
        assert!(
            diagnostics.entries()[0]
                .message
                .contains("left, center, right")
        );
    }

    #[test]
    fn test_parses_a_url_target() {
        // Given
        let body = [":target: https://example.com/"];

        // When
        let (options, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(
            options.target,
            Some(ImageTarget::Uri("https://example.com/".to_string()))
        );
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_parses_a_reference_target() {
        // Given
        let body = [":target: some label_"];

        // When
        let (options, _, _) = parse(&body);

        // Then
        assert_eq!(
            options.target,
            Some(ImageTarget::Reference(TargetName::new("some label")))
        );
    }

    #[test]
    fn test_reports_an_empty_target() {
        // Given
        let body = [":target:"];

        // When
        let (options, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(options.target, None);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DirectiveEmptyOptionValue]
        );
    }

    #[test]
    fn test_splits_the_class_option() {
        // Given
        let body = [":class: one two"];

        // When
        let (options, _, _) = parse(&body);

        // Then
        assert_eq!(options.classes, vec!["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn test_normalizes_the_name_option() {
        // Given
        let body = [":name: My  Logo"];

        // When
        let (options, _, _) = parse(&body);

        // Then
        assert_eq!(options.name, Some(TargetName::new("my logo").into()));
    }

    #[test]
    fn test_reports_an_empty_name() {
        // Given
        let body = [":name:"];

        // When
        let (options, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(options.name, None);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DirectiveEmptyOptionValue]
        );
    }

    #[test]
    fn test_parses_every_loading_value() {
        for loading in ImageLoading::ALL {
            // Given
            let body = format!(":loading: {}", loading.as_str());

            // When
            let (options, _, diagnostics) = parse(&[&body]);

            // Then
            assert_eq!(options.loading, *loading);
            assert!(diagnostics.entries().is_empty());
        }
    }

    #[test]
    fn test_reports_an_unknown_loading_value() {
        // Given
        let body = [":loading: eager"];

        // When
        let (options, _, diagnostics) = parse(&body);

        // Then
        assert_eq!(options.loading, ImageLoading::Link);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::ImageInvalidLoading]
        );
    }

    #[test]
    fn test_hands_back_options_it_does_not_know() {
        // Given — `.. figure::`'s own two
        let body = [":figwidth: image", ":figclass: framed", ":alt: x"];

        // When
        let (options, leftovers, diagnostics) = parse(&body);

        // Then
        assert_eq!(
            leftovers,
            vec!["figwidth".to_string(), "figclass".to_string()]
        );
        assert_eq!(options.alt.as_deref(), Some("x"));
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_a_later_option_line_wins() {
        // Given
        let body = [":width: 10px", ":width: 20px"];

        // When
        let (options, _, _) = parse(&body);

        // Then
        assert_eq!(
            options.width.map(|width| width.to_string()),
            Some("20px".to_string())
        );
    }

    /// Scans `body` into option lines and runs the shared parser over them in
    /// the substitution-definition context.
    fn parse_in_substitution(body: &[&str]) -> (CommonImageOptions, Vec<String>, Diagnostics) {
        let lines: Vec<String> = body.iter().map(|line| (*line).to_string()).collect();
        let (option_lines, _) = scan_option_lines(&lines);
        let refs: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let (options, unrecognized) = parse_common_image_options(
            &refs,
            DIRECTIVE,
            ImageContext::Substitution,
            &mut diagnostics,
            &ctx,
        );
        let leftovers = unrecognized
            .iter()
            .map(|line| line.name.clone())
            .collect::<Vec<_>>();
        (options, leftovers, diagnostics)
    }

    #[test]
    fn test_substitution_context_accepts_every_vertical_alignment() {
        for align in ImageAlign::VERTICAL {
            // Given
            let body = format!(":align: {}", align.as_str());

            // When
            let (options, _, diagnostics) = parse_in_substitution(&[&body]);

            // Then
            assert_eq!(options.align, Some(*align));
            assert!(diagnostics.entries().is_empty());
        }
    }

    #[test]
    fn test_substitution_context_still_accepts_horizontal_alignment() {
        // Given
        let body = [":align: center"];

        // When
        let (options, _, diagnostics) = parse_in_substitution(&body);

        // Then
        assert_eq!(options.align, Some(ImageAlign::Center));
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_substitution_context_rejects_an_unknown_alignment_listing_all_six() {
        // Given
        let body = [":align: sideways"];

        // When
        let (_, _, diagnostics) = parse_in_substitution(&body);

        // Then
        assert_eq!(codes(&diagnostics), vec![DiagnosticCode::ImageInvalidAlign]);
        assert!(
            diagnostics.entries()[0].message.contains("top"),
            "expected all six alignments listed, got: {}",
            diagnostics.entries()[0].message
        );
    }

    #[test]
    fn test_substitution_context_rejects_a_name_option() {
        // Given
        let body = [":name: My Logo"];

        // When
        let (options, _, diagnostics) = parse_in_substitution(&body);

        // Then
        assert_eq!(options.name, None);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::SubstitutionImageNameNotAllowed]
        );
    }

    #[test]
    fn test_with_uri_attaches_the_argument() {
        // Given
        let (options, _, _) = parse(&[":alt: A logo"]);
        let uri = AssetUri::new("logo.png");

        // When
        let image = options.with_uri(uri.clone());

        // Then
        assert_eq!(image.uri, uri);
        assert_eq!(image.alt.as_deref(), Some("A logo"));
    }
}
