//! Whether a document uses only what its Bazel build declared.
//!
//! The parser is resilient and unaware of build configuration: it parses a
//! diagram or an image whether or not anything will compile or bundle it. The
//! build is strict, and fails on what a library left out — `diagrams = True`,
//! an `images` or a `downloads` entry. The worker's `parse` and
//! `validate_assets` actions and the language server both ask these
//! functions, so the editor underlines exactly what fails the build.
//!
//! None of these is subject to `.. noqa:`: each is a fault of the build file,
//! which no comment in a document should be able to hide.

use std::path::{Path, PathBuf};

use rinx_ast::{
    Diagnostic, DiagnosticCode, Directive, Document, ImageOptions, InlineNode, Node, Span,
    SubstitutionKind, for_each_inline_list, walk_nodes,
};

/// The files a site bundles, as paths from the source root — what
/// [`rinx_ast::AssetUri::resolve`] yields.
///
/// A trait because the two callers know it differently: the worker by what
/// reached the site's bundle directories, the language server by what the
/// manifest's libraries declare.
pub trait DeclaredFiles {
    /// Whether the site bundles `path` into `_images/`.
    fn has_image(&self, path: &Path) -> bool;
    /// Whether the site bundles `path` into `_downloads/`.
    fn has_download(&self, path: &Path) -> bool;
}

/// One error per picture and per linked `:download:` in `doc` naming a
/// project file `declared` does not hold, at the construct.
///
/// External URLs and the `!` form of `:download:` name nothing to bundle and
/// are skipped. A role a `replace::` substitution copies into several places
/// is reported once, where it was written.
#[must_use]
pub fn undeclared_assets(doc: &Document, declared: &dyn DeclaredFiles) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    for (path, span) in image_paths(doc) {
        if !declared.has_image(&path) {
            found.push(Diagnostic::at(
                DiagnosticCode::ImageUndeclared,
                format!(
                    "image {} was not bundled — if this is a Bazel build, add it to the \
                     library's images attribute",
                    path.display()
                ),
                span,
            ));
        }
    }
    for (path, span) in download_paths(doc) {
        if !declared.has_download(&path) {
            found.push(Diagnostic::at(
                DiagnosticCode::DownloadUndeclared,
                format!(
                    "download file {} was not bundled — if this is a Bazel build, add it to the \
                     library's downloads attribute",
                    path.display()
                ),
                span,
            ));
        }
    }
    found
}

/// The project file each `.. image::`, `.. figure::` and `.. |name| image::`
/// in `doc` shows, with where it was written, once per place.
fn image_paths(doc: &Document) -> Vec<(PathBuf, Option<Span>)> {
    let mut paths = Vec::new();
    let mut record = |options: &ImageOptions, fallback: Option<Span>| {
        if let Some(resolved) = options.uri.resolve(&doc.path) {
            let entry = (resolved, options.span.or(fallback));
            if !paths.contains(&entry) {
                paths.push(entry);
            }
        }
    };
    walk_nodes(&doc.nodes, &mut |node| match node {
        Node::Directive(Directive::Image(options)) => record(options, None),
        Node::Directive(Directive::Figure(figure)) => record(&figure.image, None),
        // A `.. |name| image::` definition names a real image too, subject to
        // the same `images` attribute requirement, used or not.
        Node::Directive(Directive::SubstitutionDefinition(definition)) => {
            if let SubstitutionKind::Image(options) = &definition.kind {
                record(options, definition.span);
            }
        }
        _ => {}
    });
    paths
}

