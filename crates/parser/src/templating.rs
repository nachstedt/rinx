//! Rendering a document's source as a Jinja template before parsing it.
//!
//! Real Sphinx projects template their `.rst` files with a `conf.py` hook on
//! the `source-read` event; this build cannot run one, so the same transform
//! is a declared, opt-in step instead — `rinx_template` performs it
//! and that crate's documentation explains why it exists and what it refuses.
//!
//! What lives here is the part that belongs to *parsing*: turning the crate's
//! per-line origins into a [`TemplateMap`] the parse can ask questions of, and
//! reporting a template that would not render as a diagnostic rather than a
//! failure. The parse then continues on the **unrendered** text, so a broken
//! template costs the page its expansion and nothing else — the same
//! error-resilience the rest of this crate is written for.

use rinx_ast::{Diagnostic, DiagnosticCode, FileId, Span};
use rinx_template::{
    LoadedTemplate, RenderedSource, TemplateError, TemplateErrorKind, TemplateLoader, render_source,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

/// Where one line of the text being parsed was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WrittenLine {
    /// The 1-based line within that file.
    pub(crate) line: u32,
    /// The file, or `None` for the document itself.
    pub(crate) file: Option<FileId>,
}

/// Where every line of a rendered document was written.
///
/// A `Vec` indexed by the rendered line rather than a range map: a Jinja pass
/// can attribute consecutive rendered lines to arbitrarily distant source
/// lines, so there is nothing to compress, and the lookup is on the path of
/// every span the parse builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TemplateMap {
    lines: Vec<WrittenLine>,
}

impl TemplateMap {
    /// Where the 1-based `rendered` line was written.
    ///
    /// A line past the end maps to itself in the document. That is
    /// unreachable for a position inside the text — the map has one entry per
    /// line of it — and is the honest answer for the one-past-the-end position
    /// a span at the very end of a document can name.
    pub(crate) fn written_at(&self, rendered: u32) -> WrittenLine {
        let index = usize::try_from(rendered)
            .unwrap_or(usize::MAX)
            .wrapping_sub(1);
        self.lines.get(index).copied().unwrap_or(WrittenLine {
            line: rendered,
            file: None,
        })
    }
}

/// A document's source after the Jinja pass.
pub(crate) struct TemplatedSource {
    /// The rendered text, which is what gets parsed.
    pub(crate) text: String,
    /// Where each of its lines was written.
    pub(crate) map: TemplateMap,
}

/// The text a parse under `ctx` would actually see, or `None` when templating
/// is off or the render failed.
///
/// Exists for the `--dump-rendered` debugging flag, which is how somebody
/// looking at a surprising page sees the source the parser saw. It renders a
/// second time rather than handing the parse's own result back, because that
/// result is a parse-internal detail and the flag is never passed by a build.
#[must_use]
pub fn rendered_source(input: &str, ctx: &ParseCtx<'_>) -> Option<String> {
    let context = ctx.jinja?;
    render_source(input, &ParseFileTemplates(ctx), context)
        .ok()
        .map(|rendered| rendered.text)
}

/// Renders `input` as a Jinja template, if this parse was told to.
///
/// Returns `None` when templating is off, and also when it failed — the
/// diagnostic is recorded and the caller parses the original text.
pub(crate) fn apply_templating(
    input: &str,
    ctx: &ParseCtx<'_>,
    diagnostics: &mut Diagnostics,
) -> Option<TemplatedSource> {
    let context = ctx.jinja?;
    let loader = ParseFileTemplates(ctx);
    match render_source(input, &loader, context) {
        Ok(rendered) => Some(interned(rendered, diagnostics)),
        Err(error) => {
            let file = diagnostics_file(&error, diagnostics);
            diagnostics.push(diagnose(&error, file));
            None
        }
    }
}

/// Interns each template the render read, so the map can name it by id.
fn interned(rendered: RenderedSource, diagnostics: &mut Diagnostics) -> TemplatedSource {
    let files: Vec<FileId> = rendered
        .templates
        .iter()
        .map(|id| diagnostics.intern_source_file(id))
        .collect();
    let lines = rendered
        .lines
        .iter()
        .map(|line| WrittenLine {
            line: line.line,
            file: line.template.and_then(|index| files.get(index).copied()),
        })
        .collect();
    TemplatedSource {
        text: rendered.text,
        map: TemplateMap { lines },
    }
}

