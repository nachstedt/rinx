//! Where a template's text comes from.
//!
//! The crate performs no I/O of its own, for the reason
//! `rusty_sphinx_parser`'s own file loader does not: the same rendering has to
//! work in a live preview, where the document is an editor buffer, and in unit
//! tests, which must not depend on a filesystem. The parser adapts its
//! [`ParseFileLoader`] to this trait, so a Jinja `{% include %}` and an
//! `.. include::` read through exactly one seam and a missing file is reported
//! once, in one voice.
//!
//! [`ParseFileLoader`]: https://docs.rs/rusty_sphinx_parser

/// A template that was read, and the identity the caller knows it by.
///
/// `id` is the resolved, source-root-relative path — the same canonical form
/// `.. include::` resolves against, so a template and an included fragment
/// naming the same file agree about what that file is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedTemplate {
    /// The resolved, source-root-relative path.
    pub id: String,
    /// The template's text.
    pub text: String,
}

/// Supplies the text of a template named by `{% include %}`.
pub trait TemplateLoader {
    /// Reads the template written as `path` inside the file whose id is
    /// `relative_to` (`None` for the document itself).
    ///
    /// # Errors
    ///
    /// Returns a human-readable explanation when the template cannot be read.
    /// It is surfaced verbatim to the document's author, so it should read as
    /// an explanation rather than an error code.
    fn load(&self, path: &str, relative_to: Option<&str>) -> Result<LoadedTemplate, String>;
}

/// The default loader: refuses every request.
///
/// Used where no filesystem context exists, so that an `{% include %}` in such
/// a context produces an honest diagnostic rather than silently reading
/// something relative to the process's working directory.
pub struct RejectTemplates;

impl TemplateLoader for RejectTemplates {
    fn load(&self, path: &str, _relative_to: Option<&str>) -> Result<LoadedTemplate, String> {
        Err(format!(
            "cannot read template '{path}': this parse was given no directory to resolve it against"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reject_templates_names_the_template_it_refused() {
        // Given
        let loader = RejectTemplates;

        // When
        let result = loader.load("header.rst", None);

        // Then
        assert_eq!(
            result,
            Err(
                "cannot read template 'header.rst': this parse was given no directory to resolve \
                 it against"
                    .to_string()
            )
        );
    }
}
