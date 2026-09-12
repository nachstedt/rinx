//! Rendering one document's source as a Jinja template.

use std::collections::BTreeMap;

use minijinja::{Environment, ErrorKind, UndefinedBehavior};

use crate::error::{TemplateError, TemplateErrorKind};
use crate::loader::TemplateLoader;
use crate::marker::{SourceLine, inject, strip};
use crate::scan::{Scan, scan};

/// The name the document itself is registered under.
///
/// Angle brackets so it cannot collide with a file somebody wrote: a template
/// is named by a path, and no path is spelled like this.
const DOCUMENT: &str = "<document>";

/// How deep a chain of `{% include %}`s may go.
///
/// A backstop rather than the mechanism — a genuine cycle is caught exactly,
/// and this only bounds a chain that is long without repeating.
const MAX_DEPTH: usize = 16;

/// A document's source after the Jinja pass, and where each line came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSource {
    /// The rendered text, with every line marker removed.
    pub text: String,
    /// One entry per line of [`text`](Self::text), in order.
    pub lines: Vec<SourceLine>,
    /// The loader ids of the templates rendered into this document, indexed by
    /// [`SourceLine::template`].
    pub templates: Vec<String>,
}

impl RenderedSource {
    /// The identity result: `text` unchanged, every line its own.
    fn unchanged(text: &str) -> Self {
        Self {
            text: text.to_string(),
            lines: (1..=u32::try_from(text.lines().count()).unwrap_or(u32::MAX))
                .map(|line| SourceLine {
                    template: None,
                    line,
                })
                .collect(),
            templates: Vec::new(),
        }
    }
}

/// One template read on the way to rendering a document.
struct Template {
    /// The name it is written as, which is how `MiniJinja` resolves it.
    name: String,
    /// The loader's resolved id, which is how a diagnostic names it.
    id: String,
    /// Its text, with line markers injected.
    marked: String,
}

/// Renders `text` as a Jinja template, tracking where every rendered line came
/// from.
///
/// `context` binds names the template may read, in the way a Sphinx project's
/// `html_context` does. A template that reads a name bound by neither it nor a
/// `{% set %}` is an error rather than an empty string.
///
/// # Errors
///
/// Returns the first construct that could not be scanned, read or rendered,
/// attributed to the file it was written in.
pub fn render_source(
    text: &str,
    loader: &dyn TemplateLoader,
    context: &[(String, String)],
) -> Result<RenderedSource, TemplateError> {
    if !holds_jinja(text) {
        return Ok(RenderedSource::unchanged(text));
    }
    let document = scan(text)?;
    let mut templates = Vec::new();
    collect(&document, None, &mut Vec::new(), &mut templates, loader)?;

    let marked = inject(text, 0, &document.line_starts_inside_tag);
    let rendered = render_marked(&marked, &templates, context)?;
    let (text, lines) = strip(&rendered);
    Ok(RenderedSource {
        text,
        lines,
        templates: templates.into_iter().map(|template| template.id).collect(),
    })
}

/// Whether `text` contains a Jinja delimiter at all.
///
/// The fast path matters twice: it keeps a project that templates nothing from
/// paying for a renderer, and it keeps a document that merely *mentions* `{{`
/// from being rendered — this pass is opt-in per library, and within a library
/// only the documents that actually use Jinja should be subject to its rules.
fn holds_jinja(text: &str) -> bool {
    text.contains("{{") || text.contains("{%") || text.contains("{#")
}

/// Reads every template reachable from `origin`, depth first.
///
/// `chain` is the names currently being read, outermost first — what makes a
/// cycle detectable. `relative_to` is the loader id of the file `origin` was
/// scanned from, which is what a nested include resolves against.
fn collect(
    origin: &Scan,
    relative_to: Option<&str>,
    chain: &mut Vec<String>,
    templates: &mut Vec<Template>,
    loader: &dyn TemplateLoader,
) -> Result<(), TemplateError> {
    for include in &origin.includes {
        if chain.contains(&include.name) || chain.len() >= MAX_DEPTH {
            let mut cycle = chain.clone();
            cycle.push(include.name.clone());
            return Err(TemplateError {
                kind: TemplateErrorKind::RecursiveInclude(cycle),
                template: relative_to.map(ToString::to_string),
                line: Some(include.line),
            });
        }
        if templates.iter().any(|known| known.name == include.name) {
            continue;
        }
        let template =
            loader
                .load(&include.name, relative_to)
                .map_err(|message| TemplateError {
                    kind: TemplateErrorKind::NotFound {
                        name: include.name.clone(),
                        message,
                    },
                    template: relative_to.map(ToString::to_string),
                    line: Some(include.line),
                })?;
        let nested =
            scan(&template.text).map_err(|error| error.in_template(Some(template.id.clone())))?;
        let index = templates.len();
        templates.push(Template {
            name: include.name.clone(),
            id: template.id.clone(),
            marked: inject(&template.text, index + 1, &nested.line_starts_inside_tag),
        });
        chain.push(include.name.clone());
        collect(&nested, Some(&template.id), chain, templates, loader)?;
        chain.pop();
    }
    Ok(())
}

