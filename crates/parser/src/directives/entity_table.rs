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

use crate::directives::invocation::{DirectiveInvocation, ParseState, single, unknown};
use crate::directives::option_value::OptionValueType;
use crate::directives::spec::OptionValue;
use crate::directives::spec::{Arguments, DirectiveSpec};
use rinx_ast::{
    Diagnostic, DiagnosticCode, Directive, EntityTable, EntityTableSource, TableWidths,
};
use rinx_filter::FieldName;

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

use super::filter_option::{
    FilterCodes, ShorthandFilterOption, read_filter_option, report_unknown_field,
};
use super::option_value::Located;
use super::options::{DirectiveOptions, OptionLine, classify_options, directive_options};
use super::table_options::{CommonTableOptions, TableOption, read_table_option};
use super::table_widths::widths_for_columns;

/// The only `:style:` this build renders.
///
/// sphinx-needs also offers `datatables`, which is a JavaScript grid. Naming
/// it is reported rather than ignored: the author asked for sorting and
/// filtering in the browser and would otherwise get a static table with no
/// indication that they did not.
const SUPPORTED_STYLE: &str = "table";

/// The advice for sphinx-needs' other ways of selecting entities, which a
/// `:filter:` says in this build's own language.
const WRITE_AS_FILTER: &str = "write the selection as :filter:";

directive_options! {
    /// The options `.. entity-table::` takes: sphinx-needs' `needtable`'s
    /// nineteen, of which this build reads five, and the presentation options
    /// every table takes.
    enum EntityTableOption {
        /// `:filter:`.
        Filter => (["filter"], Filter, "The entities listed, by a filter."),
        /// `:columns:`.
        Columns => (["columns"], Text, "The fields shown as columns, comma-separated."),
        /// `:sort:`.
        Sort => (["sort"], Text, "The field the rows are sorted by."),
        /// `:colwidths:`.
        Colwidths(Located<TableWidths>) => (["colwidths"], TableDataWidthsInvalid, "sphinx-needs' spelling of `:widths:`."),
        /// `:style:`.
        Style(StaticStyle) => (["style"], EntityTableUnsupportedStyle, "How the table is drawn: only `table` is supported."),
        _ => (["filter-func"], Text, "The entities listed, by a Python function.", WRITE_AS_FILTER),
        _ => (["filter_warning"], Text, "The text shown when nothing is listed."),
        _ => (["cypher"], Text, "The entities listed, by a Cypher query.", WRITE_AS_FILTER),
        _ => (["export_id"], Text, "The id the filter is exported under."),
        _ => (["max_items"], Integer, "How many rows are listed at most."),
        _ => (["page_size"], Integer, "How many rows a page of a `datatables` table shows."),
        _ => (["show_filters"], Flag, "Shows the filter above the table."),
        _ => (["show_parts"], Flag, "Lists each entity's parts as rows of their own."),
        _ => (["sort_by"], Text, "The field the rows are sorted by.", "write it as :sort:"),
        _ => (["style_row"], Text, "A class for each row."),
        _ => (["style_col"], Text, "A class for each column.");
        Shorthand(ShorthandFilterOption),
        Table(TableOption),
    }
}

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["entity-table", "needtable"],
    doc: "A table of the entities a filter selects.",
    arguments: Arguments::NONE,
    takes_content: false,
    content_hint: Some("write the selection as :filter:"),
    options: <EntityTableOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

/// Interprets `.. entity-table::` or `.. needtable::`, recording which was
/// written.
pub(in crate::directives) fn interpret_entity_table(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    let Ok(source) = input.name.parse::<rinx_ast::EntityTableSource>() else {
        return unknown(input, state);
    };
    single(parse_entity_table(source, input, state.diagnostics))
}

/// Parses a `.. entity-table::` / `.. needtable::` into a
/// [`Directive::EntityTable`].
pub(super) fn parse_entity_table(
    source: EntityTableSource,
    invocation: &DirectiveInvocation<'_, '_>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let directive = source.as_str();
    let directive_span = invocation.span;
    let ctx = &invocation.ctx;
    let unindented_lines = &invocation.lines;
    let option_lines = &invocation.options;
    let mut common = CommonTableOptions::empty();
    let mut table = EntityTable {
        span: directive_span,
        ..EntityTable::new(source)
    };

    let mut colwidths = None;
    for (option, line) in
        classify_options::<EntityTableOption>(option_lines, directive, diagnostics, ctx)
    {
        match option {
            EntityTableOption::Filter => {
                let source_line = unindented_lines.get(line.line_index).map(String::as_str);
                table.filter = read_filter(line, source_line, directive, diagnostics, ctx);
            }
            EntityTableOption::Columns => {
                if let Some(columns) = read_columns(line, directive, diagnostics, ctx) {
                    table.columns = columns;
                }
            }
            EntityTableOption::Sort => table.sort = read_sort(line, directive, diagnostics, ctx),
            EntityTableOption::Colwidths(widths) => colwidths = Some(widths),
            // Only `table` reads, and it is what is drawn anyway.
            EntityTableOption::Style(StaticStyle) => {}
            EntityTableOption::Shorthand(option) => match option {},
            EntityTableOption::Table(option) => {
                read_table_option(option, line, &mut common, directive, diagnostics, ctx);
            }
        }
    }
    table.width = common.width;
    table.align = common.align;
    table.classes = common.classes;
    table.name = common.name;

    table.widths = read_widths(
        common.widths,
        colwidths,
        table.columns.len(),
        directive,
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
) -> Option<rinx_filter::Expr> {
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
            DiagnosticCode::EntityTableInvalidValue,
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
            DiagnosticCode::EntityTableInvalidValue,
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
    widths: Option<Located<TableWidths>>,
    colwidths: Option<Located<TableWidths>>,
    columns: usize,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<TableWidths> {
    let widths = match (widths, colwidths) {
        (Some(_), Some(colwidths)) => {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityTableDuplicateWidths,
                format!(
                    "{directive}: :widths: and :colwidths: are two spellings of one option; give only one"
                ),
                colwidths.span(ctx),
            ));
            return None;
        }
        (Some(widths), None) | (None, Some(widths)) => widths,
        (None, None) => return None,
    };
    widths_for_columns(widths, columns, directive, diagnostics, ctx)
}

/// A `:style:`: only [`SUPPORTED_STYLE`] reads, the static table this build
/// draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StaticStyle;

impl OptionValueType for StaticStyle {
    const VALUE: OptionValue<'static> = OptionValue::Choice(|| vec![SUPPORTED_STYLE]);

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        let written = line.value.trim();
        if written == SUPPORTED_STYLE {
            Ok(Self)
        } else {
            Err(format!(
                "'{written}' is not supported; a static '{SUPPORTED_STYLE}' is rendered instead"
            ))
        }
    }
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
