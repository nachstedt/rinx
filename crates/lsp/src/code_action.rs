//! Quick fixes: what one click changes to make a finding go away (roadmap
//! #21).
//!
//! The one fix so far adds a missing dependency: a toctree entry naming a
//! document outside its library's `deps` fails the build, and the edit adding
//! the library listing that document to `deps` is computed here — which
//! library from the manifest ([`missing_deps`]), or from the build file of
//! the document's package for a library the site does not collect yet
//! ([`library_listing`]), and where in the writer's build file
//! ([`add_deps`]). Both may refuse, and an action is offered only when the
//! edit can be made, so an author never clicks a fix that does nothing.
//!
//! The edit is computed with the action, not on resolve: an editor asks for
//! actions whenever the cursor moves, so the file is read moments before the
//! click, and clients that cannot resolve an action get the edit as well.

use std::collections::HashMap;
use std::path::Path;

use lsp_types::{CodeAction, CodeActionKind, Range, TextEdit, Uri, WorkspaceEdit};
use rinx_ast::{Diagnostic, DiagnosticSubject};

use crate::diagnostics::{ParsedSource, to_lsp_diagnostic};
use crate::position::{PositionEncoding, to_lsp_range};
use crate::project::{
    Judgement, Membership, ProjectModel, add_deps, build_file_of, library_listing, missing_deps,
};
use crate::uri::file_uri;

