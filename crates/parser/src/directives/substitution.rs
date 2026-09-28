//! `.. |name| replace::`/`unicode::`/`image::` — substitution definitions.
//!
//! Recognizing one is [`super::dispatch::parse_body_directive`]'s job: the
//! marker's own name, `|name| inner`, is not a directive name at all, so it
//! is split with [`split_substitution_marker`] before anything here runs.
//! What is particular to each of the three modelled directives is parsed
//! below; `date` and `raw` are deliberately unmodelled (see
//! [`rinx_ast::SubstitutionDefinition`]) and so fall back to
//! [`Directive::Unknown`] like any other unrecognized directive name, via
//! [`parse_substitution_definition`] returning `None`.
//!
//! Resolving a reference against the definitions collected here is a
//! separate, later concern — a whole-document pass
//! (`crate::blocks::substitutions::resolve_substitutions`) that runs once
//! parsing finishes, since a `|name|` may be written before its definition.

use rinx_ast::{
    AssetUri, Diagnostic, DiagnosticCode, Directive, Span, SubstitutionDefinition,
    SubstitutionKind, TrimSides,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::indent::unindent_body_lines;
use crate::inline::{SourceMap, parse_inline_text_mapped};

use super::body::body_span;
use super::image::{ImageContext, parse_common_image_options, report_option_conflicts};
use super::options::{OptionLine, report_unknown_options, scan_option_lines};

/// Splits a directive marker's name into a substitution name and the inner
/// directive it embeds, for `name`s of the form `|substitution text| inner`.
///
/// Returns `None` for anything else, including a marker whose substitution
/// text has leading/trailing whitespace — docutils forbids that outright, and
/// this build doesn't try to explain the rejection any further than falling
/// back to [`Directive::Unknown`], the same as any other malformed marker.
pub(super) fn split_substitution_marker(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix('|')?;
    let (sub_name, after) = rest.split_once('|')?;
    if sub_name.is_empty() || sub_name != sub_name.trim() {
        return None;
    }
    let inner = after.trim_start();
    if inner.is_empty() || inner.contains(char::is_whitespace) {
        return None;
    }
    Some((sub_name, inner))
}

/// Parses the directive embedded in a `.. |sub_name| inner::` marker into a
/// [`Directive::SubstitutionDefinition`], or `None` when `inner` names none
/// of the three modelled substitution directives, or when the one it does
/// name failed outright (a missing image URI) — either way the caller falls
/// back to [`Directive::Unknown`].
pub(in crate::directives) fn parse_substitution_definition(
    sub_name: &str,
    inner: &str,
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Directive> {
    let kind = match inner {
        "replace" => Some(parse_replace_kind(argument, body_lines, ctx)),
        "unicode" => Some(parse_unicode_kind(
            argument,
            body_lines,
            directive_span,
            diagnostics,
            ctx,
        )),
        "image" => parse_image_kind(argument, directive_span, body_lines, diagnostics, ctx),
        _ => None,
    }?;
    Some(Directive::SubstitutionDefinition(SubstitutionDefinition {
        name: sub_name.to_string(),
        kind,
        span: directive_span,
    }))
}

/// The text a `replace` substitution's content is inline-parsed from: the
/// argument on the marker's own line, plus any further lines written below
/// it — docutils reads the whole thing as one paragraph.
///
/// Joined with spaces and inline-parsed under [`SourceMap::none`], so a
/// cross-reference written inside a `replace` substitution degrades to a
/// positionless diagnostic rather than a wrong one — the same trade-off
/// `.. csv-table::`'s generated cells make, and for the same reason: there is
/// no single source line this text belongs to.
fn replace_content_text(argument: &str, body_lines: &[&str]) -> String {
    let mut text = argument.trim().to_string();
    for line in body_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(trimmed);
    }
    text
}

fn parse_replace_kind(argument: &str, body_lines: &[&str], ctx: &ParseCtx<'_>) -> SubstitutionKind {
    let text = replace_content_text(argument, body_lines);
    let inlines = parse_inline_text_mapped(&text, ctx.default_domain, &SourceMap::none(), ctx);
    SubstitutionKind::Replace(inlines)
}

const UNICODE_DIRECTIVE: &str = "unicode";

fn parse_unicode_kind(
    argument: &str,
    body_lines: &[&str],
    directive_span: Option<Span>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> SubstitutionKind {
    let unindented = unindent_body_lines(body_lines);
    let (option_lines, _) = scan_option_lines(&unindented);
    let mut trim = TrimSides::default();
    let mut unrecognized = Vec::new();
    for line in &option_lines {
        match line.name.as_str() {
            "trim" => {
                trim.ltrim = true;
                trim.rtrim = true;
            }
            "ltrim" => trim.ltrim = true,
            "rtrim" => trim.rtrim = true,
            _ => unrecognized.push(line),
        }
    }
    report_unknown_options(
        &unrecognized,
        UNICODE_DIRECTIVE,
        DiagnosticCode::DirectiveUnknownOption,
        diagnostics,
        ctx,
    );

    let span = directive_span.or_else(|| body_span(body_lines, ctx));
    let text = decode_unicode_codes(argument, span, diagnostics);
    SubstitutionKind::Unicode { text, trim }
}

/// Decodes a `unicode` substitution's argument into the literal text it
/// names: decimal numbers, hexadecimal numbers prefixed by `0x`, `x`, `\x`,
/// `U+`, `u` or `\u`, or XML-style hexadecimal character entities
/// (`&#x1a2b;`), whitespace-separated — with anything from a literal `" .. "`
/// onward treated as an author's comment, exactly as docutils documents it.
fn decode_unicode_codes(
    argument: &str,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) -> String {
    let codes = argument.find(" .. ").map_or(argument, |at| &argument[..at]);
    let mut text = String::new();
    for token in codes.split_whitespace() {
        match decode_unicode_token(token) {
            Some(ch) => text.push(ch),
            None => diagnostics.push(Diagnostic::at(
                DiagnosticCode::SubstitutionInvalidUnicodeCode,
                format!(
                    "{UNICODE_DIRECTIVE}: '{token}' is not a decimal number, a recognized \
                     hexadecimal form, or a valid Unicode codepoint"
                ),
                span,
            )),
        }
    }
    text
}

/// Decodes one whitespace-separated token of a `unicode` substitution's
/// argument into the character it names.
fn decode_unicode_token(token: &str) -> Option<char> {
    let value = if let Some(hex) = token.strip_prefix("&#x").and_then(|s| s.strip_suffix(';')) {
        u32::from_str_radix(hex, 16).ok()?
    } else if let Some(hex) = token.strip_prefix("0x") {
        u32::from_str_radix(hex, 16).ok()?
    } else if let Some(hex) = token.strip_prefix("\\x") {
        u32::from_str_radix(hex, 16).ok()?
    } else if let Some(hex) = token.strip_prefix("U+") {
        u32::from_str_radix(hex, 16).ok()?
    } else if let Some(hex) = token.strip_prefix("\\u") {
        u32::from_str_radix(hex, 16).ok()?
    } else if let Some(hex) = token.strip_prefix('x') {
        u32::from_str_radix(hex, 16).ok()?
    } else if let Some(hex) = token.strip_prefix('u') {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        token.parse::<u32>().ok()?
    };
    char::from_u32(value)
}

const IMAGE_DIRECTIVE: &str = "image";

fn parse_image_kind(
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<SubstitutionKind> {
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, _) = scan_option_lines(&unindented_lines);
    let refs: Vec<&OptionLine> = option_lines.iter().collect();
    let (common, unrecognized) = parse_common_image_options(
        &refs,
        IMAGE_DIRECTIVE,
        ImageContext::Substitution,
        diagnostics,
        ctx,
    );
    report_unknown_options(
        &unrecognized,
        IMAGE_DIRECTIVE,
        DiagnosticCode::DirectiveUnknownOption,
        diagnostics,
        ctx,
    );

    if argument.trim().is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::ImageMissingUri,
            format!("{IMAGE_DIRECTIVE}: the directive needs an image path or URL as its argument"),
            body_span(body_lines, ctx),
        ));
        return None;
    }

    let mut options = common.with_uri(AssetUri::new(argument));
    options.span = directive_span;
    report_option_conflicts(&options, &option_lines, IMAGE_DIRECTIVE, diagnostics, ctx);
    Some(SubstitutionKind::Image(Box::new(options)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Domain, ImageAlign, InlineNode};

    fn dispatch(name: &str, argument: &str, body: &[&str]) -> Option<(Directive, Diagnostics)> {
        let (sub_name, inner) = split_substitution_marker(name)?;
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let directive = parse_substitution_definition(
            sub_name,
            inner,
            argument,
            None,
            body,
            &mut diagnostics,
            &ctx,
        )?;
        Some((directive, diagnostics))
    }

    #[test]
    fn test_split_substitution_marker_splits_name_and_inner_directive() {
        // Given / When
        let split = split_substitution_marker("|release| replace");

        // Then
        assert_eq!(split, Some(("release", "replace")));
    }

    #[test]
    fn test_split_substitution_marker_rejects_a_name_with_no_bars() {
        // Given / When / Then
        assert_eq!(split_substitution_marker("toctree"), None);
    }

    #[test]
    fn test_split_substitution_marker_rejects_leading_whitespace_in_the_name() {
        // Given / When / Then — docutils: substitution text may not begin or
        // end with whitespace
        assert_eq!(split_substitution_marker("| release| replace"), None);
    }

    #[test]
    fn test_split_substitution_marker_rejects_a_missing_inner_directive() {
        // Given / When / Then
        assert_eq!(split_substitution_marker("|release|"), None);
    }

    #[test]
    fn test_split_substitution_marker_rejects_an_inner_directive_with_a_domain_prefix() {
        // Given — the inner name has to be one bare word
        let split = split_substitution_marker("|release| py replace");

        // Then
        assert_eq!(split, None);
    }

    #[test]
    fn test_parse_substitution_definition_returns_none_for_an_unmodelled_directive() {
        // Given — `date` is deliberately unmodelled, see the module doc
        let result = dispatch("|today| date", "", &[]);

        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_replace_produces_inline_parsed_content() {
        // Given
        let (directive, diagnostics) = dispatch("|reST| replace", "reStructuredText", &[]).unwrap();

        // Then
        let Directive::SubstitutionDefinition(definition) = directive else {
            panic!("expected a substitution definition");
        };
        assert_eq!(definition.name, "reST");
        assert_eq!(
            definition.kind,
            SubstitutionKind::Replace(vec![InlineNode::Text("reStructuredText".to_string())])
        );
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_parse_replace_parses_inline_markup_in_its_content() {
        // Given
        let (directive, _) = dispatch("|Python| replace", "*Python*", &[]).unwrap();

        // Then
        let Directive::SubstitutionDefinition(definition) = directive else {
            panic!("expected a substitution definition");
        };
        assert_eq!(
            definition.kind,
            SubstitutionKind::Replace(vec![InlineNode::Emphasis("Python".to_string())])
        );
    }

    #[test]
    fn test_parse_replace_joins_continuation_lines() {
        // Given — docutils reads the content as one paragraph
        let (directive, _) = dispatch(
            "|long| replace",
            "This is a rather long",
            &["   replacement."],
        )
        .unwrap();

        // Then
        let Directive::SubstitutionDefinition(definition) = directive else {
            panic!("expected a substitution definition");
        };
        assert_eq!(
            definition.kind,
            SubstitutionKind::Replace(vec![InlineNode::Text(
                "This is a rather long replacement.".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_unicode_decodes_a_hex_codepoint_with_0x_prefix() {
        // Given
        let (directive, diagnostics) = dispatch("|copy| unicode", "0xA9", &[]).unwrap();

        // Then
        let Directive::SubstitutionDefinition(definition) = directive else {
            panic!("expected a substitution definition");
        };
        assert_eq!(
            definition.kind,
            SubstitutionKind::Unicode {
                text: "\u{a9}".to_string(),
                trim: TrimSides::default(),
            }
        );
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_parse_unicode_drops_a_trailing_comment() {
        // Given
        let (directive, _) = dispatch("|copy| unicode", "0xA9 .. copyright sign", &[]).unwrap();

        // Then
        let Directive::SubstitutionDefinition(definition) = directive else {
            panic!("expected a substitution definition");
        };
        assert_eq!(
            definition.kind,
            SubstitutionKind::Unicode {
                text: "\u{a9}".to_string(),
                trim: TrimSides::default(),
            }
        );
    }

    #[test]
    fn test_parse_unicode_decodes_several_forms_in_one_argument() {
        // Given — U+2122 and a decimal number
        let (directive, _) = dispatch("|mixed| unicode", "U+2122 65", &[]).unwrap();

        // Then
        let Directive::SubstitutionDefinition(definition) = directive else {
            panic!("expected a substitution definition");
        };
        assert_eq!(
            definition.kind,
            SubstitutionKind::Unicode {
                text: "\u{2122}A".to_string(),
                trim: TrimSides::default(),
            }
        );
    }

    #[test]
    fn test_parse_unicode_reports_an_unrecognized_token() {
        // Given
        let (_, diagnostics) = dispatch("|bogus| unicode", "not-a-code", &[]).unwrap();

        // Then
        assert_eq!(
            diagnostics
                .entries()
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>(),
            vec![DiagnosticCode::SubstitutionInvalidUnicodeCode]
        );
    }

    #[test]
    fn test_parse_unicode_reads_the_trim_flag() {
        // Given
        let (directive, _) = dispatch("|nbsp| unicode", "0x20", &["   :trim:"]).unwrap();

        // Then
        let Directive::SubstitutionDefinition(definition) = directive else {
            panic!("expected a substitution definition");
        };
        let SubstitutionKind::Unicode { trim, .. } = definition.kind else {
            panic!("expected Unicode");
        };
        assert!(trim.ltrim && trim.rtrim);
    }

    #[test]
    fn test_parse_unicode_reads_ltrim_and_rtrim_independently() {
        // Given
        let (directive, _) = dispatch("|x| unicode", "0x20", &["   :ltrim:"]).unwrap();

        // Then
        let Directive::SubstitutionDefinition(definition) = directive else {
            panic!("expected a substitution definition");
        };
        let SubstitutionKind::Unicode { trim, .. } = definition.kind else {
            panic!("expected Unicode");
        };
        assert!(trim.ltrim && !trim.rtrim);
    }

    #[test]
    fn test_parse_image_produces_image_options() {
        // Given
        let (directive, diagnostics) = dispatch("|biohazard| image", "biohazard.png", &[]).unwrap();

        // Then
        let Directive::SubstitutionDefinition(definition) = directive else {
            panic!("expected a substitution definition");
        };
        let SubstitutionKind::Image(options) = definition.kind else {
            panic!("expected Image");
        };
        assert_eq!(options.uri, AssetUri::new("biohazard.png"));
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_parse_image_accepts_a_vertical_alignment() {
        // Given
        let (directive, diagnostics) =
            dispatch("|red light| image", "red_light.png", &["   :align: top"]).unwrap();

        // Then
        let Directive::SubstitutionDefinition(definition) = directive else {
            panic!("expected a substitution definition");
        };
        let SubstitutionKind::Image(options) = definition.kind else {
            panic!("expected Image");
        };
        assert_eq!(options.align, Some(ImageAlign::Top));
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_parse_image_rejects_a_name_option() {
        // Given
        let (_, diagnostics) =
            dispatch("|logo| image", "logo.png", &["   :name: the logo"]).unwrap();

        // Then
        assert_eq!(
            diagnostics
                .entries()
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>(),
            vec![DiagnosticCode::SubstitutionImageNameNotAllowed]
        );
    }

    #[test]
    fn test_parse_image_returns_none_for_a_missing_uri() {
        // Given / When / Then — falls back to `Directive::Unknown` upstream
        assert!(dispatch("|logo| image", "", &[]).is_none());
    }
}
