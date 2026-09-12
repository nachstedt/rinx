//! What can go wrong while expanding a diagram's template.

use std::fmt;

use rusty_sphinx_ast::DiagnosticCode;

/// A template that could not be expanded.
///
/// Carries no [`Span`](rusty_sphinx_ast::Span): the caller holds the diagram
/// node and knows where it was written, and a template error's *own* line is
/// relative to the template rather than to the document. Keeping the position
/// with the caller is what lets the renderer report against the directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UmlError {
    /// The template is not valid Jinja, or evaluating it failed. `line` is the
    /// line *within the template*, counted from 1, when the engine gave one.
    Template {
        message: String,
        line: Option<usize>,
    },
    /// A template asked for an entity no document declares.
    UnknownEntity(String),
    /// A `filter()` this build's filter language cannot evaluate.
    InvalidFilter { filter: String, message: String },
    /// An `.. entity-arch::` written outside any entity, so `need` would be
    /// bound to nothing.
    ArchOutsideEntity,
    /// A `uml()` import that reaches itself, directly or through a cycle.
    RecursiveImport { chain: Vec<String> },
    /// A `:config:` naming a preamble the site config does not declare.
    UnknownConfig(String),
    /// The expansion drew nothing at all.
    EmptyDiagram,
}

impl UmlError {
    /// The diagnostic code a caller reports this under.
    ///
    /// On the error rather than at each call site, so the renderer and any
    /// later reporter cannot disagree about what a given failure is called —
    /// a code is an author-facing name a `.. noqa:` spells.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::Template { .. } => DiagnosticCode::UmlTemplateError,
            Self::UnknownEntity(_) => DiagnosticCode::UmlUnknownEntity,
            Self::InvalidFilter { .. } => DiagnosticCode::UmlInvalidFilter,
            Self::ArchOutsideEntity => DiagnosticCode::UmlArchOutsideEntity,
            Self::RecursiveImport { .. } => DiagnosticCode::UmlRecursiveImport,
            Self::UnknownConfig(_) => DiagnosticCode::UmlUnknownConfig,
            Self::EmptyDiagram => DiagnosticCode::UmlEmptyResult,
        }
    }
}

impl fmt::Display for UmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Template {
                message,
                line: Some(line),
            } => write!(f, "template error on line {line} of the diagram: {message}"),
            Self::Template {
                message,
                line: None,
            } => write!(f, "template error: {message}"),
            Self::UnknownEntity(id) => {
                write!(f, "no entity is declared with the id '{id}'")
            }
            Self::InvalidFilter { filter, message } => {
                write!(f, "filter '{filter}' cannot be evaluated: {message}")
            }
            Self::ArchOutsideEntity => write!(
                f,
                "an architecture diagram is only meaningful inside an entity, where it draws the \
                 one it sits in; write `.. entity-diagram::` for a free-standing diagram"
            ),
            Self::UnknownConfig(name) => write!(
                f,
                "no PlantUML preamble named '{name}' is declared; add it under [uml_configs] in \
                 the site's rusty_sphinx.toml"
            ),
            Self::EmptyDiagram => write!(
                f,
                "the diagram is empty, so no picture was drawn — a filter that matches nothing \
                 expands to nothing, and PlantUML has no empty diagram to compile"
            ),
            Self::RecursiveImport { chain } => write!(
                f,
                "a diagram cannot import itself: {}",
                chain.join(" imports ")
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_each_failure_names_its_own_code() {
        // Given one of each
        let errors = [
            UmlError::Template {
                message: "unexpected end of input".to_string(),
                line: Some(3),
            },
            UmlError::UnknownEntity("REQ_404".to_string()),
            UmlError::InvalidFilter {
                filter: "len(x) > 0".to_string(),
                message: "function calls are not supported".to_string(),
            },
            UmlError::ArchOutsideEntity,
            UmlError::RecursiveImport {
                chain: vec!["A".to_string(), "A".to_string()],
            },
            UmlError::UnknownConfig("monochrome".to_string()),
            UmlError::EmptyDiagram,
        ];

        // When
        let codes: Vec<DiagnosticCode> = errors.iter().map(UmlError::code).collect();

        // Then — every failure is distinguishable by its code, so a `.. noqa:`
        // can silence one without silencing the rest
        let mut unique = codes.clone();
        unique.sort_by_key(|code| code.as_str());
        unique.dedup();
        assert_eq!(unique.len(), codes.len());
    }

    #[test]
    fn test_a_template_error_names_the_line_it_was_found_on() {
        // Given
        let error = UmlError::Template {
            message: "unknown filter".to_string(),
            line: Some(4),
        };

        // When
        let message = error.to_string();

        // Then
        assert!(message.contains("line 4"), "{message}");
        assert!(message.contains("unknown filter"), "{message}");
    }

    #[test]
    fn test_a_template_error_without_a_line_still_reads() {
        // Given — minijinja does not always know where a failure happened
        let error = UmlError::Template {
            message: "unknown filter".to_string(),
            line: None,
        };

        // When
        let message = error.to_string();

        // Then
        assert!(!message.contains("line"), "{message}");
        assert!(message.contains("unknown filter"), "{message}");
    }

    #[test]
    fn test_an_unknown_entity_quotes_the_id_that_was_asked_for() {
        // Given
        let error = UmlError::UnknownEntity("REQ_404".to_string());

        // When / Then
        assert!(error.to_string().contains("REQ_404"));
    }

    #[test]
    fn test_a_recursive_import_shows_the_chain() {
        // Given
        let error = UmlError::RecursiveImport {
            chain: vec!["A".to_string(), "B".to_string(), "A".to_string()],
        };

        // When
        let message = error.to_string();

        // Then
        assert!(message.contains("A imports B imports A"), "{message}");
    }
}
