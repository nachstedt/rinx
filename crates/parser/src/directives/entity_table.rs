//! `.. entity-table::`, and its sphinx-needs spelling `.. needtable::` — a
//! table of the entities matching a filter.
//!
//! Unlike every other table directive, this one's *content* is not in the
//! document: it names a question, and the rows are resolved against the
//! project index while rendering. What happens here is everything that can
//! only happen here — parsing the filter expression, and checking every field
//! it names against the entity schema, both while the option line's own
//! position is still in hand.
//!
//! An unreadable option is dropped and reported, leaving the table itself
//! intact, which is the error-resilience every directive here follows.

use rusty_sphinx_ast::{
    Diagnostic, DiagnosticCode, Directive, EntityTable, EntityTableSource, Span,
};
use rusty_sphinx_filter::FieldName;

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::indent::unindent_body_lines;

use super::filter_option::{FilterCodes, read_filter_option, report_unknown_field};
use super::options::{OptionLine, report_unknown_options, scan_option_lines};
use super::table_options::parse_common_table_options;
use super::table_widths::parse_widths_option;

/// The only `:style:` this build renders.
///
/// sphinx-needs also offers `datatables`, which is a JavaScript grid. Naming
/// it is reported rather than ignored: the author asked for sorting and
/// filtering in the browser and would otherwise get a static table with no
/// indication that they did not.
const SUPPORTED_STYLE: &str = "table";

/// Parses a `.. entity-table::` / `.. needtable::` into a
/// [`Directive::EntityTable`].
pub(super) fn parse_entity_table(
    source: EntityTableSource,
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let directive = source.as_str();
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, _) = scan_option_lines(&unindented_lines);
    let refs: Vec<&OptionLine> = option_lines.iter().collect();

    let (common, remaining) = parse_common_table_options(&refs, directive, diagnostics, ctx);

    let mut table = EntityTable {
        source,
        span: directive_span,
        width: common.width,
        align: common.align,
        classes: common.classes,
        name: common.name,
        ..EntityTable::new(source)
    };

    if !argument.trim().is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::DirectiveEntityTableUnknownOption,
            format!(
                "{directive}: takes no argument; write the selection as :filter: instead of '{}'",
                argument.trim()
            ),
            directive_span,
        ));
    }

    let mut colwidths = None;
    let mut unrecognized = Vec::new();
    for line in remaining {
        match line.name.as_str() {
            "filter" => {
                let source_line = unindented_lines.get(line.line_index).map(String::as_str);
                table.filter = read_filter(line, source_line, directive, diagnostics, ctx);
            }
            "columns" => {
                if let Some(columns) = read_columns(line, directive, diagnostics, ctx) {
                    table.columns = columns;
                }
            }
            "sort" => table.sort = read_sort(line, directive, diagnostics, ctx),
            "colwidths" => colwidths = Some(line),
            "style" => check_style(line, directive, diagnostics, ctx),
            _ => unrecognized.push(line),
        }
    }

    table.widths = read_widths(
        common.widths_raw.as_deref(),
        colwidths,
        table.columns.len(),
        directive,
        diagnostics,
        ctx,
    );

    report_unknown_options(
        &unrecognized,
        directive,
        DiagnosticCode::DirectiveEntityTableUnknownOption,
        diagnostics,
        ctx,
    );

    Directive::EntityTable(Box::new(table))
}

/// Reads `:filter:` under this directive's own diagnostic codes.
///
/// The reading itself is [`read_filter_option`], shared with
/// `.. entity-flow::` so the two cannot disagree about which entities an
/// expression selects. A table whose filter could not be parsed lists
/// *everything* rather than nothing: the diagnostic already says what is
/// wrong, and an empty table on top of it would hide which entities the author
/// was reaching for.
fn read_filter(
    line: &OptionLine,
    source_line: Option<&str>,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<rusty_sphinx_filter::Expr> {
    read_filter_option(line, source_line, directive, FILTER_CODES, diagnostics, ctx)
}

/// The codes a table reports its filter's failures under.
const FILTER_CODES: FilterCodes = FilterCodes {
    invalid: DiagnosticCode::EntityTableInvalidFilter,
    unknown_field: DiagnosticCode::EntityTableUnknownField,
};

/// Reads `:columns:`, dropping the whole option if any entry is unusable.
///
/// All-or-nothing deliberately: a table silently missing one column is harder
/// to notice than one showing the default set, and the diagnostic names the
/// column either way.
fn read_columns(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Vec<FieldName>> {
    let mut columns = Vec::new();
    let mut usable = true;
    for written in split_list(&line.value) {
        match read_field(&written, line, directive, diagnostics, ctx) {
            Some(name) => columns.push(name),
            None => usable = false,
        }
    }
    if columns.is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::DirectiveEntityTableUnknownOption,
            format!("{directive}: :columns: names no column"),
            ctx.line_span(line.line_index, &line.raw),
        ));
        return None;
    }
    usable.then_some(columns)
}

/// Reads `:sort:`, which names exactly one field.
fn read_sort(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<FieldName> {
    let written = line.value.trim();
    if written.contains(',') {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::DirectiveEntityTableUnknownOption,
            format!("{directive}: :sort: takes one field, not a list: '{written}'"),
            ctx.line_span(line.line_index, &line.raw),
        ));
        return None;
    }
    read_field(written, line, directive, diagnostics, ctx)
}

/// Turns one written name into a [`FieldName`] the schema recognises.
fn read_field(
    written: &str,
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<FieldName> {
    let name = match FieldName::new(written) {
        Ok(name) => name,
        Err(error) => {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityTableUnknownField,
                format!("{directive}: :{}: {error}", line.name),
                ctx.line_span(line.line_index, &line.raw),
            ));
            return None;
        }
    };
    if !ctx.schema.declares_field(name.as_str()) {
        report_unknown_field(
            &name,
            line,
            directive,
            DiagnosticCode::EntityTableUnknownField,
            diagnostics,
            ctx,
        );
        return None;
    }
    Some(name)
}

/// Resolves `:widths:` and sphinx-needs' `:colwidths:` spelling of it.
///
/// The column count is already known here, unlike in the data tables, because
/// `:columns:` fixes it before any row exists.
fn read_widths(
    widths_raw: Option<&str>,
    colwidths: Option<&OptionLine>,
    columns: usize,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<rusty_sphinx_ast::TableWidths> {
    let raw = match (widths_raw, colwidths) {
        (Some(_), Some(line)) => {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityTableDuplicateWidths,
                format!(
                    "{directive}: :widths: and :colwidths: are two spellings of one option; give only one"
                ),
                ctx.line_span(line.line_index, &line.raw),
            ));
            return None;
        }
        (Some(raw), None) => raw.to_string(),
        (None, Some(line)) => line.value.clone(),
        (None, None) => return None,
    };

    let span = colwidths.and_then(|line| ctx.line_span(line.line_index, &line.raw));
    parse_widths_option(&raw, columns, directive, diagnostics, span)
}

/// Reports a `:style:` this build cannot render.
fn check_style(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let written = line.value.trim();
    if written == SUPPORTED_STYLE {
        return;
    }
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::EntityTableUnsupportedStyle,
        format!(
            "{directive}: :style: '{written}' is not supported; a static '{SUPPORTED_STYLE}' is rendered instead"
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Splits a comma-separated option value, dropping empty entries.
fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests;
