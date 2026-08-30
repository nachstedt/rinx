//! Parse-time configuration threaded through every block-level parser.
//!
//! Two things every nested parse needs to know: the domain a *bare* directive
//! or role resolves to, and how to obtain the contents of a file a directive
//! names (today only `.. csv-table::`'s `:file:` option). They travel together
//! in a [`ParseCtx`] rather than as separate parameters, so adding the next
//! piece of parse-time configuration doesn't touch two dozen signatures again.
//!
//! The loader is an injected trait object rather than a direct
//! `std::fs::read_to_string` call because this crate performs no I/O of its
//! own: it is also the live-preview path, where a document is parsed straight
//! from an editor buffer, and it is exercised by unit tests that must not
//! depend on the filesystem. `rusty_sphinx_worker` supplies the real
//! filesystem-backed loader.

use rusty_sphinx_ast::{Domain, Position, Span};

/// Supplies the contents of a file named by a directive option.
///
/// `path` is exactly the text the author wrote (e.g. `data/fruits.csv`);
/// resolving it against a base directory is the implementation's job. The
/// error string is surfaced verbatim as a parse diagnostic, so it should read
/// as an explanation to the document's author.
pub trait CsvFileLoader {
    /// # Errors
    ///
    /// Returns a human-readable explanation when the file cannot be read.
    fn load(&self, path: &str) -> Result<String, String>;
}

/// The default loader: refuses every request.
///
/// Used wherever no filesystem context exists — [`crate::parse`], the legacy
/// `process_rst()` path, and every unit test — so that a `:file:` option in
/// those contexts produces an honest diagnostic instead of silently reading
/// something relative to the process's working directory.
pub struct RejectCsvFiles;

impl CsvFileLoader for RejectCsvFiles {
    fn load(&self, path: &str) -> Result<String, String> {
        Err(format!(
            "cannot read '{path}': this parse was given no directory to resolve \
             :file: against"
        ))
    }
}

/// Where the line slice a parser is currently walking sits in the original
/// document, so a local `(line, column)` can be translated back to a real
/// source position.
///
/// Both fields are the 1-based position, *in the original document*, of
/// `lines[0]`'s first character. A nested parse of a directive body or a list
/// item runs over a freshly built `Vec<&str>` whose indices start at zero
/// again, and several of those also strip a common indent — so a line offset
/// alone is not enough, and the column must be carried too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Origin {
    line: u32,
    column: u32,
}

/// Configuration for one parse, borrowed by every block-level parser.
pub struct ParseCtx<'a> {
    /// The domain a bare (unprefixed) directive or role resolves to.
    pub default_domain: Domain,
    /// How to read a file named by a `:file:` option.
    pub csv_files: &'a dyn CsvFileLoader,
    /// Where the current line slice came from, or `None` when it came from
    /// nowhere in the source — see [`Self::synthetic`].
    origin: Option<Origin>,
}

impl<'a> ParseCtx<'a> {
    /// A context that resolves bare constructs in `default_domain` and has no
    /// filesystem access.
    #[must_use]
    pub fn with_domain(default_domain: Domain) -> Self {
        Self {
            default_domain,
            csv_files: &RejectCsvFiles,
            origin: Some(Origin { line: 1, column: 1 }),
        }
    }

    /// A context that resolves bare constructs in `default_domain` and reads
    /// `:file:` options through `csv_files`.
    #[must_use]
    pub fn new(default_domain: Domain, csv_files: &'a dyn CsvFileLoader) -> Self {
        Self {
            default_domain,
            csv_files,
            origin: Some(Origin { line: 1, column: 1 }),
        }
    }

    /// The context for a nested parse over a line slice that begins
    /// `line_offset` lines below this one's first line, and whose lines have
    /// had `column_offset` leading characters stripped.
    ///
    /// Offsets compose, so nesting a body inside a body inside a list item
    /// still lands on the right source position.
    #[must_use]
    pub(crate) fn nested(&self, line_offset: usize, column_offset: usize) -> Self {
        Self {
            default_domain: self.default_domain,
            csv_files: self.csv_files,
            origin: self.origin.map(|origin| Origin {
                line: origin.line + u32::try_from(line_offset).unwrap_or(0),
                column: origin.column + u32::try_from(column_offset).unwrap_or(0),
            }),
        }
    }

    /// The context for a nested parse over lines that exist nowhere in the
    /// source — today only the rows `.. csv-table::` generates from CSV data.
    ///
    /// Everything parsed under it reports positionless diagnostics, which is
    /// honest: pointing at a line of the `.rst` that does not contain the
    /// offending text would be worse than pointing nowhere.
    #[must_use]
    pub(crate) fn synthetic(&self) -> Self {
        Self {
            default_domain: self.default_domain,
            csv_files: self.csv_files,
            origin: None,
        }
    }

    /// The source position of `local_column` on `local_line`, both 0-based
    /// indices into the slice being parsed.
    #[must_use]
    pub(crate) fn position(&self, local_line: usize, local_column: usize) -> Option<Position> {
        self.origin.map(|origin| {
            Position::new(
                origin.line + u32::try_from(local_line).unwrap_or(0),
                origin.column + u32::try_from(local_column).unwrap_or(0),
            )
        })
    }

    /// A span covering the whole of `local_line`, whose content is `text`.
    /// The common shape for a block-level diagnostic: it can name the
    /// offending line, but no column within it means anything.
    #[must_use]
    pub(crate) fn line_span(&self, local_line: usize, text: &str) -> Option<Span> {
        let start = self.position(local_line, 0)?;
        let end = self.position(local_line, text.chars().count())?;
        Some(Span::new(start, end))
    }

    /// A span covering `first` through `last` inclusive (0-based indices),
    /// for a diagnostic about a multi-line construct as a whole. `last_text`
    /// is the content of the last line.
    #[must_use]
    pub(crate) fn lines_span(&self, first: usize, last: usize, last_text: &str) -> Option<Span> {
        let start = self.position(first, 0)?;
        let end = self.position(last, last_text.chars().count())?;
        Some(Span::new(start, end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_domain_keeps_the_requested_domain() {
        // Given / When
        let ctx = ParseCtx::with_domain(Domain::C);

        // Then
        assert_eq!(ctx.default_domain, Domain::C);
    }

    #[test]
    fn test_reject_csv_files_names_the_path_it_refused() {
        // Given
        let loader = RejectCsvFiles;

        // When
        let result = loader.load("data/fruits.csv");

        // Then
        let message = result.expect_err("RejectCsvFiles must refuse every path");
        assert!(message.contains("data/fruits.csv"), "{message}");
        assert!(message.contains(":file:"), "{message}");
    }

    #[test]
    fn test_with_domain_uses_the_rejecting_loader() {
        // Given
        let ctx = ParseCtx::with_domain(Domain::Py);

        // When
        let result = ctx.csv_files.load("anything.csv");

        // Then
        assert!(result.is_err());
    }
}
