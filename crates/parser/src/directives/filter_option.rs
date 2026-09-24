//! Reading a filter, shared by every directive that takes one.
//!
//! Its own module rather than a helper inside any of them because they must
//! agree exactly: a `.. entity-table::`, an `.. entity-flow::` and an
//! `.. entity-pie::` asking the same question of the same project must select
//! the same entities, and must refuse the same expressions at the same column.
//! What differs between them is only which diagnostic code the failure is
//! reported under — a code names the construct — so that is what the caller
//! passes in.
//!
//! A filter is not always written on an option line. A pie chart's body holds
//! one per *content* line, so the work is split: [`read_filter_text`] takes a
//! filter and a [`FilterSite`] saying where it was written, and
//! [`read_filter_option`] is the thin caller that derives that site from an
//! [`OptionLine`]. Both report at the column the expression breaks at, which
//! is the whole reason a filter is parsed here rather than where it is
//! evaluated.
//!
//! Why the filter is parsed *here* at all, rather than by the renderer that
//! evaluates it: this is the last phase holding the option line's own
//! position, so it is the only one that can point at the character a broken
//! expression breaks at.

use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, EntityId, Span, UpdateTarget};
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

/// Where a filter was written, for its diagnostics to point at.
#[derive(Debug, Clone, Copy)]
pub(in crate::directives) struct FilterSite<'a> {
    /// The filter's line, indexed within the directive body.
    pub line_index: usize,
    /// The column the filter text starts at, when an exact one exists.
    ///
    /// `None` for a value joined from continuation lines: an offset into the
    /// joined text no longer names a column in any of them, so the whole line
    /// is reported instead. Reporting the line is right exactly where
    /// reporting a column would be wrong.
    pub column: Option<usize>,
    /// The whole line, for the span used when there is no exact column.
    pub raw: &'a str,
}

/// Which construct a filter diagnostic names, and under which codes.
#[derive(Debug, Clone, Copy)]
pub(in crate::directives) struct FilterOwner<'a> {
    /// The directive as the author spelled it.
    pub directive: &'a str,
    /// The option the filter was written on, or `None` for a body line.
    pub option: Option<&'a str>,
    /// The codes its two failures are reported under.
    pub codes: FilterCodes,
}

impl FilterOwner<'_> {
    /// How a diagnostic about this filter opens — `"needpie: :filter:"` for an
    /// option, `"needpie:"` for a body line, which has no option to name.
    fn prefix(&self) -> String {
        self.option.map_or_else(
            || format!("{}:", self.directive),
            |option| format!("{}: :{option}:", self.directive),
        )
    }
}

/// Reads a filter, reporting a broken expression at the column it breaks at
/// and every field the schema does not declare.
///
/// `None` means the expression could not be parsed. Every caller treats that
/// as "select everything" rather than "select nothing": the diagnostic already
/// says what is wrong, and an empty result on top of it hides which entities
/// the author was reaching for.
pub(in crate::directives) fn read_filter_text(
    text: &str,
    site: FilterSite<'_>,
    owner: FilterOwner<'_>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Expr> {
    match parse_filter(text) {
        Ok(expr) => {
            for name in expr.field_names() {
                if !ctx.schema.declares_field(name.as_str()) {
                    report_unknown_field_at(name, site, owner, diagnostics, ctx);
                }
            }
            Some(expr)
        }
        Err(error) => {
            diagnostics.push(Diagnostic::at(
                owner.codes.invalid,
                format!("{} {error}", owner.prefix()),
                filter_error_span(site, &error, ctx),
            ));
            None
        }
    }
}

/// Reads a `.. entity-update::`/`.. needextend::` argument, which — unlike
/// every other filter site — may equally name a single entity's id, a
/// decision only the merged project can make (see
/// `docs/decisions/019-entity-update.md`).
///
/// Both readings are computed independently and kept: `candidate_id` when the
/// trimmed text is a legal [`EntityId`] spelling, `filter` when it also parses
/// as a filter expression (a bare id like `REQ_001` does — it is
/// `Truthy(Field("REQ_001"))` — while one containing `:`/`-`/`.` usually does
/// not). Apply time prefers the id whenever it names an entity that actually
/// exists, exactly as upstream sphinx-needs disambiguates.
///
/// Reports [`DiagnosticCode::EntityUpdateInvalidArgument`] only when *neither*
/// reading survives — deliberately not an unknown-field diagnostic against
/// the filter reading even when one is available: which reading apply time
/// ends up using depends on the whole project, and warning about a filter
/// reading that turns out unused (because the argument was actually a good
/// id) would be a false positive on the common case.
pub(in crate::directives) fn read_update_argument(
    argument: &str,
    directive_span: Option<Span>,
    directive: &str,
    diagnostics: &mut Diagnostics,
) -> Option<UpdateTarget> {
    let raw = argument.trim();
    if raw.is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntityUpdateInvalidArgument,
            format!("{directive}: needs a target — an entity id or a filter"),
            directive_span,
        ));
        return None;
    }
    let candidate_id = EntityId::new(raw).ok();
    let filter = parse_filter(raw).ok();
    if candidate_id.is_none() && filter.is_none() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntityUpdateInvalidArgument,
            format!(
                "{directive}: '{raw}' is neither a legal entity id nor a filter this build can evaluate"
            ),
            directive_span,
        ));
        return None;
    }
    Some(UpdateTarget {
        candidate_id,
        filter,
        raw: raw.to_string(),
    })
}

