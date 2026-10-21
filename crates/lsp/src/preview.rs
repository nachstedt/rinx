//! The preview: one document's finished page, as the build would write it,
//! rendered against the project's current index (roadmap #22).
//!
//! The editor asks with a `rinx/preview` request rather than starting a
//! process per edit, so the page reads every document's latest parse: a
//! title changed in another file shows in the sidebar at once, without a
//! build. The page is assembled by `rinx_renderer::render_document_page`,
//! the build's own render action's call, so the two cannot differ but in the
//! stylesheet's address.
//!
//! A `rinx_site` names its template, stylesheet and entity templates
//! ([`Presentation`]); every other project has the build's defaults, embedded
//! here. Each file is read when the page is rendered, so an edit to one shows
//! on the next preview.
//!
//! A webview loads neither a `file:` address nor one relative to a page that
//! was never written, so the stylesheet is linked under the `resourceRoot`
//! the client serves the file system under, or else embedded as a `data:`
//! address.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use lsp_types::TextDocumentIdentifier;
use rinx_analyzer::{DocumentAnalysis, IndexSettings, build_project_index_from_analyses};
use rinx_ast::Document;
use rinx_entity::EntitySchema;
use rinx_index::ProjectIndex;
use rinx_renderer::config::SiteConfig;
use serde::{Deserialize, Serialize};

use crate::project::{BundledFiles, DEFAULT_ROOT_DOC, PageFile, Presentation};
use crate::render::embeddable_images;
use crate::uri::file_uri;

/// The request rendering a document's page.
pub const PREVIEW_METHOD: &str = "rinx/preview";

/// What a `rinx/preview` request asks for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewParams {
    /// The open document whose page is wanted.
    pub text_document: TextDocumentIdentifier,
    /// The address the client serves the file system's root under, such as
    /// a VS Code webview's `asWebviewUri(file:///)`; a file's address is this
    /// followed by its path. Without one, the stylesheet is embedded.
    #[serde(default)]
    pub resource_root: Option<String>,
}

/// The answer to a `rinx/preview` request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PreviewResult {
    /// The page, a complete HTML document.
    pub html: String,
}

/// The build's default page template, which a `rinx_site` naming none uses.
const BUILTIN_TEMPLATE: &str = include_str!("../theme/default.html");

/// The build's default stylesheet.
const BUILTIN_STYLESHEET: &str = include_str!("../theme/default.css");

/// How a project's pages are rendered.
pub struct PreviewSettings<'a> {
    pub config: &'a SiteConfig,
    pub schema: &'a EntitySchema,
    pub presentation: &'a Presentation,
    /// The images the site bundles, when the build declares them.
    pub bundled: Option<&'a BundledFiles>,
    /// See [`PreviewParams::resource_root`].
    pub resource_root: Option<&'a str>,
}

/// Why a page could not be rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewError {
    /// A file of the project's [`Presentation`] could not be read.
    Unreadable { path: PathBuf, error: String },
    /// The page template could not be parsed or rendered.
    Template(String),
}

impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable { path, error } => {
                write!(f, "cannot read `{}`: {error}", path.display())
            }
            Self::Template(error) => write!(f, "cannot render the page template: {error}"),
        }
    }
}

/// Why the server rendered no page for a `rinx/preview` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewRefusal {
    /// The document is not open, so the server holds no text of it.
    NotOpen,
    /// Rendering failed.
    Failed(PreviewError),
}

impl std::fmt::Display for PreviewRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOpen => f.write_str("the document is not open"),
            Self::Failed(error) => error.fmt(f),
        }
    }
}

/// The page of `document` against `index`, as `settings` say, reading each
/// file of the presentation with `read`.
///
/// # Errors
///
/// A [`PreviewError`] when a file cannot be read or the template fails.
pub fn render_preview(
    document: &Document,
    index: &ProjectIndex,
    settings: &PreviewSettings<'_>,
    read: &dyn Fn(&Path) -> io::Result<String>,
) -> Result<String, PreviewError> {
    let read_file = |path: &Path| {
        read(path).map_err(|error| PreviewError::Unreadable {
            path: path.to_path_buf(),
            error: error.to_string(),
        })
    };
    let template = match &settings.presentation.template {
        PageFile::Builtin => BUILTIN_TEMPLATE.to_string(),
        PageFile::File(path) => read_file(path)?,
    };
    let entity_templates = rinx_renderer::load_entity_templates(
        settings
            .presentation
            .entity_templates
            .iter()
            .map(PathBuf::as_path),
        read,
    )
    .map_err(|(path, error)| PreviewError::Unreadable {
        path,
        error: error.to_string(),
    })?;
    let output = rinx_renderer::render_with_assets(
        document,
        index,
        &document.path,
        settings.config,
        &embeddable_images(settings.bundled),
        settings.schema,
        &entity_templates,
    );
    let css_path = stylesheet_href(
        &settings.presentation.stylesheet,
        settings.resource_root,
        &read_file,
    )?;
    rinx_renderer::render_document_page(
        &output.html,
        document,
        index,
        &document.path,
        settings.config,
        &template,
        &css_path,
    )
    .map_err(|error| PreviewError::Template(format!("{error:#}")))
}