/// The quick fixes for what `parsed` — a document of the project `model` —
/// fails the build on within `range`, reading a build file with `read`.
#[must_use]
pub fn quick_fixes(
    parsed: &ParsedSource,
    model: &ProjectModel,
    range: Range,
    encoding: PositionEncoding,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Vec<CodeAction> {
    let Membership::Listed(listing) = &model.settings.membership else {
        return Vec::new();
    };
    let Some(listed) = listing.get(&parsed.document.path) else {
        return Vec::new();
    };
    parsed
        .undeclared
        .iter()
        .filter_map(|diagnostic| {
            let span = diagnostic.span.filter(|span| span.file.is_none())?;
            let found = to_lsp_range(span, &parsed.text, encoding);
            if found.end < range.start || range.end < found.start {
                return None;
            }
            dependency_fix(
                diagnostic,
                found,
                parsed,
                model,
                listed.library,
                encoding,
                read,
            )
        })
        .collect()
}

/// The action adding to the `deps` of library `from` what `diagnostic`,
/// found at `found`, says its toctree entry needs — or `None` when no edit
/// would fix it. Positions in the build file count in `encoding`, as the
/// document's do.
fn dependency_fix(
    diagnostic: &Diagnostic,
    found: Range,
    parsed: &ParsedSource,
    model: &ProjectModel,
    from: usize,
    encoding: PositionEncoding,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Option<CodeAction> {
    let Some(DiagnosticSubject::UndeclaredToctreeEntry(needed)) = &diagnostic.subject else {
        return None;
    };
    let libraries = &model.settings.libraries;
    let outside = |document: &str| library_listing(&model.source_root, document, read);
    let labels = missing_deps(libraries, from, &parsed.document.path, needed, &outside).ok()?;
    let writer = &libraries[from].label;
    let build_file = build_file_of(&model.source_root, writer)?;
    let text = read(&build_file)?;
    let edits = add_deps(&text, writer, &labels).ok()?;
    let edits: Vec<TextEdit> = edits
        .into_iter()
        .map(|edit| TextEdit::new(to_lsp_range(edit.span, &text, encoding), edit.text))
        .collect();
    let uri = file_uri(&build_file)?;
    Some(add_dependency_action(
        to_lsp_diagnostic(diagnostic, &Judgement::as_failure(diagnostic), found),
        writer,
        &labels,
        uri,
        edits,
    ))
}

/// The action titled for adding `labels` to the `deps` of `writer`, fixing
/// `diagnostic` with `edits` of the build file at `build_file`.
fn add_dependency_action(
    diagnostic: lsp_types::Diagnostic,
    writer: &str,
    labels: &[String],
    build_file: Uri,
    edits: Vec<TextEdit>,
) -> CodeAction {
    CodeAction {
        title: format!("Add {} to the deps of {writer}", labels.join(", ")),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic]),
        edit: Some(WorkspaceEdit {
            changes: Some(HashMap::from([(build_file, edits)])),
            ..WorkspaceEdit::default()
        }),
        is_preferred: Some(true),
        ..CodeAction::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use lsp_types::Position;
    use rinx_toctree::{DeclaredDocuments, undeclared_toctree_entries};

    use crate::files::FileReads;
    use crate::project::{LibraryModel, Listing, ParseSettings, ProjectSettings, ProjectSource};

    const INDEX: &str = "Index\n=====\n\n.. toctree::\n\n   guide\n   ../api/ref\n";

    const BUILD: &str = "rinx_library(\n    name = \"root\",\n    srcs = [\"index.rst\"],\n    deps = [\":guide\"],\n)\n";

    /// `docs/index.rst` parsed as `//docs:root`'s, which declares only
    /// itself and `docs/guide.rst`.
    fn parsed() -> ParsedSource {
        let document =
            rinx_parser::parse("docs/index.rst", INDEX, &rinx_parser::ParseCtx::default()).document;
        let declared = DeclaredDocuments::from_names(["docs/index", "docs/guide"]);
        ParsedSource {
            undeclared: undeclared_toctree_entries(&document, &declared),
            document,
            source_map: rinx_ast::SourceMap::default(),
            reads: FileReads::default(),
            text: INDEX.to_string(),
        }
    }

    /// A site under `root` of `//docs:root`, `//docs:guide` and `//api:docs`.
    fn model(root: &Path) -> ProjectModel {
        let library = |label: &str, documents: &[&str], deps: &[&str]| LibraryModel {
            label: label.to_string(),
            deps: deps.iter().map(ToString::to_string).collect(),
            documents: documents.iter().map(ToString::to_string).collect(),
            parse: ParseSettings::default(),
        };
        let mut listing = Listing::default();
        for (index, doc_path) in ["docs/index.rst", "docs/guide.rst", "api/ref.rst"]
            .into_iter()
            .enumerate()
        {
            listing.insert(doc_path.to_string(), root.join(doc_path), index);
        }
        ProjectModel {
            source_root: root.to_path_buf(),
            source: ProjectSource::Bazel {
                site: "//docs:site".to_string(),
                manifest: root.join("site.lsp_manifest.json"),
            },
            settings: ProjectSettings {
                membership: Membership::Listed(listing),
                libraries: vec![
                    library("//docs:root", &["docs/index.rst"], &["//docs:guide"]),
                    library("//docs:guide", &["docs/guide.rst"], &[]),
                    library("//api:docs", &["api/ref.rst"], &[]),
                ],
                ..ProjectSettings::default()
            },
        }
    }

    /// A workspace whose `docs/BUILD.bazel` holds `build`.
    fn workspace(name: &str, build: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("rinx_code_action_{name}_{}", std::process::id()));
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("docs/BUILD.bazel"), build).unwrap();
        root
    }

    fn fixes(root: &Path, line: u32) -> Vec<CodeAction> {
        let at = Position::new(line, 4);
        quick_fixes(
            &parsed(),
            &model(root),
            Range::new(at, at),
            PositionEncoding::Utf16,
            &|path| std::fs::read_to_string(path).ok(),
        )
    }

    #[test]
    fn test_an_undeclared_entry_offers_adding_its_library_to_deps() {
        // Given
        let root = workspace("offers", BUILD);

        // When — the cursor on `../api/ref`, the toctree's second entry
        let fixes = fixes(&root, 6);
        let _ = std::fs::remove_dir_all(&root);

        // Then
        assert_eq!(fixes.len(), 1, "{fixes:?}");
        let fix = &fixes[0];
        assert_eq!(fix.title, "Add //api:docs to the deps of //docs:root");
        assert_eq!(fix.kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(fix.is_preferred, Some(true));
        let diagnostics = fix.diagnostics.as_deref().unwrap_or_default();
        assert_eq!(
            diagnostics[0].code,
            Some(lsp_types::NumberOrString::String(
                "toctree.undeclared-dependency".to_string()
            ))
        );
        let changes = fix.edit.as_ref().and_then(|edit| edit.changes.as_ref());
        let build_file = file_uri(&root.join("docs/BUILD.bazel")).unwrap();
        let edits = &changes.unwrap()[&build_file];
        assert_eq!(
            edits,
            &[TextEdit::new(
                Range::new(Position::new(3, 11), Position::new(3, 21)),
                "[\n        \":guide\",\n        \"//api:docs\",\n    ]".to_string()
            )]
        );
    }

    #[test]
    fn test_a_range_meeting_no_undeclared_entry_offers_nothing() {
        // Given
        let root = workspace("elsewhere", BUILD);

        // When — the cursor on the title
        let fixes = fixes(&root, 0);
        let _ = std::fs::remove_dir_all(&root);

        // Then
        assert_eq!(fixes, []);
    }

    #[test]
    fn test_a_build_file_that_cannot_be_edited_offers_nothing() {
        // Given — `deps` computed, so no edit is safe
        let root = workspace(
            "computed",
            "rinx_library(\n    name = \"root\",\n    deps = COMMON,\n)\n",
        );

        // When
        let fixes = fixes(&root, 6);
        let _ = std::fs::remove_dir_all(&root);

        // Then
        assert_eq!(fixes, []);
    }

    #[test]
    fn test_a_project_listing_no_documents_offers_nothing() {
        // Given — a folder of no Bazel site
        let root = workspace("folder", BUILD);
        let mut model = model(&root);
        model.settings.membership = Membership::Matched;

        // When
        let at = Position::new(6, 4);
        let fixes = quick_fixes(
            &parsed(),
            &model,
            Range::new(at, at),
            PositionEncoding::Utf16,
            &|path| std::fs::read_to_string(path).ok(),
        );
        let _ = std::fs::remove_dir_all(&root);

        // Then
        assert_eq!(fixes, []);
    }
}
