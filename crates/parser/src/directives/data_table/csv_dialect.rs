//! Translating `.. csv-table::`'s dialect options into a CSV reader, and
//! running it.
//!
//! docutils builds a Python `csv.Dialect` from `:delim:`, `:quote:`,
//! `:escape:` and `:keepspace:`; this module builds the `csv` crate's
//! equivalent. Two places where the two cannot be made identical are called
//! out at their implementations below — non-ASCII delimiters, and the exact
//! reach of `skipinitialspace`.

use crate::diagnostics::Diagnostics;
use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, Span, TableSource};

use crate::context::ParseCtx;
use crate::directives::options::OptionLine;

/// The `.. csv-table::` options that shape how the data is tokenized.
///
/// Kept apart from `SharedTableOptions` (rather than widening it) because
/// none of these mean anything to a `.. list-table::`.
pub(super) struct CsvDialect {
    pub delimiter: u8,
    pub quote: u8,
    pub escape: Option<u8>,
    /// `:keepspace:` — when false (the default), leading whitespace is
    /// stripped from each field, matching docutils' `skipinitialspace`.
    pub keepspace: bool,
}

impl Default for CsvDialect {
    fn default() -> Self {
        Self {
            delimiter: b',',
            quote: b'"',
            escape: None,
            keepspace: false,
        }
    }
}

impl CsvDialect {
    /// Applies one `:delim:`/`:quote:`/`:escape:`/`:keepspace:` option line,
    /// reporting `true` if it recognized the option's name.
    pub(super) fn apply_option(
        &mut self,
        line: &OptionLine,
        diagnostics: &mut Diagnostics,
        ctx: &ParseCtx<'_>,
    ) -> bool {
        let span = ctx.line_span(line.line_index, &line.raw);
        match line.name.as_str() {
            "delim" => {
                if let Some(byte) =
                    parse_single_char_option(&line.value, "delim", diagnostics, span)
                {
                    self.delimiter = byte;
                }
            }
            "quote" => {
                if let Some(byte) =
                    parse_single_char_option(&line.value, "quote", diagnostics, span)
                {
                    self.quote = byte;
                }
            }
            "escape" => {
                if let Some(byte) =
                    parse_single_char_option(&line.value, "escape", diagnostics, span)
                {
                    self.escape = Some(byte);
                }
            }
            // A flag option: docutils' `:keepspace:` takes no value, and its
            // mere presence turns `skipinitialspace` off.
            "keepspace" => self.keepspace = true,
            _ => return false,
        }
        true
    }
}

/// Parses a `:delim:`/`:quote:`/`:escape:` value into the single byte the CSV
/// reader wants, accepting every spelling docutils'
/// `single_char_or_whitespace_or_unicode` does: a literal character, the words
/// `space` and `tab`, and the `0x20` / `x20` / `\x20` / `u+0020` code-point
/// forms.
///
/// **Deliberate deviation from docutils:** the `csv` crate's reader is
/// byte-oriented, so a delimiter, quote or escape character outside ASCII is
/// rejected with a diagnostic instead of silently mis-splitting a multi-byte
/// character. docutils accepts any Unicode character here. Do not "fix" this
/// by truncating the character to its first UTF-8 byte — that would split
/// fields in the middle of unrelated characters.
pub(super) fn parse_single_char_option(
    raw: &str,
    option_name: &str,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> Option<u8> {
    let directive = TableSource::Csv.as_str();
    let Some(character) = resolve_single_char(raw) else {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::CsvDialectInvalidChar,
            format!(
                "{directive}: :{option_name}: value '{raw}' is not a single character, \
                 'space', 'tab', or a character code such as '0x20'"
            ),
            span,
        ));
        return None;
    };
    if !character.is_ascii() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::CsvDialectNonAscii,
            format!(
                "{directive}: :{option_name}: value '{raw}' is not supported — only ASCII \
                 delimiter, quote and escape characters can be used"
            ),
            span,
        ));
        return None;
    }
    let mut buffer = [0u8; 4];
    Some(character.encode_utf8(&mut buffer).as_bytes()[0])
}

/// Resolves the option value to the character it names, with no ASCII check.
fn resolve_single_char(raw: &str) -> Option<char> {
    let mut chars = raw.chars();
    match (chars.next(), chars.next()) {
        (Some(only), None) => return Some(only),
        (None, _) => return None,
        _ => {}
    }
    match raw.to_ascii_lowercase().as_str() {
        "space" => return Some(' '),
        "tab" => return Some('\t'),
        _ => {}
    }
    let digits = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .or_else(|| raw.strip_prefix("\\x"))
        .or_else(|| raw.strip_prefix("\\X"))
        .or_else(|| raw.strip_prefix("u+"))
        .or_else(|| raw.strip_prefix("U+"))
        .or_else(|| raw.strip_prefix('x'))
        .or_else(|| raw.strip_prefix('X'))?;
    let code = u32::from_str_radix(digits, 16).ok()?;
    char::from_u32(code)
}