/// The page of `document` as the only document of a project with the
/// build's defaults — for a document in no project.
///
/// # Errors
///
/// A [`PreviewError`] when the template fails.
pub fn render_preview_alone(
    document: &Document,
    resource_root: Option<&str>,
    read: &dyn Fn(&Path) -> io::Result<String>,
) -> Result<String, PreviewError> {
    let analyses = BTreeMap::from([(document.path.clone(), DocumentAnalysis::of(document))]);
    let schema = EntitySchema::empty();
    let index = build_project_index_from_analyses(
        &analyses,
        &IndexSettings::new(DEFAULT_ROOT_DOC),
        &schema,
    )
    .index;
    let settings = PreviewSettings {
        config: &SiteConfig::default(),
        schema: &schema,
        presentation: &Presentation::default(),
        bundled: None,
        resource_root,
    };
    render_preview(document, &index, &settings, read)
}

/// Where the page finds `stylesheet`: under `resource_root` when it is a
/// file the client serves, else embedded as a `data:` address.
fn stylesheet_href(
    stylesheet: &PageFile,
    resource_root: Option<&str>,
    read_file: &dyn Fn(&Path) -> Result<String, PreviewError>,
) -> Result<String, PreviewError> {
    let text = match (stylesheet, resource_root) {
        (PageFile::File(path), Some(root)) => {
            if let Some(href) = resource_href(root, path) {
                return Ok(href);
            }
            read_file(path)?
        }
        (PageFile::File(path), None) => read_file(path)?,
        (PageFile::Builtin, _) => BUILTIN_STYLESHEET.to_string(),
    };
    Ok(format!("data:text/css;base64,{}", BASE64.encode(text)))
}

