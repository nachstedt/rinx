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

use rusty_sphinx_ast::{
    Diagnostic, DiagnosticCode, ImageAlign, ImageLoading, ImageOptions, ImageTarget, ImageUri,
    Length, LengthOrPercentage, TargetName, is_vertical_name,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::options::OptionLine;

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
    pub name: Option<TargetName>,
    pub loading: ImageLoading,
}

impl CommonImageOptions {
    fn empty() -> Self {
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
    pub(in crate::directives) fn with_uri(self, uri: ImageUri) -> ImageOptions {
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

/// Reads a `:scale:` value.
///
/// docutils' `directives.percentage` strips one trailing `%` and then demands
/// a non-negative integer, so `50` and `50%` are the same option and `-50` is
/// no option at all.
fn parse_scale(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    let number = trimmed.strip_suffix('%').unwrap_or(trimmed).trim();
    number.parse::<u32>().ok()
}

/// Reports an option whose value is required but was left empty.
fn report_empty_value(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::ImageEmptyOptionValue,
        format!(
            "{directive}: :{}: needs a value, so the option was ignored",
            line.name
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Consumes the nine shared options out of `option_lines`, returning them
/// together with the lines it did not recognize, in source order.
///
/// `directive` only shapes the diagnostics' wording, so a `.. image::` author
/// is never told about `:figwidth:`.
pub(in crate::directives) fn parse_common_image_options<'a>(
    option_lines: &[&'a OptionLine],
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (CommonImageOptions, Vec<&'a OptionLine>) {
    let mut options = CommonImageOptions::empty();
    let mut unrecognized = Vec::new();

    for line in option_lines {
        match line.name.as_str() {
            // docutils' `unchanged`, so an empty `:alt:` is a deliberate
            // empty alt attribute — the accessible spelling for a purely
            // decorative image — rather than a mistake to report.
            "alt" => options.alt = Some(line.value.clone()),
            "height" => match Length::new(&line.value) {
                Ok(height) => options.height = Some(height),
                Err(problem) => report_invalid_length(line, directive, &problem, diagnostics, ctx),
            },
            "width" => match LengthOrPercentage::new(&line.value) {
                Ok(width) => options.width = Some(width),
                Err(problem) => report_invalid_length(line, directive, &problem, diagnostics, ctx),
            },
            "scale" => match parse_scale(&line.value) {
                Some(scale) => options.scale = Some(scale),
                None => diagnostics.push(Diagnostic::at(
                    DiagnosticCode::ImageInvalidScale,
                    format!(
                        "{directive}: :scale: expects a non-negative percentage, found '{}'",
                        line.value
                    ),
                    ctx.line_span(line.line_index, &line.raw),
                )),
            },
            "align" => match ImageAlign::parse(&line.value) {
                Some(align) => options.align = Some(align),
                None => report_invalid_align(line, directive, diagnostics, ctx),
            },
            "target" => {
                if line.value.is_empty() {
                    report_empty_value(line, directive, diagnostics, ctx);
                } else {
                    options.target = Some(ImageTarget::new(&line.value));
                }
            }
            "class" => {
                options.classes = line.value.split_whitespace().map(str::to_string).collect();
            }
            "name" => {
                if line.value.is_empty() {
                    report_empty_value(line, directive, diagnostics, ctx);
                } else {
                    options.name = Some(TargetName::new(&line.value));
                }
            }
            "loading" => match ImageLoading::parse(&line.value) {
                Some(loading) => options.loading = loading,
                None => diagnostics.push(Diagnostic::at(
                    DiagnosticCode::ImageInvalidLoading,
                    format!(
                        "{directive}: :loading: expects one of {}, found '{}'",
                        ImageLoading::ALL
                            .iter()
                            .map(|loading| loading.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                        line.value
                    ),
                    ctx.line_span(line.line_index, &line.raw),
                )),
            },
            _ => unrecognized.push(*line),
        }
    }

    (options, unrecognized)
}

/// Reports a measurement option that did not parse, quoting the reason the
/// measurement type gave rather than restating it here.
pub(in crate::directives) fn report_invalid_length(
    line: &OptionLine,
    directive: &str,
    problem: &rusty_sphinx_ast::InvalidLength,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::ImageInvalidLength,
        format!("{directive}: :{}: {problem}", line.name),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports an `:align:` value that is not one of the three horizontal ones.
///
/// A vertical name gets its own sentence: docutils *does* accept those, but
/// only on an image inside a substitution definition, so an author who writes
/// one has hit a real docutils rule rather than a typo, and listing the three
/// horizontal values alone would not explain why.
fn report_invalid_align(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let valid = ImageAlign::ALL
        .iter()
        .map(|align| align.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let message = if is_vertical_name(&line.value) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directives::options::scan_option_lines;
    use rusty_sphinx_ast::Domain;

    const DIRECTIVE: &str = "image";

    /// Scans `body` into option lines and runs the shared parser over them.
    fn parse(body: &[&str]) -> (CommonImageOptions, Vec<String>, Diagnostics) {
        let lines: Vec<String> = body.iter().map(|line| (*line).to_string()).collect();
        let (option_lines, _) = scan_option_lines(&lines);
        let refs: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let (options, unrecognized) =
            parse_common_image_options(&refs, DIRECTIVE, &mut diagnostics, &ctx);
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
        for align in ImageAlign::ALL {
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
            vec![DiagnosticCode::ImageEmptyOptionValue]
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
        assert_eq!(options.name, Some(TargetName::new("my logo")));
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
            vec![DiagnosticCode::ImageEmptyOptionValue]
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

    #[test]
    fn test_with_uri_attaches_the_argument() {
        // Given
        let (options, _, _) = parse(&[":alt: A logo"]);
        let uri = ImageUri::new("logo.png");

        // When
        let image = options.with_uri(uri.clone());

        // Then
        assert_eq!(image.uri, uri);
        assert_eq!(image.alt.as_deref(), Some("A logo"));
    }
}