/// The project file each linked `:download:` in `doc` names, with where it
/// was written, once per place.
fn download_paths(doc: &Document) -> Vec<(PathBuf, Option<Span>)> {
    let mut paths = Vec::new();
    let mut record = |inlines: &Vec<InlineNode>| {
        for inline in inlines {
            if let InlineNode::DownloadReference {
                target,
                link: true,
                span,
                ..
            } = inline
                && let Some(resolved) = target.resolve(&doc.path)
            {
                let entry = (resolved, *span);
                if !paths.contains(&entry) {
                    paths.push(entry);
                }
            }
        }
    };
    for_each_inline_list(&doc.nodes, &mut record);
    // A `replace::` definition is no inline list of the document's, but its
    // roles are written there: reported even when unused, as an image
    // definition is.
    walk_nodes(&doc.nodes, &mut |node| {
        if let Node::Directive(Directive::SubstitutionDefinition(definition)) = node
            && let SubstitutionKind::Replace(inlines) = &definition.kind
        {
            record(inlines);
        }
    });
    paths
}

/// One error per diagram in `doc`, however deeply nested — what a library
/// without `diagrams = True` fails on, since nothing would compile the
/// picture its page names.
#[must_use]
pub fn disabled_diagrams(doc: &Document) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    walk_nodes(&doc.nodes, &mut |node| {
        let (directive, code, span) = match node {
            Node::Directive(Directive::Uml(uml)) => (
                uml.source.as_str(),
                DiagnosticCode::UmlDiagramsDisabled,
                uml.span,
            ),
            // A flowchart draws no less of a picture for having generated it,
            // so it needs the same opt-in — under its own code, because a code
            // names the construct.
            Node::Directive(Directive::EntityFlow(flow)) => (
                flow.source.as_str(),
                DiagnosticCode::EntityFlowDiagramsDisabled,
                flow.span,
            ),
            // A sequence diagram is generated like a flowchart, and needs the
            // opt-in for the flowchart's reason.
            Node::Directive(Directive::EntitySequence(sequence)) => (
                sequence.source.as_str(),
                DiagnosticCode::EntitySequenceDiagramsDisabled,
                sequence.span,
            ),
            _ => return,
        };
        found.push(Diagnostic::at(
            code,
            format!(
                "{directive}: this library does not compile diagrams; set `diagrams = True` on \
                 its rinx_library to enable them"
            ),
            span,
        ));
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{
        AdmonitionKind, AssetUri, EntityFlow, EntityFlowSource, EntityId, EntitySequence,
        EntitySequenceSource, Figure, ListItem, NonEmptyVector, Position, SubstitutionDefinition,
        Uml, UmlSource,
    };

    /// A site bundling exactly the images and downloads listed.
    struct Bundled {
        images: Vec<&'static str>,
        downloads: Vec<&'static str>,
    }

    impl DeclaredFiles for Bundled {
        fn has_image(&self, path: &Path) -> bool {
            self.images.iter().any(|image| Path::new(image) == path)
        }
        fn has_download(&self, path: &Path) -> bool {
            self.downloads.iter().any(|file| Path::new(file) == path)
        }
    }

    /// A site bundling nothing.
    fn nothing_bundled() -> Bundled {
        Bundled {
            images: Vec::new(),
            downloads: Vec::new(),
        }
    }

    fn span_on(line: u32) -> Span {
        Span::new(Position::new(line, 1), Position::new(line, 20))
    }

    /// An image of `uri` written on `line`.
    fn image(uri: &str, line: u32) -> ImageOptions {
        ImageOptions {
            span: Some(span_on(line)),
            ..ImageOptions::new(AssetUri::new(uri))
        }
    }

    /// A linked `:download:` of `target`, written on `line`.
    fn download(target: &str, line: u32) -> InlineNode {
        InlineNode::DownloadReference {
            display: None,
            target: AssetUri::new(target),
            link: true,
            span: Some(span_on(line)),
        }
    }

    fn document(path: &str, nodes: Vec<Node>) -> Document {
        Document::new(path.to_string(), nodes)
    }

    fn codes(found: &[Diagnostic]) -> Vec<DiagnosticCode> {
        found.iter().map(|diagnostic| diagnostic.code).collect()
    }

    fn lines(found: &[Diagnostic]) -> Vec<u32> {
        found
            .iter()
            .map(|diagnostic| diagnostic.span.expect("a position").start.line)
            .collect()
    }

    #[test]
    fn test_image_paths_resolve_against_the_document() {
        // Given
        let doc = document(
            "guide/intro.rst",
            vec![Node::Directive(Directive::Image(Box::new(image(
                "images/logo.png",
                2,
            ))))],
        );

        // When
        let paths = image_paths(&doc);

        // Then
        assert_eq!(
            paths,
            [(PathBuf::from("guide/images/logo.png"), Some(span_on(2)))]
        );
    }

    #[test]
    fn test_image_paths_skip_an_external_url() {
        // Given — nothing bundles it, so its absence is not a build failure
        let doc = document(
            "index.rst",
            vec![Node::Directive(Directive::Image(Box::new(image(
                "https://example.com/logo.png",
                1,
            ))))],
        );

        // When / Then
        assert!(image_paths(&doc).is_empty());
    }

    #[test]
    fn test_image_paths_find_a_figure_and_a_substitution_image() {
        // Given — a `.. |name| image::` names a real image too, used or not
        let doc = document(
            "index.rst",
            vec![
                Node::Directive(Directive::Figure(Box::new(Figure::new(image(
                    "logo.png", 1,
                ))))),
                Node::Directive(Directive::SubstitutionDefinition(SubstitutionDefinition {
                    name: "biohazard".to_string(),
                    kind: SubstitutionKind::Image(Box::new(ImageOptions::new(AssetUri::new(
                        "biohazard.png",
                    )))),
                    span: Some(span_on(5)),
                })),
            ],
        );

        // When
        let paths = image_paths(&doc);

        // Then — the definition's own line stands in for the image's
        assert_eq!(
            paths,
            [
                (PathBuf::from("logo.png"), Some(span_on(1))),
                (PathBuf::from("biohazard.png"), Some(span_on(5))),
            ]
        );
    }

    #[test]
    fn test_download_paths_find_a_role_nested_in_a_container() {
        // Given — a role inside a list item, which no block walk reaches
        let doc = document(
            "guide/intro.rst",
            vec![Node::BulletList {
                bullet: '-',
                items: vec![ListItem {
                    nodes: vec![Node::Paragraph(vec![download("../data/a.csv", 3)])],
                }],
            }],
        );

        // When
        let paths = download_paths(&doc);

        // Then
        assert_eq!(paths, [(PathBuf::from("data/a.csv"), Some(span_on(3)))]);
    }

    #[test]
    fn test_download_paths_skip_external_urls_and_the_bang_form() {
        // Given — neither names a file the site bundles
        let unlinked = InlineNode::DownloadReference {
            display: None,
            target: AssetUri::new("tool.py"),
            link: false,
            span: None,
        };
        let doc = document(
            "index.rst",
            vec![Node::Paragraph(vec![
                download("https://example.com/tool.zip", 1),
                unlinked,
            ])],
        );

        // When / Then
        assert!(download_paths(&doc).is_empty());
    }

    #[test]
    fn test_download_paths_report_a_substituted_role_once_where_it_was_written() {
        // Given — a `replace::` holding a role, copied into a paragraph by the
        // parser's substitution pass with the definition's span
        let doc = document(
            "index.rst",
            vec![
                Node::Directive(Directive::SubstitutionDefinition(SubstitutionDefinition {
                    name: "data".to_string(),
                    kind: SubstitutionKind::Replace(vec![download("a.csv", 1)]),
                    span: Some(span_on(1)),
                })),
                Node::Paragraph(vec![download("a.csv", 1)]),
                Node::Paragraph(vec![download("a.csv", 1)]),
            ],
        );

        // When
        let paths = download_paths(&doc);

        // Then
        assert_eq!(paths, [(PathBuf::from("a.csv"), Some(span_on(1)))]);
    }

    #[test]
    fn test_undeclared_assets_accepts_what_the_site_bundles() {
        // Given
        let doc = document(
            "index.rst",
            vec![
                Node::Directive(Directive::Image(Box::new(image("logo.png", 1)))),
                Node::Paragraph(vec![download("a.csv", 2)]),
            ],
        );
        let bundled = Bundled {
            images: vec!["logo.png"],
            downloads: vec!["a.csv"],
        };

        // When / Then
        assert!(undeclared_assets(&doc, &bundled).is_empty());
    }

    #[test]
    fn test_undeclared_assets_reports_each_at_its_construct() {
        // Given
        let doc = document(
            "guide/intro.rst",
            vec![
                Node::Directive(Directive::Image(Box::new(image("logo.png", 1)))),
                Node::Paragraph(vec![download("a.csv", 3)]),
            ],
        );

        // When
        let found = undeclared_assets(&doc, &nothing_bundled());

        // Then
        assert_eq!(
            codes(&found),
            [
                DiagnosticCode::ImageUndeclared,
                DiagnosticCode::DownloadUndeclared
            ]
        );
        assert_eq!(lines(&found), [1, 3]);
        assert!(found[0].message.contains("guide/logo.png"), "{found:?}");
        assert!(found[0].message.contains("images attribute"), "{found:?}");
        assert!(found[1].message.contains("guide/a.csv"), "{found:?}");
        assert!(
            found[1].message.contains("downloads attribute"),
            "{found:?}"
        );
    }

    #[test]
    fn test_undeclared_assets_does_not_take_an_image_for_a_download() {
        // Given — the same file, bundled only as a picture
        let doc = document(
            "index.rst",
            vec![Node::Paragraph(vec![download("a.png", 1)])],
        );
        let bundled = Bundled {
            images: vec!["a.png"],
            downloads: Vec::new(),
        };

        // When
        let found = undeclared_assets(&doc, &bundled);

        // Then
        assert_eq!(codes(&found), [DiagnosticCode::DownloadUndeclared]);
    }

    #[test]
    fn test_disabled_diagrams_reports_each_diagram_at_its_own_line() {
        // Given — one at top level and one nested in an admonition
        let uml = |source, line| {
            let mut uml = Uml::new(source, "A -> B".to_string());
            uml.span = Some(span_on(line));
            Node::Directive(Directive::Uml(Box::new(uml)))
        };
        let doc = document(
            "index.rst",
            vec![
                uml(UmlSource::PlantUml, 1),
                Node::Directive(Directive::Admonition {
                    kind: AdmonitionKind::Note,
                    title: None,
                    collapsible: None,
                    body: vec![uml(UmlSource::Uml, 7)],
                }),
            ],
        );

        // When
        let found = disabled_diagrams(&doc);

        // Then
        assert_eq!(lines(&found), [1, 7]);
        assert_eq!(
            codes(&found),
            [
                DiagnosticCode::UmlDiagramsDisabled,
                DiagnosticCode::UmlDiagramsDisabled
            ]
        );
        assert!(found[0].message.contains("diagrams = True"), "{found:?}");
    }

    #[test]
    fn test_disabled_diagrams_reports_generated_pictures_under_their_own_codes() {
        // Given — a flowchart and a sequence diagram are compiled by the same
        // action a written diagram is
        let sequence = EntitySequence::new(
            EntitySequenceSource::NeedSequence,
            NonEmptyVector::single(EntityId::new("COMP_UI").unwrap()),
            NonEmptyVector::single("sends".to_string()),
        );
        let doc = document(
            "index.rst",
            vec![
                Node::Directive(Directive::EntityFlow(Box::new(EntityFlow::new(
                    EntityFlowSource::EntityFlow,
                )))),
                Node::Directive(Directive::EntitySequence(Box::new(sequence))),
            ],
        );

        // When
        let found = disabled_diagrams(&doc);

        // Then
        assert_eq!(
            codes(&found),
            [
                DiagnosticCode::EntityFlowDiagramsDisabled,
                DiagnosticCode::EntitySequenceDiagramsDisabled
            ]
        );
        assert!(found[1].message.starts_with("needsequence:"), "{found:?}");
    }

    #[test]
    fn test_disabled_diagrams_finds_nothing_in_a_document_without_one() {
        // Given
        let doc = document("index.rst", vec![Node::Paragraph(Vec::new())]);

        // When / Then
        assert!(disabled_diagrams(&doc).is_empty());
    }
}