/// The address of the file at `path` under `root`, its path encoded as in a
/// `file:` URI — or `None` for a path no URI spells.
fn resource_href(root: &str, path: &Path) -> Option<String> {
    let uri = file_uri(path)?;
    Some(format!("{}{}", root.trim_end_matches('/'), uri.path()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCUMENT: &str = "Guide\n=====\n\nSee :doc:`setup`.\n";
    const SETUP: &str = "Setting up\n==========\n";

    /// The index the documents `(path, text)` fold into.
    fn index_of(documents: &[(&str, &str)]) -> ProjectIndex {
        let analyses = documents
            .iter()
            .map(|(path, text)| {
                let document = rinx_parser::parse(path, text);
                ((*path).to_string(), DocumentAnalysis::of(&document))
            })
            .collect();
        build_project_index_from_analyses(
            &analyses,
            &IndexSettings::new("index"),
            &EntitySchema::empty(),
        )
        .index
    }

    /// A reader serving `files` (path, text), and nothing else.
    fn disk(files: &[(&str, &str)]) -> impl Fn(&Path) -> io::Result<String> + use<> {
        let files: Vec<(PathBuf, String)> = files
            .iter()
            .map(|(path, text)| (PathBuf::from(path), (*text).to_string()))
            .collect();
        move |path: &Path| {
            files
                .iter()
                .find(|(file, _)| file == path)
                .map(|(_, text)| text.clone())
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not found"))
        }
    }

    /// The page of `index.rst` holding `DOCUMENT` beside `setup.rst`,
    /// presented as `presentation` and reading `files`.
    fn preview(
        presentation: &Presentation,
        resource_root: Option<&str>,
        files: &[(&str, &str)],
    ) -> Result<String, PreviewError> {
        let document = rinx_parser::parse("index.rst", DOCUMENT);
        let index = index_of(&[("index.rst", DOCUMENT), ("setup.rst", SETUP)]);
        let settings = PreviewSettings {
            config: &SiteConfig::default(),
            schema: &EntitySchema::empty(),
            presentation,
            bundled: None,
            resource_root,
        };
        render_preview(&document, &index, &settings, &disk(files))
    }

    #[test]
    fn test_render_preview_draws_the_page_with_another_documents_title() {
        // Given / When
        let page = preview(&Presentation::default(), None, &[]).unwrap();

        // Then — the default template, the page's own title, and the link
        // text the index gives `setup`
        assert!(page.starts_with("<!DOCTYPE html>"), "{page}");
        assert!(page.contains("<title>Guide"), "{page}");
        assert!(page.contains("Setting up</span></a>"), "{page}");
    }

    #[test]
    fn test_render_preview_embeds_the_builtin_stylesheet() {
        // Given / When
        let page = preview(&Presentation::default(), Some("https://webview/"), &[]).unwrap();

        // Then
        let embedded = format!(
            "href=\"data:text/css;base64,{}\"",
            BASE64.encode(BUILTIN_STYLESHEET)
        );
        assert!(page.contains(&embedded));
    }

    #[test]
    fn test_render_preview_uses_the_sites_template() {
        // Given
        let presentation = Presentation {
            template: PageFile::File(PathBuf::from("/work/page.html")),
            ..Presentation::default()
        };
        let template = "<h1>{{ page_title }}</h1>{{ body }}";

        // When
        let page = preview(&presentation, None, &[("/work/page.html", template)]).unwrap();

        // Then
        assert!(page.starts_with("<h1>Guide</h1>"), "{page}");
    }

    #[test]
    fn test_render_preview_links_the_sites_stylesheet_under_the_resource_root() {
        // Given
        let presentation = Presentation {
            template: PageFile::File(PathBuf::from("/work/page.html")),
            stylesheet: PageFile::File(PathBuf::from("/work/my site.css")),
            ..Presentation::default()
        };

        // When
        let page = preview(
            &presentation,
            Some("https://webview/"),
            &[("/work/page.html", "{{ css_path }}")],
        )
        .unwrap();

        // Then
        assert_eq!(page, "https://webview/work/my%20site.css");
    }

    #[test]
    fn test_render_preview_embeds_the_sites_stylesheet_without_a_resource_root() {
        // Given
        let presentation = Presentation {
            template: PageFile::File(PathBuf::from("/work/page.html")),
            stylesheet: PageFile::File(PathBuf::from("/work/site.css")),
            ..Presentation::default()
        };

        // When
        let page = preview(
            &presentation,
            None,
            &[
                ("/work/page.html", "{{ css_path }}"),
                ("/work/site.css", "p {}"),
            ],
        )
        .unwrap();

        // Then
        assert_eq!(
            page,
            format!("data:text/css;base64,{}", BASE64.encode("p {}"))
        );
    }

    #[test]
    fn test_render_preview_alone_draws_a_document_with_the_defaults() {
        // Given — a document whose `:doc:` names one no project holds
        let document = rinx_parser::parse("notes.rst", DOCUMENT);

        // When
        let page = render_preview_alone(&document, None, &disk(&[])).unwrap();

        // Then
        assert!(page.contains("<title>Guide"), "{page}");
        assert!(!page.contains("Setting up"), "{page}");
    }

    #[test]
    fn test_render_preview_names_a_file_it_cannot_read() {
        // Given
        let presentation = Presentation {
            template: PageFile::File(PathBuf::from("/work/gone.html")),
            ..Presentation::default()
        };

        // When
        let error = preview(&presentation, None, &[]).unwrap_err();

        // Then
        assert_eq!(
            error.to_string(),
            "cannot read `/work/gone.html`: not found"
        );
    }

    #[test]
    fn test_render_preview_names_an_entity_template_it_cannot_read() {
        // Given
        let presentation = Presentation {
            entity_templates: vec![PathBuf::from("/work/req.html")],
            ..Presentation::default()
        };

        // When
        let error = preview(&presentation, None, &[]).unwrap_err();

        // Then
        assert_eq!(
            error,
            PreviewError::Unreadable {
                path: PathBuf::from("/work/req.html"),
                error: "not found".to_string()
            }
        );
    }

    #[test]
    fn test_render_preview_reports_a_broken_template() {
        // Given
        let presentation = Presentation {
            template: PageFile::File(PathBuf::from("/work/page.html")),
            ..Presentation::default()
        };

        // When
        let error = preview(&presentation, None, &[("/work/page.html", "{% if %}")]).unwrap_err();

        // Then
        assert!(matches!(error, PreviewError::Template(_)), "{error}");
    }
}