/// The id a failed render's file is known by, interning it if need be.
fn diagnostics_file(error: &TemplateError, diagnostics: &mut Diagnostics) -> Option<FileId> {
    error
        .template
        .as_deref()
        .map(|id| diagnostics.intern_source_file(id))
}

/// The diagnostic a failed render is reported as.
fn diagnose(error: &TemplateError, file: Option<FileId>) -> Diagnostic {
    let code = match error.kind {
        TemplateErrorKind::Syntax(_) => DiagnosticCode::JinjaSyntax,
        TemplateErrorKind::Undefined(_) => DiagnosticCode::JinjaUndefinedValue,
        TemplateErrorKind::Render(_) => DiagnosticCode::JinjaRenderError,
        TemplateErrorKind::NotFound { .. } => DiagnosticCode::JinjaTemplateNotFound,
        TemplateErrorKind::WhitespaceControl => DiagnosticCode::JinjaWhitespaceControl,
        TemplateErrorKind::DynamicName(_) => DiagnosticCode::JinjaDynamicTemplateName,
        TemplateErrorKind::RecursiveInclude(_) => DiagnosticCode::JinjaRecursiveInclude,
    };
    let message = error.to_string();
    match error.line.and_then(|line| u32::try_from(line).ok()) {
        // The span covers the line as a whole: the text it is measured against
        // is the *unrendered* template, which this function does not hold, and
        // no column within a line somebody is about to be pointed at means
        // anything anyway.
        Some(line) => Diagnostic::at(
            code,
            message,
            Some(Span::whole_line(line, "").with_file(file)),
        ),
        None => Diagnostic::without_span(code, message),
    }
}

/// Reads a Jinja template through the loader this parse was given.
///
/// The adapter is the whole reason `{% include %}` and `.. include::` cannot
/// disagree about *what* a file is: both resolve through the same
/// [`ParseFileLoader`](crate::ParseFileLoader), so a missing one is refused in
/// one voice — the loader's, which names the Bazel attribute it belongs in.
///
/// Where they differ is what a name is relative to, and that difference is
/// deliberate. A `.. include::` resolves against the file it is written in,
/// as docutils does. A template name resolves against the **source root**, as
/// a Jinja environment's loader does — the `FileSystemLoader` a `conf.py`
/// builds is rooted at the Sphinx source directory, so a project's shared
/// header is named the same way from every document, however deep. Prefixing
/// `/` is how that is said to the loader, whose leading-slash convention
/// already means exactly this.
struct ParseFileTemplates<'a, 'b>(&'a ParseCtx<'b>);

