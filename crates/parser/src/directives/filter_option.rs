//! Reading a `:filter:` option, shared by the two directives that take one.
//!
//! Its own module rather than a helper inside either of them because the two
//! must agree exactly: a `.. entity-table::` and an `.. entity-flow::` asking
//! the same question of the same project must select the same entities, and
//! must refuse the same expressions at the same column. What differs between
//! them is only which diagnostic code the failure is reported under — a code
//! names the construct — so that is what the caller passes in.
//!
//! Why the filter is parsed *here* at all, rather than by the renderer that
//! evaluates it: this is the last phase holding the option line's own
//! position, so it is the only one that can point at the character a broken
//! expression breaks at.

use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, Span};
use rusty_sphinx_filter::{Expr, FieldName, FilterError, parse_filter};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

use super::options::OptionLine;

/// Which codes a caller reports a filter's two failures under.
#[derive(Debug, Clone, Copy)]
pub(in crate::directives) struct FilterCodes {
    /// An expression the filter language cannot parse.
    pub invalid: DiagnosticCode,
    /// A field name no entity type declares.
    pub unknown_field: DiagnosticCode,
}

/// Reads a `:filter:` value, reporting a broken expression at the column it
/// breaks at and every field the schema does not declare.
///
/// `None` means the expression could not be parsed. Every caller treats that
/// as "select everything" rather than "select nothing": the diagnostic already
/// says what is wrong, and an empty result on top of it hides which entities
/// the author was reaching for.
pub(in crate::directives) fn read_filter_option(
    line: &OptionLine,
    source_line: Option<&str>,
    directive: &str,
    codes: FilterCodes,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Expr> {
    match parse_filter(&line.value) {
        Ok(expr) => {
            for name in expr.field_names() {
                if !ctx.schema.declares_field(name.as_str()) {
                    report_unknown_field(
                        name,
                        line,
                        directive,
                        codes.unknown_field,
                        diagnostics,
                        ctx,
                    );
                }
            }
            Some(expr)
        }
        Err(error) => {
            diagnostics.push(Diagnostic::at(
                codes.invalid,
                format!("{directive}: :filter: {error}"),
                filter_error_span(line, source_line, &error, ctx),
            ));
            None
        }
    }
}

/// Reports a field no entity type declares, offering the whole vocabulary.
///
/// Shared with the options that name a *single* field — a table's `:columns:`
/// and `:sort:` — which is why the option's own name comes off `line` rather
/// than being fixed to `filter`.
pub(in crate::directives) fn report_unknown_field(
    name: &FieldName,
    line: &OptionLine,
    directive: &str,
    code: DiagnosticCode,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        code,
        format!(
            "{directive}: :{}: unknown field '{name}'; the schema declares {}",
            line.name,
            ctx.schema.field_names().join(", ")
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// The span a filter error points at.
///
/// A filter written on one line gets the exact characters that broke; one
/// wrapped across several gets the whole first line, because the continuation
/// lines were joined with spaces and an offset into the joined text no longer
/// names a column in any of them. Reporting the line is right where reporting
/// a column would be wrong.
///
/// A joined value is recognised by comparing against the *source* line rather
/// than by looking at the value: continuations are appended to `raw` and
/// `value` alike, so `raw` still ends with `value` and only the original text
/// can tell the two cases apart.
fn filter_error_span(
    line: &OptionLine,
    source_line: Option<&str>,
    error: &FilterError,
    ctx: &ParseCtx<'_>,
) -> Option<Span> {
    let written_on_one_line = source_line.is_some_and(|source| source.trim() == line.raw);
    if !written_on_one_line {
        return ctx.line_span(line.line_index, &line.raw);
    }
    let value_column = line.raw.chars().count() - line.value.chars().count();
    let start = ctx.position(line.line_index, value_column + error.offset)?;
    let end = ctx.position(line.line_index, value_column + error.offset + error.length)?;
    Some(start.to(end))
}
