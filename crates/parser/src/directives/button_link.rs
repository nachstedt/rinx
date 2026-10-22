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

use crate::directives::invocation::{DirectiveInvocation, ParseState, single};
use crate::directives::spec::{Arguments, DirectiveShape, DirectiveSpec};
use rinx_ast::{
    ButtonFlag, ButtonLink, ButtonTarget, Diagnostic, DiagnosticCode, Directive, InlineNode,
    SemanticColor, TextAlign,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::inline::{SourceMap, parse_inline_text_mapped};

use super::classes::split_classes;
use super::error_node::malformed_node;
use super::options::{
    DirectiveOptions, OptionLine, choice_names, classify_options, directive_options, uri_argument,
};

const DIRECTIVE: &str = "button-link";

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["button-link"],
    doc: "A link drawn as a button.",
    shape: DirectiveShape {
        arguments: Arguments::ONE_TEXT,
        takes_options: true,
        takes_content: true,
        content_hint: None,
    },
    options: <ButtonLinkOption as DirectiveOptions>::GROUPS,
};

/// Interprets `.. button-link::`.
pub(in crate::directives) fn interpret_button_link(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    single(parse_button_link(input, state.diagnostics))
}

/// Parses a `.. button-link::` into a [`Directive::ButtonLink`].
pub(super) fn parse_button_link(
    invocation: &DirectiveInvocation<'_, '_>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let ctx = &invocation.ctx;
    let directive_span = invocation.span;
    let option_lines = &invocation.options;

    let target = uri_argument(invocation.argument(0));
    if target.is_empty() {
        // The reader has reported the missing argument.
        return malformed_node(
            DIRECTIVE,
            "",
            invocation.source_body,
            format!("{DIRECTIVE}: the directive needs a URL as its argument"),
        );
    }

    let mut button = ButtonLink::new(ButtonTarget::Url(target));
    button.span = directive_span;
    read_options(&mut button, option_lines, diagnostics, ctx);
    if button.has_unusable_outline() {
        report_unusable_outline(option_lines, diagnostics, ctx);
    }

    let content = invocation.content();
    button.label = parse_label(&content.lines, 0, diagnostics, &content.ctx);
    Directive::ButtonLink(Box::new(button))
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

directive_options! {
    /// The options `.. button-link::` takes: sphinx-design's nine.
    enum ButtonLinkOption {
        /// `:color:`.
        Color => (["color"], Choice(|| choice_names(SemanticColor::ALL, SemanticColor::as_str)), "The button's colour."),
        /// `:align:`.
        Align => (["align"], Choice(|| choice_names(TextAlign::ALL, TextAlign::as_str)), "Where the button sits on its line."),
        /// `:tooltip:`.
        Tooltip => (["tooltip"], Text, "Text shown when hovering the button."),
        /// `:class:`.
        Class => (["class"], Classes, "Classes for the button."),
        /// `:outline:`.
        Outline => (["outline"], Flag, "Draws the button as an outline."),
        /// `:expand:`.
        Expand => (["expand"], Flag, "Stretches the button across its line."),
        /// `:click-parent:`.
        ClickParent => (["click-parent"], Flag, "Makes the whole parent element clickable."),
        /// `:shadow:`.
        Shadow => (["shadow"], Flag, "Draws a shadow under the button."),
        // Accepted by sphinx-design here only because both button directives
        // share one option spec.
        _ => (["ref-type"], Text, "How a button-ref resolves its target; meaningless here.", "it selects how a `.. button-ref::` resolves its target, and this directive's argument is a URL"),
    }
}

/// Reads every option onto `button`, returning the lines nobody claimed.
fn read_options(
    button: &mut ButtonLink,
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    for (option, line) in
        classify_options::<ButtonLinkOption>(option_lines, DIRECTIVE, diagnostics, ctx)
    {
        match option {
            // `directives.flag` accepts no value at all, so what matters is
            // only that one of the four was written.
            ButtonLinkOption::Outline => {
                button.flags.insert(ButtonFlag::Outline);
            }
            ButtonLinkOption::Expand => {
                button.flags.insert(ButtonFlag::Expand);
            }
            ButtonLinkOption::ClickParent => {
                button.flags.insert(ButtonFlag::ClickParent);
            }
            ButtonLinkOption::Shadow => {
                button.flags.insert(ButtonFlag::Shadow);
            }
            ButtonLinkOption::Color => match SemanticColor::parse(&line.value) {
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
            ButtonLinkOption::Align => match TextAlign::parse(&line.value) {
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
            ButtonLinkOption::Tooltip => {
                if line.value.is_empty() {
                    report_empty_value(line, "tooltip", diagnostics, ctx);
                } else {
                    button.tooltip = Some(line.value.clone());
                }
            }
            ButtonLinkOption::Class => button.class = split_classes(&line.value),
        }
    }
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