impl TemplateLoader for ParseFileTemplates<'_, '_> {
    fn load(&self, path: &str, _relative_to: Option<&str>) -> Result<LoadedTemplate, String> {
        let from_root = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        self.0
            .files
            .load(&from_root, None)
            .map(|file| LoadedTemplate {
                id: file.id,
                text: file.text,
            })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rinx_ast::{Document, Domain, InlineNode, Node};

    use super::*;
    use crate::context::ParseCtx;
    use crate::{LoadedFile, ParseFileLoader, parse_with_ctx};

    /// A loader resolving a source-root-relative path to itself, so these
    /// tests exercise templating rather than the worker's path arithmetic.
    struct FakeFiles(HashMap<String, String>);

    impl FakeFiles {
        fn with(files: &[(&str, &str)]) -> Self {
            Self(
                files
                    .iter()
                    .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
                    .collect(),
            )
        }
    }

    impl ParseFileLoader for FakeFiles {
        fn load(&self, path: &str, _relative_to: Option<&str>) -> Result<LoadedFile, String> {
            let path = path.strip_prefix('/').unwrap_or(path);
            self.0
                .get(path)
                .map(|text| LoadedFile {
                    id: path.to_string(),
                    text: text.clone(),
                })
                .ok_or_else(|| format!("cannot read '{path}': no such file"))
        }
    }

    /// Parses `rst` with the Jinja pass switched on.
    fn parse_templated(files: &[(&str, &str)], rst: &str) -> Document {
        let loader = FakeFiles::with(files);
        let context = [("release".to_string(), "3.14".to_string())];
        let ctx = ParseCtx::new(Domain::Py, &loader).with_jinja(&context);
        parse_with_ctx("guide.rst", rst, &ctx)
    }

    /// The text of every paragraph in `nodes`.
    fn paragraphs(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .filter_map(|node| match node {
                Node::Paragraph(inline) => Some(
                    inline
                        .iter()
                        .filter_map(|part| match part {
                            InlineNode::Text(text) => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_without_the_setting_a_jinja_line_stays_the_text_it_is() {
        // Given a document opening the way the sphinx-needs demo's do
        let rst = "{% set page=\"index.rst\" %}\n\nBody\n";
        let loader = FakeFiles::with(&[]);
        let ctx = ParseCtx::new(Domain::Py, &loader);

        // When
        let document = parse_with_ctx("guide.rst", rst, &ctx);

        // Then it is parsed as the prose it literally is, and nothing is read
        assert_eq!(
            paragraphs(&document.nodes),
            vec![
                "{% set page=\"index.rst\" %}".to_string(),
                "Body".to_string()
            ]
        );
        assert!(document.diagnostics.is_empty());
    }

    #[test]
    fn test_an_include_splices_the_templates_prose_into_the_document() {
        // Given
        let rst = "{% set page=\"index.rst\" %}\n{% include \"header.rst\" %}\n\nBody\n";
        let header = "Source of {{ page }}, release {{ release }}.\n";

        // When
        let document = parse_templated(&[("header.rst", header)], rst);

        // Then
        assert_eq!(
            paragraphs(&document.nodes),
            vec![
                "Source of index.rst, release 3.14.".to_string(),
                "Body".to_string()
            ]
        );
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
    }

    #[test]
    fn test_a_diagnostic_below_an_include_names_the_documents_own_line() {
        // Given a header long enough that an unmapped position would be wrong
        let rst = "{% include \"header.rst\" %}\n\n.. noqa: not.a.code\n";
        let header = "one\n\ntwo\n\nthree\n";

        // When
        let document = parse_templated(&[("header.rst", header)], rst);

        // Then the directive is reported on line 3 of the document, not line 7
        let span = document.diagnostics[0].span.expect("positioned");
        assert_eq!(span.start.line, 3);
        assert_eq!(span.file, None);
    }

    #[test]
    fn test_a_diagnostic_inside_a_template_names_that_template() {
        // Given
        let rst = "{% include \"header.rst\" %}\n\nBody\n";
        let header = "one\n\n.. noqa: not.a.code\n";

        // When
        let document = parse_templated(&[("header.rst", header)], rst);

        // Then
        let span = document.diagnostics[0].span.expect("positioned");
        assert_eq!(span.start.line, 3, "line 3 of the header");
        assert_eq!(
            document.span_path(Some(span)),
            Some("header.rst"),
            "and the header is the file it is measured in"
        );
    }

    #[test]
    fn test_a_missing_template_is_reported_and_the_document_still_parses() {
        // Given
        let rst = "{% include \"header.rst\" %}\n\nBody\n";

        // When
        let document = parse_templated(&[], rst);

        // Then
        assert_eq!(
            document.diagnostics[0].code,
            DiagnosticCode::JinjaTemplateNotFound
        );
        assert!(
            document.diagnostics[0]
                .message
                .contains("cannot read 'header.rst'"),
            "the loader's own explanation is passed through: {}",
            document.diagnostics[0].message
        );
        assert!(
            paragraphs(&document.nodes).contains(&"Body".to_string()),
            "the page is still built from the unrendered text"
        );
    }

    #[test]
    fn test_a_refused_construct_is_reported_on_the_line_it_was_written_on() {
        // Given
        let rst = "Body\n\n{%- set page=\"index.rst\" %}\n";

        // When
        let document = parse_templated(&[], rst);

        // Then
        assert_eq!(
            document.diagnostics[0].code,
            DiagnosticCode::JinjaWhitespaceControl
        );
        assert_eq!(
            document.diagnostics[0].span.expect("positioned").start.line,
            3
        );
    }

    #[test]
    fn test_written_at_falls_back_to_the_line_itself_past_the_end() {
        // Given
        let map = TemplateMap {
            lines: vec![WrittenLine {
                line: 9,
                file: None,
            }],
        };

        // When / Then
        assert_eq!(
            map.written_at(1),
            WrittenLine {
                line: 9,
                file: None
            }
        );
        assert_eq!(
            map.written_at(4),
            WrittenLine {
                line: 4,
                file: None
            },
            "a position past the text maps to itself"
        );
    }
}
