//! What can go wrong while rendering a document's source as a template.

use std::fmt;

/// A template that could not be rendered.
///
/// Carries *where* rather than *what to call it*: the diagnostic code lives
/// with the caller, as it does for [`rusty_sphinx_filter`]'s errors, because
/// this crate does not depend on the vocabulary those codes are declared in.
///
/// [`rusty_sphinx_filter`]: https://docs.rs/rusty_sphinx_filter
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateError {
    /// What went wrong.
    pub kind: TemplateErrorKind,
    /// The loader id of the template it went wrong in, or `None` for the
    /// document itself.
    pub template: Option<String>,
    /// The 1-based line within that file, when one is known.
    pub line: Option<usize>,
}

impl TemplateError {
    /// An error found in the document itself, on `line`.
    #[must_use]
    pub(crate) const fn in_document(kind: TemplateErrorKind, line: Option<usize>) -> Self {
        Self {
            kind,
            template: None,
            line,
        }
    }

    /// The same error, attributed to the template with loader id `template`.
    #[must_use]
    pub(crate) fn in_template(mut self, template: Option<String>) -> Self {
        self.template = template;
        self
    }
}

/// The kinds of failure [`render_source`](crate::render_source) reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateErrorKind {
    /// The text is not valid Jinja.
    Syntax(String),
    /// A variable the template reads is bound to nothing.
    ///
    /// Reported rather than substituted with the empty string, which is what
    /// Jinja2 does by default: a page missing a value it asked for looks
    /// finished and is wrong, and the empty string leaves nothing to grep for.
    Undefined(String),
    /// Rendering failed for a reason that is neither of the above.
    Render(String),
    /// A template named by `{% include %}` that could not be read. The string
    /// is the loader's own explanation.
    NotFound { name: String, message: String },
    /// A whitespace-control modifier (`{%-`, `-%}` and friends), which this
    /// renderer refuses — see the crate documentation for why.
    WhitespaceControl,
    /// An `{% include %}` whose target is an expression rather than a string
    /// literal, so the build cannot know which file it reads.
    DynamicName(String),
    /// An `{% include %}` chain that reaches a template already being
    /// rendered.
    RecursiveInclude(Vec<String>),
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            TemplateErrorKind::Syntax(message) => write!(f, "invalid Jinja template: {message}"),
            TemplateErrorKind::Undefined(message) => {
                write!(f, "undefined value in a Jinja template: {message}")
            }
            TemplateErrorKind::Render(message) => {
                write!(f, "Jinja template could not be rendered: {message}")
            }
            TemplateErrorKind::NotFound { name, message } => {
                write!(f, "cannot include template '{name}': {message}")
            }
            TemplateErrorKind::WhitespaceControl => write!(
                f,
                "whitespace-control modifiers ('{{%-', '-%}}') are not supported when templating \
                 reStructuredText, because indentation decides what a block contains"
            ),
            TemplateErrorKind::DynamicName(expression) => write!(
                f,
                "the template to include must be written as a quoted name, not as the expression \
                 '{expression}': the build declares every file it reads before it runs"
            ),
            TemplateErrorKind::RecursiveInclude(chain) => {
                write!(f, "template include cycle: {}", chain.join(" -> "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_names_the_missing_template_and_the_reason() {
        // Given
        let error = TemplateError::in_document(
            TemplateErrorKind::NotFound {
                name: "header.rst".to_string(),
                message: "no such file".to_string(),
            },
            Some(2),
        );

        // When
        let text = error.to_string();

        // Then
        assert_eq!(text, "cannot include template 'header.rst': no such file");
    }

    #[test]
    fn test_display_spells_the_cycle_as_a_chain() {
        // Given
        let error = TemplateError::in_document(
            TemplateErrorKind::RecursiveInclude(vec!["a.rst".to_string(), "a.rst".to_string()]),
            None,
        );

        // When
        let text = error.to_string();

        // Then
        assert_eq!(text, "template include cycle: a.rst -> a.rst");
    }

    #[test]
    fn test_in_template_attributes_the_error_to_that_file() {
        // Given
        let error = TemplateError::in_document(TemplateErrorKind::WhitespaceControl, Some(3));

        // When
        let attributed = error.in_template(Some("shared/header.rst".to_string()));

        // Then
        assert_eq!(attributed.template.as_deref(), Some("shared/header.rst"));
        assert_eq!(attributed.line, Some(3));
    }
}
