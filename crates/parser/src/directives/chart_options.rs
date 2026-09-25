//! The options every chart over the entity graph shares, read one way.
//!
//! `.. entity-pie::` and `.. entity-bar::` are separate directives — ADR-017
//! explains why a `:type:` switch was rejected — but they are placed on a page
//! the same way and coloured the same way, so the options that do that are
//! read here once. What differs between them is only which diagnostic code a
//! failure is reported under, since a code names the construct; that is what
//! [`ChartCodes`] carries, exactly as `filter_option.rs`'s `FilterCodes` does
//! for a filter.
//!
//! The *figure* options — `:caption:`, `:align:`, `:scale:`, `:width:`,
//! `:class:` and `:name:` — are handed back as a [`FigureOption`] rather than
//! written into a node, because the two nodes are different types; each
//! caller stores the value where its own node keeps it.

use rinx_ast::{
    ChartColor, Diagnostic, DiagnosticCode, ImageAlign, LengthOrPercentage, TargetName,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

use super::options::{OptionLine, parse_percentage};

/// Which codes a chart reports the shared options' failures under.
#[derive(Debug, Clone, Copy)]
pub(in crate::directives) struct ChartCodes {
    /// A colour this build cannot draw with.
    pub invalid_color: DiagnosticCode,
    /// An `:align:` that is not one of docutils' three.
    pub invalid_align: DiagnosticCode,
    /// A `:scale:` that is not a percentage.
    pub invalid_scale: DiagnosticCode,
    /// A `:width:` that is not a length or a percentage.
    pub invalid_width: DiagnosticCode,
    /// A `:scale:` with no `:width:` to apply to.
    pub unusable_scale: DiagnosticCode,
    /// A required value left empty.
    pub empty_option_value: DiagnosticCode,
    /// A sphinx-needs option this build refuses by name.
    pub unsupported_option: DiagnosticCode,
}

/// Which chart a diagnostic names, and under which codes.
#[derive(Debug, Clone, Copy)]
pub(in crate::directives) struct ChartOwner<'a> {
    /// The directive as the author spelled it.
    pub directive: &'a str,
    /// The codes its failures are reported under.
    pub codes: ChartCodes,
}

/// One figure option's value, for the caller to store on its own node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::directives) enum FigureOption {
    Caption(String),
    Align(ImageAlign),
    Scale(u32),
    Width(LengthOrPercentage),
    Classes(Vec<String>),
    Name(TargetName),
}

/// What [`read_figure_option`] made of an option line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::directives) enum FigureRead {
    /// Not a figure option: the caller should try its own.
    Unclaimed,
    /// A figure option whose value was refused and reported.
    Reported,
    /// A figure option, read.
    Read(FigureOption),
}

/// Reads `line` if it is one of the figure options, reporting a bad value
/// against the line it was written on.
pub(in crate::directives) fn read_figure_option(
    line: &OptionLine,
    owner: ChartOwner<'_>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> FigureRead {
    let directive = owner.directive;
    let refuse = |code, message: String, diagnostics: &mut Diagnostics| {
        diagnostics.push(Diagnostic::at(
            code,
            message,
            ctx.line_span(line.line_index, &line.raw),
        ));
        FigureRead::Reported
    };
    match line.name.as_str() {
        "caption" => FigureRead::Read(FigureOption::Caption(line.value.clone())),
        "align" => match ImageAlign::parse(&line.value) {
            Some(align) => FigureRead::Read(FigureOption::Align(align)),
            None => refuse(
                owner.codes.invalid_align,
                format!(
                    "{directive}: :align: expects one of center, left, right, found '{}'",
                    line.value
                ),
                diagnostics,
            ),
        },
        "scale" => match parse_percentage(&line.value) {
            Some(scale) => FigureRead::Read(FigureOption::Scale(scale)),
            None => refuse(
                owner.codes.invalid_scale,
                format!(
                    "{directive}: :scale: expects a non-negative percentage, found '{}'",
                    line.value
                ),
                diagnostics,
            ),
        },
        "width" => match LengthOrPercentage::new(&line.value) {
            Ok(width) => FigureRead::Read(FigureOption::Width(width)),
            Err(problem) => refuse(
                owner.codes.invalid_width,
                format!("{directive}: :width: {problem}"),
                diagnostics,
            ),
        },
        "class" => FigureRead::Read(FigureOption::Classes(
            line.value.split_whitespace().map(str::to_string).collect(),
        )),
        "name" => {
            if line.value.is_empty() {
                report_empty_value(line, owner, diagnostics, ctx);
                FigureRead::Reported
            } else {
                FigureRead::Read(FigureOption::Name(TargetName::new(&line.value)))
            }
        }
        _ => FigureRead::Unclaimed,
    }
}

