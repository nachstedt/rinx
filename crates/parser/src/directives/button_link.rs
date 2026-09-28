//! `.. button-link::` — sphinx-design's button-shaped external link.
//!
//! sphinx-design's `_ButtonDirective.option_spec` is the authority for every
//! name and value below, and its `directives.choice` validators are why
//! `:color:` and `:align:` are matched case-insensitively against a closed
//! set.
//!
//! Two shapes here differ from the sphinx-design directives already parsed in
//! this crate. The directive's **content is its label**, parsed as inline
//! markup rather than as block content — the only other place this build does
//! that is a `.. dropdown::`'s title, and unlike that one-line title a label
//! may run over several lines, so it is parsed through an accumulated
//! [`SourceMap`] and a role inside it reports at its own line and column. And
//! the argument is **required**: with nothing to point at there is no button,
//! so it degrades to a [`Directive::Malformed`] that quotes its source, the
//! way an argument-less `.. image::` does.
//!
//! An unreadable *option* never costs the button, the error-resilience every
//! directive here follows.

use rinx_ast::{
    ButtonFlag, ButtonLink, ButtonTarget, Diagnostic, DiagnosticCode, Directive, InlineNode,
    SemanticColor, Span, TextAlign,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::indent::unindent_body_lines;
use crate::inline::{SourceMap, parse_inline_text_mapped};

use super::error_node::malformed_directive;
use super::options::{OptionLine, report_unknown_options, scan_option_lines};

const DIRECTIVE: &str = "button-link";

/// Parses a `.. button-link::` into a [`Directive::ButtonLink`].
pub(super) fn parse_button_link(
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);

    let target = normalize_uri(argument);
    if target.is_empty() {
        return malformed_directive(
            DIRECTIVE,
            "",
            body_lines,
            DiagnosticCode::ButtonLinkMissingTarget,
            format!("{DIRECTIVE}: the directive needs a URL as its argument"),
            directive_span,
            diagnostics,
        );
    }

    let mut button = ButtonLink::new(ButtonTarget::Url(target));
    button.span = directive_span;
    let unrecognized = read_options(&mut button, &option_lines, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        DIRECTIVE,
        DiagnosticCode::DirectiveButtonLinkUnknownOption,
        diagnostics,
        ctx,
    );
    if button.has_unusable_outline() {
        report_unusable_outline(&option_lines, diagnostics, ctx);
    }

    button.label = parse_label(
        &unindented_lines[body_start..],
        body_start,
        diagnostics,
        ctx,
    );
    Directive::ButtonLink(Box::new(button))
}

/// Normalizes the directive's argument into a URL.
///
/// docutils' `directives.uri` removes **every** whitespace character rather
/// than trimming the ends, because an argument may be wrapped across lines and
/// a URL split that way is still one URL. Written out here rather than reused
/// from the image directives, which normalize a *path* through
/// `ast::AssetUri` and carry resolution rules a button has no use for.
fn normalize_uri(argument: &str) -> String {
    argument.split_whitespace().collect()
}

/// Reads the directive's content as the button's label.
///
/// The lines are joined with the newlines docutils' `inline_text` joins them
/// with, and the map records where each one came from, so a role written on
/// the label's third line is reported against that line. `body_start` rebases
/// the map onto the directive body, since the lines arrive already split from
/// the option block above them.
fn parse_label(
    content: &[String],
    body_start: usize,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<InlineNode> {
    let mut text = String::new();
    let mut map = SourceMap::none();
    for (offset, line) in content.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        map.push(
            text.len(),
            trimmed,
            body_start + offset,
            line.len() - line.trim_start().len(),
        );
        text.push_str(trimmed);
    }
    if text.is_empty() {
        return Vec::new();
    }

    let label = parse_inline_text_mapped(&text, ctx.default_domain, &map, ctx);
    report_nested_references(&label, diagnostics, ctx);
    label
}

/// Reports a reference role written inside the label.
///
/// A button *is* a link, so a link in its label would nest one `<a>` inside
/// another. The reference is kept in the node and rendered as its text alone
/// — dropping it would lose the words the author wrote, which is the mistake
/// an unknown directive's swallowed body already taught this crate not to
/// repeat.
fn report_nested_references(
    label: &[InlineNode],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    for node in label.iter().filter(|node| node.renders_as_link()) {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::ButtonLinkNestedReference,
            format!(
                "{DIRECTIVE}: a reference in the label would nest a link inside the button, \
                 so it is shown as plain text — the button already links to its argument"
            ),
            node.span().or_else(|| ctx.line_span(0, "")),
        ));
    }
}