/// Reads a `:filter:` option, deriving its site from the option line.
pub(in crate::directives) fn read_filter_option(
    line: &OptionLine,
    source_line: Option<&str>,
    directive: &str,
    codes: FilterCodes,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Expr> {
    read_filter_text(
        &line.value,
        option_site(line, source_line),
        FilterOwner {
            directive,
            option: Some("filter"),
            codes,
        },
        diagnostics,
        ctx,
    )
}

/// Where an option's value sits, and whether an exact column survives.
///
/// A joined value is recognised by comparing against the *source* line rather
/// than by looking at the value: continuations are appended to `raw` and
/// `value` alike, so `raw` still ends with `value` and only the original text
/// can tell the two cases apart.
fn option_site<'a>(line: &'a OptionLine, source_line: Option<&str>) -> FilterSite<'a> {
    let written_on_one_line = source_line.is_some_and(|source| source.trim() == line.raw);
    FilterSite {
        line_index: line.line_index,
        column: written_on_one_line.then(|| line.raw.chars().count() - line.value.chars().count()),
        raw: &line.raw,
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
    report_unknown_field_at(
        name,
        FilterSite {
            line_index: line.line_index,
            column: None,
            raw: &line.raw,
        },
        FilterOwner {
            directive,
            option: Some(&line.name),
            codes: FilterCodes {
                invalid: code,
                unknown_field: code,
            },
        },
        diagnostics,
        ctx,
    );
}

/// Reports an unknown field against wherever the filter naming it was written.
///
/// Always the whole line rather than the field's own characters: the name is
/// found by walking the parsed expression, which no longer records where each
/// operand sat.
fn report_unknown_field_at(
    name: &FieldName,
    site: FilterSite<'_>,
    owner: FilterOwner<'_>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        owner.codes.unknown_field,
        format!(
            "{} unknown field '{name}'; the schema declares {}",
            owner.prefix(),
            ctx.schema.field_names().join(", ")
        ),
        ctx.line_span(site.line_index, site.raw),
    ));
}

/// The span a filter error points at.
///
/// A filter with an exact column gets the exact characters that broke; one
/// without gets its whole line — see [`FilterSite::column`].
fn filter_error_span(
    site: FilterSite<'_>,
    error: &FilterError,
    ctx: &ParseCtx<'_>,
) -> Option<Span> {
    let Some(column) = site.column else {
        return ctx.line_span(site.line_index, site.raw);
    };
    let start = ctx.position(site.line_index, column + error.offset)?;
    let end = ctx.position(site.line_index, column + error.offset + error.length)?;
    Some(start.to(end))
}

#[cfg(test)]
mod update_argument_tests {
    use super::*;

    #[test]
    fn test_a_bare_id_is_read_as_both_a_candidate_id_and_a_filter() {
        // Given — `REQ_001` is a legal id and also parses as
        // `Truthy(Field("REQ_001"))`
        let mut diagnostics = Diagnostics::default();

        // When
        let target = read_update_argument("REQ_001", None, "entity-update", &mut diagnostics)
            .expect("a bare id should read as a target");

        // Then
        assert_eq!(target.candidate_id, EntityId::new("REQ_001").ok());
        assert!(target.filter.is_some());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_an_id_containing_a_colon_is_a_candidate_id_but_not_a_filter() {
        // Given — legal in an `EntityId` but not in this filter grammar
        let mut diagnostics = Diagnostics::default();

        // When
        let target = read_update_argument("REQ:001", None, "entity-update", &mut diagnostics)
            .expect("a legal id spelling should still read as a target");

        // Then
        assert_eq!(target.candidate_id, EntityId::new("REQ:001").ok());
        assert_eq!(target.filter, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_a_filter_expression_is_read_with_no_candidate_id() {
        // Given — spaces and quotes are not legal in an `EntityId`
        let mut diagnostics = Diagnostics::default();

        // When
        let target =
            read_update_argument(r#"type == "req""#, None, "entity-update", &mut diagnostics)
                .expect("a valid filter should read as a target");

        // Then
        assert_eq!(target.candidate_id, None);
        assert!(target.filter.is_some());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_an_empty_argument_is_reported_and_reads_as_no_target() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let target = read_update_argument("   ", None, "entity-update", &mut diagnostics);

        // Then
        assert_eq!(target, None);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::EntityUpdateInvalidArgument
        );
    }

    #[test]
    fn test_an_argument_that_is_neither_reading_is_reported() {
        // Given — a space makes this neither a legal id nor a legal filter
        let mut diagnostics = Diagnostics::default();

        // When
        let target = read_update_argument("not & valid", None, "entity-update", &mut diagnostics);

        // Then
        assert_eq!(target, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::EntityUpdateInvalidArgument
        );
    }

    #[test]
    fn test_a_bare_id_argument_reports_no_unknown_field() {
        // Given — the whole point of deferring the unknown-field check to
        // apply time: a good id should never be second-guessed as a filter
        let mut diagnostics = Diagnostics::default();

        // When
        read_update_argument("REQ_001", None, "entity-update", &mut diagnostics);

        // Then — no unknown-field diagnostic, even though `REQ_001` also
        // parses as a filter over a field this build's schema never declares
        assert!(diagnostics.is_empty());
    }
}