/// Renders the already-marked document against the already-marked templates.
fn render_marked(
    marked: &str,
    templates: &[Template],
    context: &[(String, String)],
) -> Result<String, TemplateError> {
    let mut environment = Environment::new();
    // Strict rather than Jinja2's silent default: see `TemplateErrorKind::Undefined`.
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    // MiniJinja drops a template's final newline unless told not to. Here that
    // would delete a blank line — which in reStructuredText ends a block — both
    // at the end of the document and at the end of every included fragment.
    environment.set_keep_trailing_newline(true);
    for template in templates {
        environment
            .add_template_owned(template.name.clone(), template.marked.clone())
            .map_err(|error| translate(&error, templates))?;
    }
    environment
        .add_template_owned(DOCUMENT, marked.to_string())
        .map_err(|error| translate(&error, templates))?;
    let values: BTreeMap<&str, &str> = context
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    environment
        .get_template(DOCUMENT)
        .and_then(|template| template.render(values))
        .map_err(|error| translate(&error, templates))
}

/// Turns a `MiniJinja` failure into one of ours, attributed to the file it
/// happened in.
///
/// An error raised inside an `{% include %}` arrives wrapped: the outer layer
/// names the including template and the inner one the real cause, so the chain
/// is walked for both the message and the position.
fn translate(error: &minijinja::Error, templates: &[Template]) -> TemplateError {
    let mut cause = error;
    let mut located = error.line().map(|_| error);
    while let Some(source) = std::error::Error::source(cause)
        .and_then(|source| source.downcast_ref::<minijinja::Error>())
    {
        cause = source;
        if cause.line().is_some() {
            located = Some(cause);
        }
    }
    let message = cause.detail().unwrap_or("").to_string();
    let message = if message.is_empty() {
        cause.kind().to_string()
    } else {
        message
    };
    let kind = match cause.kind() {
        ErrorKind::SyntaxError => TemplateErrorKind::Syntax(message),
        ErrorKind::UndefinedError => TemplateErrorKind::Undefined(message),
        _ => TemplateErrorKind::Render(message),
    };
    let position = located.unwrap_or(cause);
    TemplateError {
        kind,
        template: position
            .name()
            .filter(|name| *name != DOCUMENT)
            .and_then(|name| templates.iter().find(|template| template.name == name))
            .map(|template| template.id.clone()),
        line: position.line(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::{LoadedTemplate, RejectTemplates};

    /// A loader over a fixed set of `(id, text)` pairs, resolving a name to
    /// itself.
    struct Fixed(Vec<(&'static str, &'static str)>);

    impl TemplateLoader for Fixed {
        fn load(&self, path: &str, _relative_to: Option<&str>) -> Result<LoadedTemplate, String> {
            self.0
                .iter()
                .find(|(id, _)| *id == path)
                .map(|(id, text)| LoadedTemplate {
                    id: (*id).to_string(),
                    text: (*text).to_string(),
                })
                .ok_or_else(|| format!("no such file '{path}'"))
        }
    }

    fn render(text: &str, loader: &dyn TemplateLoader) -> RenderedSource {
        render_source(text, loader, &[]).expect("renders")
    }

    #[test]
    fn test_text_without_jinja_is_returned_unchanged() {
        // Given
        let text = "Title\n=====\n\nA { brace } and a 100% sign.\n";

        // When
        let rendered = render(text, &RejectTemplates);

        // Then
        assert_eq!(rendered.text, text);
        assert_eq!(rendered.templates, Vec::<String>::new());
        assert_eq!(rendered.lines.len(), 4);
        assert_eq!(rendered.lines[3].line, 4);
    }

    #[test]
    fn test_a_set_and_an_include_splice_the_template_in() {
        // Given
        let loader = Fixed(vec![("header.rst", "Source: {{page}}\n")]);
        let text =
            "{% set page=\"index.rst\" %}\n{% include \"header.rst\" with context %}\n\nBody\n";

        // When
        let rendered = render(text, &loader);

        // Then
        assert_eq!(rendered.text, "\nSource: index.rst\n\n\nBody\n");
        assert_eq!(rendered.templates, vec!["header.rst".to_string()]);
    }

    #[test]
    fn test_every_rendered_line_names_the_file_it_was_written_in() {
        // Given
        let loader = Fixed(vec![("header.rst", "one\ntwo\n")]);
        let text = "{% include \"header.rst\" %}\nafter\n";

        // When
        let rendered = render(text, &loader);

        // Then
        assert_eq!(rendered.text, "one\ntwo\n\nafter\n");
        let origins: Vec<_> = rendered
            .lines
            .iter()
            .map(|line| (line.template, line.line))
            .collect();
        assert_eq!(
            origins,
            vec![
                (Some(0), 1), // the included file's first line
                (Some(0), 2), // its second
                (Some(0), 2), // the blank line it ended with, carried over
                (None, 2),    // and the document's own text below it
            ]
        );
    }

    #[test]
    fn test_a_context_value_is_bound_for_the_document_to_read() {
        // Given
        let text = "Version {{ version }}\n";

        // When
        let rendered = render_source(
            text,
            &RejectTemplates,
            &[("version".to_string(), "1.2".to_string())],
        )
        .expect("renders");

        // Then
        assert_eq!(rendered.text, "Version 1.2\n");
    }

    #[test]
    fn test_an_undefined_value_is_reported_rather_than_emptied() {
        // Given
        let text = "Version {{ version }}\n";

        // When
        let error = render_source(text, &RejectTemplates, &[]).expect_err("undefined");

        // Then
        assert!(matches!(error.kind, TemplateErrorKind::Undefined(_)));
        assert_eq!(error.template, None);
        assert_eq!(error.line, Some(1));
    }

    #[test]
    fn test_a_missing_template_names_the_file_and_the_line_it_was_asked_for() {
        // Given
        let text = "intro\n\n{% include \"header.rst\" %}\n";

        // When
        let error = render_source(text, &Fixed(vec![]), &[]).expect_err("missing");

        // Then
        assert_eq!(
            error.kind,
            TemplateErrorKind::NotFound {
                name: "header.rst".to_string(),
                message: "no such file 'header.rst'".to_string(),
            }
        );
        assert_eq!(error.line, Some(3));
    }

    #[test]
    fn test_a_syntax_error_inside_a_template_names_that_template() {
        // Given
        let loader = Fixed(vec![("header.rst", "ok\n{% for %}\n")]);
        let text = "{% include \"header.rst\" %}\n";

        // When
        let error = render_source(text, &loader, &[]).expect_err("bad template");

        // Then
        assert!(matches!(error.kind, TemplateErrorKind::Syntax(_)));
        assert_eq!(error.template.as_deref(), Some("header.rst"));
        assert_eq!(error.line, Some(2));
    }

    #[test]
    fn test_an_include_cycle_is_reported_as_a_chain() {
        // Given
        let loader = Fixed(vec![
            ("a.rst", "{% include \"b.rst\" %}\n"),
            ("b.rst", "{% include \"a.rst\" %}\n"),
        ]);
        let text = "{% include \"a.rst\" %}\n";

        // When
        let error = render_source(text, &loader, &[]).expect_err("cycle");

        // Then
        assert_eq!(
            error.kind,
            TemplateErrorKind::RecursiveInclude(vec![
                "a.rst".to_string(),
                "b.rst".to_string(),
                "a.rst".to_string(),
            ])
        );
    }

    #[test]
    fn test_a_computed_template_name_is_refused_by_name() {
        // Given
        let text = "{% set page=\"a.rst\" %}\n{% include page %}\n";

        // When
        let error = render_source(text, &Fixed(vec![]), &[]).expect_err("dynamic");

        // Then
        assert_eq!(
            error.kind,
            TemplateErrorKind::DynamicName("page".to_string())
        );
        assert_eq!(error.line, Some(2));
    }

    #[test]
    fn test_a_whitespace_control_modifier_is_refused_on_its_own_line() {
        // Given
        let text = "intro\n{%- set page=\"a\" %}\n";

        // When
        let error = render_source(text, &Fixed(vec![]), &[]).expect_err("trim");

        // Then
        assert_eq!(error.kind, TemplateErrorKind::WhitespaceControl);
        assert_eq!(error.line, Some(2));
    }

    #[test]
    fn test_a_loop_maps_every_generated_line_to_the_line_that_generated_it() {
        // Given
        let text = "{% for name in [\"a\", \"b\"] %}\n* {{ name }}\n{% endfor %}\n";

        // When
        let rendered = render(text, &RejectTemplates);

        // Then
        assert_eq!(rendered.text, "\n* a\n\n* b\n\n");
        let origins: Vec<_> = rendered.lines.iter().map(|line| line.line).collect();
        // Line 3 is where `{% endfor %}` is written, and the text before it
        // is part of the body — so every iteration passes through it.
        assert_eq!(origins, vec![1, 2, 3, 2, 3]);
    }

    #[test]
    fn test_a_raw_block_keeps_its_jinja_as_text() {
        // Given
        let text = "{% raw %}\n{{ not_a_value }}\n{% endraw %}\n";

        // When
        let rendered = render(text, &RejectTemplates);

        // Then
        assert_eq!(rendered.text, "\n{{ not_a_value }}\n\n");
    }
}