/// Splits CSV text into rows of field strings under `dialect`.
///
/// # Errors
///
/// Returns the reader's own message when the data is malformed (an
/// unterminated quoted field, say), for the caller to surface as a
/// diagnostic.
pub(super) fn parse_csv_rows(data: &str, dialect: &CsvDialect) -> Result<Vec<Vec<String>>, String> {
    if has_unterminated_quote(data, dialect) {
        return Err("unterminated quoted field".to_string());
    }

    let mut builder = ::csv::ReaderBuilder::new();
    builder
        .delimiter(dialect.delimiter)
        .quote(dialect.quote)
        .escape(dialect.escape)
        .double_quote(true)
        // Ragged rows are the caller's business, not an error: docutils pads
        // short rows out to the widest one.
        .flexible(true)
        // Every row is data; a csv-table's header comes from `:header:` or
        // `:header-rows:`, never from the reader's notion of a header row.
        .has_headers(false)
        // A `#` starting a field is ordinary text in RST, not a comment.
        .comment(None);

    let mut reader = builder.from_reader(data.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        rows.push(
            record
                .iter()
                .map(|field| trim_leading_space(field, dialect.keepspace))
                .collect(),
        );
    }
    Ok(rows)
}

/// Whether the data ends inside a quoted field.
///
/// docutils reads CSV with Python's `strict=True`, which raises on malformed
/// quoting; the `csv` crate has no equivalent setting and instead recovers
/// silently, which would turn a typo into a table quietly swallowing the rest
/// of the document's data. This scan restores a diagnostic for the one
/// malformed case that actually costs the author data — a quote that is never
/// closed. The subtler `strict` complaints (a stray quote *inside* an unquoted
/// field, say) stay permissive, matching the crate.
fn has_unterminated_quote(data: &str, dialect: &CsvDialect) -> bool {
    let mut in_quotes = false;
    let mut bytes = data.as_bytes().iter().copied().peekable();
    while let Some(byte) = bytes.next() {
        if in_quotes && dialect.escape == Some(byte) {
            bytes.next();
        } else if byte == dialect.quote {
            // A doubled quote inside a quoted field is an escaped quote, not
            // the field's end.
            if in_quotes && bytes.peek() == Some(&dialect.quote) {
                bytes.next();
            } else {
                in_quotes = !in_quotes;
            }
        }
    }
    in_quotes
}

