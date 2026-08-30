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

use rusty_sphinx_ast::Domain;

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

/// Configuration for one parse, borrowed by every block-level parser.
pub struct ParseCtx<'a> {
    /// The domain a bare (unprefixed) directive or role resolves to.
    pub default_domain: Domain,
    /// How to read a file named by a `:file:` option.
    pub csv_files: &'a dyn CsvFileLoader,
}

impl ParseCtx<'_> {
    /// A context that resolves bare constructs in `default_domain` and has no
    /// filesystem access.
    #[must_use]
    pub fn with_domain(default_domain: Domain) -> Self {
        Self {
            default_domain,
            csv_files: &RejectCsvFiles,
        }
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
