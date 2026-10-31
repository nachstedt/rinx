//! What a Bazel build declared, and what a document fails on without it
//! (ADR-038 §5, roadmap #19).
//!
//! A `rinx_library` declares the documents its toctrees may name (its own and
//! its `deps`'), whether it compiles diagrams, and the images and downloads
//! its site bundles. The build fails on a document using anything else. The
//! checks are the build's own — [`rinx_toctree::undeclared_toctree_entries`],
//! [`rinx_analyzer::undeclared_assets`] and
//! [`rinx_analyzer::disabled_diagrams`] — so the editor finds exactly what
//! fails it, and nothing for a project the build does not describe.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rinx_analyzer::{DeclaredFiles, disabled_diagrams, undeclared_assets};
use rinx_ast::{Diagnostic, Document};
use rinx_toctree::{DeclaredDocuments, undeclared_toctree_entries};

/// What one library's build declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildDeclarations {
    /// The documents its toctrees may name: its own, and its `deps`'.
    pub toctree_documents: DeclaredDocuments,
    /// Whether it compiles diagrams — `diagrams = True`.
    pub diagrams: bool,
    /// What its site bundles, shared by every library of the site.
    pub bundled: Arc<BundledFiles>,
}

/// The files a site bundles: every library's `images` and `downloads`, as
/// paths from the source root.
///
/// The union over the site, as the build's bundle directories are: a
/// document may show an image another library of its site declared.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BundledFiles {
    pub images: BTreeSet<PathBuf>,
    pub downloads: BTreeSet<PathBuf>,
}

impl DeclaredFiles for BundledFiles {
    fn has_image(&self, path: &Path) -> bool {
        self.images.contains(path)
    }

    fn has_download(&self, path: &Path) -> bool {
        self.downloads.contains(path)
    }
}

/// What the build fails on in `document`, against what its library
/// declared: toctree entries outside its `deps`, diagrams without
/// `diagrams = True`, images and downloads its site does not bundle.
#[must_use]
pub fn undeclared_uses(document: &Document, declared: &BuildDeclarations) -> Vec<Diagnostic> {
    let mut found = undeclared_toctree_entries(document, &declared.toctree_documents);
    if !declared.diagrams {
        found.extend(disabled_diagrams(document));
    }
    found.extend(undeclared_assets(document, declared.bundled.as_ref()));
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::DiagnosticCode;

    /// What a library declaring `documents` and bundling `images` declared,
    /// diagrams `enabled` or not.
    fn declaring(documents: &[&str], images: &[&str], enabled: bool) -> BuildDeclarations {
        BuildDeclarations {
            toctree_documents: DeclaredDocuments::from_names(documents.iter().copied()),
            diagrams: enabled,
            bundled: Arc::new(BundledFiles {
                images: images.iter().map(PathBuf::from).collect(),
                downloads: BTreeSet::new(),
            }),
        }
    }

    fn codes(found: &[Diagnostic]) -> Vec<DiagnosticCode> {
        found.iter().map(|diagnostic| diagnostic.code).collect()
    }

    const USING_EVERYTHING: &str = "\
.. toctree::

   other

.. image:: logo.png

.. plantuml::

   A -> B
";

    #[test]
    fn test_undeclared_uses_finds_nothing_when_everything_is_declared() {
        // Given
        let document = rinx_parser::parse(
            "index.rst",
            USING_EVERYTHING,
            &rinx_parser::ParseCtx::default(),
        )
        .document;
        let declared = declaring(&["index", "other"], &["logo.png"], true);

        // When
        let found = undeclared_uses(&document, &declared);

        // Then
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn test_undeclared_uses_finds_each_undeclared_use() {
        // Given
        let document = rinx_parser::parse(
            "index.rst",
            USING_EVERYTHING,
            &rinx_parser::ParseCtx::default(),
        )
        .document;
        let declared = declaring(&["index"], &[], false);

        // When
        let found = undeclared_uses(&document, &declared);

        // Then
        assert_eq!(
            codes(&found),
            [
                DiagnosticCode::ToctreeUndeclaredDependency,
                DiagnosticCode::UmlDiagramsDisabled,
                DiagnosticCode::ImageUndeclared,
            ]
        );
    }

    #[test]
    fn test_bundled_files_tell_an_image_from_a_download() {
        // Given
        let bundled = BundledFiles {
            images: BTreeSet::from([PathBuf::from("a.png")]),
            downloads: BTreeSet::from([PathBuf::from("b.zip")]),
        };

        // When / Then
        assert!(bundled.has_image(Path::new("a.png")));
        assert!(!bundled.has_download(Path::new("a.png")));
        assert!(bundled.has_download(Path::new("b.zip")));
        assert!(!bundled.has_image(Path::new("b.zip")));
    }
}