/// **Deliberate deviation from docutils:** Python's `skipinitialspace` strips
/// leading whitespace only where it sits between the delimiter and an
/// *unquoted* field, whereas by the time the `csv` crate hands a field back,
/// whether it was quoted is no longer visible. So the trim is applied to
/// every field. The observable outcome differs only for a quoted field
/// deliberately opening with a space, which the author can preserve with
/// `:keepspace:`. Do not "fix" this by switching to the crate's
/// `Trim::Fields`, which would also strip *trailing* whitespace that docutils
/// keeps.
fn trim_leading_space(field: &str, keepspace: bool) -> String {
    if keepspace {
        field.to_string()
    } else {
        field.trim_start_matches([' ', '\t']).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::Domain;

    fn option(name: &str, value: &str) -> OptionLine {
        OptionLine {
            name: name.to_string(),
            value: value.to_string(),
            raw: format!(":{name}: {value}"),
            line_index: 0,
        }
    }

    fn rows(data: &str, dialect: &CsvDialect) -> Vec<Vec<String>> {
        parse_csv_rows(data, dialect).expect("well-formed CSV")
    }

    #[test]
    fn test_default_dialect_is_comma_separated_and_double_quoted() {
        // Given / When
        let dialect = CsvDialect::default();

        // Then
        assert_eq!(dialect.delimiter, b',');
        assert_eq!(dialect.quote, b'"');
        assert_eq!(dialect.escape, None);
        assert!(!dialect.keepspace);
    }

    #[test]
    fn test_apply_option_reports_an_unrelated_option_as_unclaimed() {
        // Given
        let mut dialect = CsvDialect::default();
        let mut diagnostics = Diagnostics::default();

        // When
        let claimed = dialect.apply_option(
            &option("widths", "auto"),
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert!(!claimed);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_apply_option_sets_the_delimiter() {
        // Given
        let mut dialect = CsvDialect::default();
        let mut diagnostics = Diagnostics::default();

        // When
        let claimed = dialect.apply_option(
            &option("delim", ";"),
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert!(claimed);
        assert!(diagnostics.is_empty());
        assert_eq!(dialect.delimiter, b';');
    }

    #[test]
    fn test_apply_option_sets_the_quote_and_escape_characters() {
        // Given
        let mut dialect = CsvDialect::default();
        let mut diagnostics = Diagnostics::default();

        // When
        dialect.apply_option(
            &option("quote", "'"),
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        dialect.apply_option(
            &option("escape", "\\"),
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(dialect.quote, b'\'');
        assert_eq!(dialect.escape, Some(b'\\'));
    }

    #[test]
    fn test_apply_option_treats_keepspace_as_a_flag() {
        // Given
        let mut dialect = CsvDialect::default();
        let mut diagnostics = Diagnostics::default();

        // When
        let claimed = dialect.apply_option(
            &option("keepspace", ""),
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert!(claimed);
        assert!(dialect.keepspace);
    }

    #[test]
    fn test_parse_single_char_option_accepts_a_literal_character() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let byte = parse_single_char_option(";", "delim", &mut diagnostics, None);

        // Then
        assert_eq!(byte, Some(b';'));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_single_char_option_accepts_the_space_and_tab_keywords() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let space = parse_single_char_option("space", "delim", &mut diagnostics, None);
        let tab = parse_single_char_option("tab", "delim", &mut diagnostics, None);

        // Then
        assert_eq!(space, Some(b' '));
        assert_eq!(tab, Some(b'\t'));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_single_char_option_accepts_every_code_point_spelling() {
        // Given
        let spellings = ["0x20", "0X20", "\\x20", "x20", "u+0020", "U+0020"];
        let mut diagnostics = Diagnostics::default();

        // When / Then
        for spelling in spellings {
            assert_eq!(
                parse_single_char_option(spelling, "delim", &mut diagnostics, None),
                Some(b' '),
                "spelling {spelling} should resolve to a space"
            );
        }
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_single_char_option_rejects_a_multi_character_value() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let byte = parse_single_char_option("::", "delim", &mut diagnostics, None);

        // Then
        assert_eq!(byte, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.contains("single character"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_parse_single_char_option_rejects_an_empty_value() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let byte = parse_single_char_option("", "quote", &mut diagnostics, None);

        // Then
        assert_eq!(byte, None);
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_parse_single_char_option_rejects_a_non_ascii_character() {
        // Given — docutils would accept this; we cannot, see the doc comment.
        let mut diagnostics = Diagnostics::default();

        // When
        let byte = parse_single_char_option("§", "delim", &mut diagnostics, None);

        // Then
        assert_eq!(byte, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.contains("only ASCII"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_parse_single_char_option_rejects_a_non_ascii_code_point() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let byte = parse_single_char_option("0x00a7", "delim", &mut diagnostics, None);

        // Then
        assert_eq!(byte, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.contains("only ASCII"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_parse_csv_rows_splits_simple_data() {
        // Given
        let data = "Fruit,Colour\nApple,Red\n";

        // When
        let parsed = rows(data, &CsvDialect::default());

        // Then
        assert_eq!(parsed, vec![vec!["Fruit", "Colour"], vec!["Apple", "Red"]]);
    }

    #[test]
    fn test_parse_csv_rows_keeps_a_comma_inside_a_quoted_field() {
        // Given
        let data = "\"Apple, Braeburn\",Red\n";

        // When
        let parsed = rows(data, &CsvDialect::default());

        // Then
        assert_eq!(parsed, vec![vec!["Apple, Braeburn", "Red"]]);
    }

    #[test]
    fn test_parse_csv_rows_unescapes_a_doubled_quote() {
        // Given
        let data = "\"He said \"\"hi\"\"\",Red\n";

        // When
        let parsed = rows(data, &CsvDialect::default());

        // Then
        assert_eq!(parsed, vec![vec!["He said \"hi\"", "Red"]]);
    }

    #[test]
    fn test_parse_csv_rows_keeps_a_newline_inside_a_quoted_field() {
        // Given
        let data = "\"first\nsecond\",Red\n";

        // When
        let parsed = rows(data, &CsvDialect::default());

        // Then
        assert_eq!(parsed, vec![vec!["first\nsecond", "Red"]]);
    }

    #[test]
    fn test_parse_csv_rows_honours_a_custom_delimiter() {
        // Given
        let dialect = CsvDialect {
            delimiter: b';',
            ..CsvDialect::default()
        };

        // When
        let parsed = rows("Apple;Red\n", &dialect);

        // Then
        assert_eq!(parsed, vec![vec!["Apple", "Red"]]);
    }

    #[test]
    fn test_parse_csv_rows_honours_a_custom_quote_character() {
        // Given
        let dialect = CsvDialect {
            quote: b'\'',
            ..CsvDialect::default()
        };

        // When
        let parsed = rows("'Apple, Braeburn',Red\n", &dialect);

        // Then
        assert_eq!(parsed, vec![vec!["Apple, Braeburn", "Red"]]);
    }

    #[test]
    fn test_parse_csv_rows_honours_a_custom_escape_character() {
        // Given
        let dialect = CsvDialect {
            escape: Some(b'\\'),
            ..CsvDialect::default()
        };

        // When
        let parsed = rows("\"He said \\\"hi\\\"\",Red\n", &dialect);

        // Then
        assert_eq!(parsed, vec![vec!["He said \"hi\"", "Red"]]);
    }

    #[test]
    fn test_parse_csv_rows_strips_leading_space_by_default() {
        // Given
        let data = "Apple,   Red\n";

        // When
        let parsed = rows(data, &CsvDialect::default());

        // Then
        assert_eq!(parsed, vec![vec!["Apple", "Red"]]);
    }

    #[test]
    fn test_parse_csv_rows_keeps_leading_space_under_keepspace() {
        // Given
        let dialect = CsvDialect {
            keepspace: true,
            ..CsvDialect::default()
        };

        // When
        let parsed = rows("Apple,   Red\n", &dialect);

        // Then
        assert_eq!(parsed, vec![vec!["Apple", "   Red"]]);
    }

    #[test]
    fn test_parse_csv_rows_keeps_trailing_space() {
        // Given — docutils' skipinitialspace never touches trailing space.
        let data = "Apple  ,Red\n";

        // When
        let parsed = rows(data, &CsvDialect::default());

        // Then
        assert_eq!(parsed, vec![vec!["Apple  ", "Red"]]);
    }

    #[test]
    fn test_parse_csv_rows_allows_rows_of_differing_lengths() {
        // Given
        let data = "a,b,c\nd,e\n";

        // When
        let parsed = rows(data, &CsvDialect::default());

        // Then
        assert_eq!(parsed, vec![vec!["a", "b", "c"], vec!["d", "e"]]);
    }

    #[test]
    fn test_parse_csv_rows_treats_a_leading_hash_as_data() {
        // Given
        let data = "#hash,Red\n";

        // When
        let parsed = rows(data, &CsvDialect::default());

        // Then
        assert_eq!(parsed, vec![vec!["#hash", "Red"]]);
    }

    #[test]
    fn test_parse_csv_rows_reports_an_unterminated_quoted_field() {
        // Given
        let data = "\"unterminated, Red\nApple, Red\n";

        // When
        let result = parse_csv_rows(data, &CsvDialect::default());

        // Then
        let message = result.expect_err("expected an error for unterminated quoting");
        assert!(message.contains("unterminated"), "{message}");
    }

    #[test]
    fn test_has_unterminated_quote_accepts_balanced_data() {
        // Given / When / Then
        let dialect = CsvDialect::default();
        assert!(!has_unterminated_quote("\"a\",b\n", &dialect));
        assert!(!has_unterminated_quote("a,b\n", &dialect));
        assert!(!has_unterminated_quote("\"a\"\"b\"\n", &dialect));
        assert!(!has_unterminated_quote("\"a\nb\"\n", &dialect));
    }

    #[test]
    fn test_has_unterminated_quote_detects_an_unclosed_field() {
        // Given / When / Then
        let dialect = CsvDialect::default();
        assert!(has_unterminated_quote("\"a,b\n", &dialect));
        assert!(has_unterminated_quote("a,\"b\n", &dialect));
    }

    #[test]
    fn test_has_unterminated_quote_respects_the_escape_character() {
        // Given — the escaped quote must not be read as the field's end.
        let dialect = CsvDialect {
            escape: Some(b'\\'),
            ..CsvDialect::default()
        };

        // When / Then
        assert!(!has_unterminated_quote("\"a\\\"b\"\n", &dialect));
        assert!(has_unterminated_quote("\"a\\\"b\n", &dialect));
    }

    #[test]
    fn test_parse_csv_rows_returns_nothing_for_empty_data() {
        // Given
        let data = "";

        // When
        let parsed = rows(data, &CsvDialect::default());

        // Then
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_trim_leading_space_leaves_the_field_alone_under_keepspace() {
        // Given / When / Then
        assert_eq!(trim_leading_space("  x ", true), "  x ");
        assert_eq!(trim_leading_space("  x ", false), "x ");
        assert_eq!(trim_leading_space("\tx", false), "x");
    }
}