/// Reads every option onto `button`, returning the lines nobody claimed.
fn read_options<'a>(
    button: &mut ButtonLink,
    option_lines: &'a [OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<&'a OptionLine> {
    let mut unrecognized = Vec::new();
    for line in option_lines {
        // Checked before the named options: `directives.flag` accepts no value
        // at all, so what matters is only that one of the four was written.
        if let Some(flag) = ButtonFlag::parse(&line.name) {
            button.flags.insert(flag);
            continue;
        }
        match line.name.as_str() {
            "color" => match SemanticColor::parse(&line.value) {
                Some(color) => button.color = Some(color),
                None => report_invalid_choice(
                    line,
                    "color",
                    &names(SemanticColor::ALL, SemanticColor::as_str),
                    DiagnosticCode::ButtonLinkInvalidColor,
                    diagnostics,
                    ctx,
                ),
            },
            "align" => match TextAlign::parse(&line.value) {
                Some(align) => button.align = Some(align),
                None => report_invalid_choice(
                    line,
                    "align",
                    &names(TextAlign::ALL, TextAlign::as_str),
                    DiagnosticCode::ButtonLinkInvalidAlign,
                    diagnostics,
                    ctx,
                ),
            },
            "tooltip" => {
                if line.value.is_empty() {
                    report_empty_value(line, "tooltip", diagnostics, ctx);
                } else {
                    button.tooltip = Some(line.value.clone());
                }
            }
            "class" => button.class = split_classes(&line.value),
            // Accepted by sphinx-design here only because both button
            // directives share one option spec; it selects how a
            // `.. button-ref::` resolves its argument, and this directive's
            // argument is a URL that resolves against nothing.
            "ref-type" => report_unsupported(
                line,
                "it selects how a `.. button-ref::` resolves its target, and this directive's \
                 argument is a URL",
                diagnostics,
                ctx,
            ),
            _ => unrecognized.push(line),
        }
    }
    unrecognized
}

/// Splits a class-list option value, as docutils' `class_option` does.
fn split_classes(value: &str) -> Vec<String> {
    value.split_whitespace().map(str::to_string).collect()
}

/// The written names of a closed option vocabulary, for a diagnostic.
fn names<T: Copy>(values: &[T], name_of: fn(T) -> &'static str) -> String {
    values
        .iter()
        .copied()
        .map(name_of)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Reports a value that is not one of the ones the option accepts, listing
/// them in the order sphinx-design declares them.
fn report_invalid_choice(
    line: &OptionLine,
    option: &str,
    accepted: &str,
    code: DiagnosticCode,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        code,
        format!(
            "{DIRECTIVE}: :{option}: expects one of {accepted}, found '{}'",
            line.value
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports an option written without the value it needs.
fn report_empty_value(
    line: &OptionLine,
    option: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::ButtonLinkEmptyOptionValue,
        format!(
            "{DIRECTIVE}: a :{option}: option needs a value: {}",
            line.raw
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports an option this build refuses by name rather than silently ignoring.
fn report_unsupported(
    line: &OptionLine,
    advice: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::ButtonLinkUnsupportedOption,
        format!(
            "{DIRECTIVE}: :{}: is not supported, so it was ignored — {advice}",
            line.name
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports an `:outline:` with no `:color:` to outline.
///
/// sphinx-design adds a colour class only when `:color:` was written, so
/// `:outline:` alone produces nothing at all. Pointed at the `:outline:` line,
/// which is the one the author can act on — the missing option has no line.
fn report_unusable_outline(
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let Some(line) = option_lines
        .iter()
        .find(|line| line.name == ButtonFlag::Outline.as_str())
    else {
        return;
    };
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::ButtonLinkUnusableOutline,
        format!(
            "{DIRECTIVE}: :outline: has no :color: to outline, so it was ignored — \
             add a :color:, which is what an outline is drawn in"
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

#[cfg(test)]
mod tests;