/// Reads `:colors:` — one colour per wedge or series, in order.
///
/// An entry this build cannot draw with is dropped and reported while the rest
/// are kept, which is `:relations:`'s rule rather than `:columns:`'s: losing
/// every colour over one misspelling would change a chart the author can see
/// into one they cannot recognise. A dropped entry shifts the rest, so the
/// diagnostic matters — it is the only sign the colours are no longer the ones
/// that were written.
pub(in crate::directives) fn read_colors(
    line: &OptionLine,
    owner: ChartOwner<'_>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<ChartColor> {
    line.value
        .split(',')
        .map(str::trim)
        .filter(|written| !written.is_empty())
        .filter_map(|written| read_color(written, line, owner, diagnostics, ctx))
        .collect()
}

/// Reads one colour, reporting it against the option line it was written on.
pub(in crate::directives) fn read_color(
    written: &str,
    line: &OptionLine,
    owner: ChartOwner<'_>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<ChartColor> {
    match ChartColor::parse(written) {
        Ok(color) => Some(color),
        Err(problem) => {
            diagnostics.push(Diagnostic::at(
                owner.codes.invalid_color,
                format!("{}: :{}: {problem}", owner.directive, line.name),
                ctx.line_span(line.line_index, &line.raw),
            ));
            None
        }
    }
}

/// What to tell an author who wrote an option `refused` lists, or `None` when
/// the name is not in it.
///
/// A table rather than a match arm apiece: the option is being refused, so the
/// sentence beside it is the entire feature.
pub(in crate::directives) fn unsupported_advice(
    refused: &[(&str, &'static str)],
    name: &str,
) -> Option<&'static str> {
    refused
        .iter()
        .find(|(option, _)| *option == name)
        .map(|(_, advice)| *advice)
}

/// Reports an option this build does not implement, and what to write instead.
pub(in crate::directives) fn report_unsupported(
    line: &OptionLine,
    advice: &str,
    owner: ChartOwner<'_>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        owner.codes.unsupported_option,
        format!(
            "{}: :{}: is not supported, so it was ignored — {advice}",
            owner.directive, line.name
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports an option whose value is required but was left empty.
pub(in crate::directives) fn report_empty_value(
    line: &OptionLine,
    owner: ChartOwner<'_>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        owner.codes.empty_option_value,
        format!(
            "{}: :{}: needs a value, so the option was ignored",
            owner.directive, line.name
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports a `:scale:` that has no `:width:` to apply to.
///
/// Pointed at the `:scale:` line rather than at the directive, since that is
/// the line the author would have to change — the choice `.. image::` and
/// every other picture directive already make for their own version of this.
pub(in crate::directives) fn report_unusable_scale(
    option_lines: &[OptionLine],
    owner: ChartOwner<'_>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let span = option_lines
        .iter()
        .rev()
        .find(|line| line.name == "scale")
        .and_then(|line| ctx.line_span(line.line_index, &line.raw));
    diagnostics.push(Diagnostic::at(
        owner.codes.unusable_scale,
        format!(
            "{}: :scale: has no :width: to apply to, so it was ignored — the chart is \
             drawn at a size this build chooses, so it has no size of its own to scale",
            owner.directive
        ),
        span,
    ));
}

#[cfg(test)]
mod tests;
