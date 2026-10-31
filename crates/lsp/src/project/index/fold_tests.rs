//! Folding a project's documents: what the index keeps and forgets as they
//! change, and what the fold finds about each document.

use super::test_support::*;
use super::*;
use rinx_ast::TargetName;

#[test]
fn test_project_index_forgets_a_label_its_document_no_longer_defines() {
    // Given
    let mut folder = folder();
    folder.record("a.rst".to_string(), labelled("a.rst", "setup"));
    assert!(
        folder
            .project_index()
            .targets
            .contains_key(&TargetName::new("setup"))
    );

    // When — the label is renamed in the editor.
    folder.record("a.rst".to_string(), labelled("a.rst", "install"));

    // Then
    let index = folder.project_index();
    assert!(!index.targets.contains_key(&TargetName::new("setup")));
    assert!(index.targets.contains_key(&TargetName::new("install")));
}

#[test]
fn test_record_scanned_keeps_a_more_recent_analysis() {
    // Given — `a.rst` analysed from its open buffer during the scan.
    let mut folder = folder();
    folder.record("a.rst".to_string(), labelled("a.rst", "edited"));

    // When
    folder.record_scanned([
        ("a.rst".to_string(), labelled("a.rst", "saved")),
        ("b.rst".to_string(), labelled("b.rst", "other")),
    ]);

    // Then
    let index = folder.project_index();
    assert!(index.targets.contains_key(&TargetName::new("edited")));
    assert!(!index.targets.contains_key(&TargetName::new("saved")));
    assert!(index.targets.contains_key(&TargetName::new("other")));
    assert_eq!(folder.document_count(), 2);
}

#[test]
fn test_generation_changes_only_when_a_document_changes() {
    // Given
    let mut folder = folder();
    let start = folder.generation();

    // When — a document is recorded, then recorded again unchanged
    folder.record("a.rst".to_string(), labelled("a.rst", "setup"));
    let after_change = folder.generation();
    folder.record("a.rst".to_string(), labelled("a.rst", "setup"));

    // Then
    assert_ne!(after_change, start);
    assert_eq!(folder.generation(), after_change);
}

#[test]
fn test_generation_changes_when_a_scan_brings_a_new_document() {
    // Given — `a.rst` already recorded from its open buffer
    let mut folder = folder();
    folder.record("a.rst".to_string(), labelled("a.rst", "edited"));
    let before = folder.generation();

    // When — the scan brings only `a.rst`, which is kept, then `b.rst`
    folder.record_scanned([("a.rst".to_string(), labelled("a.rst", "saved"))]);
    let after_known = folder.generation();
    folder.record_scanned([("b.rst".to_string(), labelled("b.rst", "other"))]);

    // Then
    assert_eq!(after_known, before);
    assert_ne!(folder.generation(), before);
}

#[test]
fn test_forget_drops_a_document_from_the_index() {
    // Given
    let mut folder = folder();
    folder.record("a.rst".to_string(), labelled("a.rst", "setup"));
    let before = folder.generation();

    // When
    folder.forget("a.rst");

    // Then
    assert!(folder.project_index().targets.is_empty());
    assert_eq!(folder.document_count(), 0);
    assert_ne!(folder.generation(), before);
}

#[test]
fn test_forget_of_an_unknown_document_changes_nothing() {
    // Given
    let mut folder = folder();
    let before = folder.generation();

    // When
    folder.forget("a.rst");

    // Then
    assert_eq!(folder.generation(), before);
}

#[test]
fn test_record_scanned_does_not_bring_back_a_forgotten_document() {
    // Given — `a.rst` deleted while the scan, which had read it, ran
    let mut folder = folder();
    folder.forget("a.rst");

    // When
    folder.record_scanned([
        ("a.rst".to_string(), labelled("a.rst", "stale")),
        ("b.rst".to_string(), labelled("b.rst", "other")),
    ]);

    // Then
    let index = folder.project_index();
    assert!(!index.targets.contains_key(&TargetName::new("stale")));
    assert!(index.targets.contains_key(&TargetName::new("other")));
}

#[test]
fn test_record_after_forget_lets_the_scan_keep_its_rule() {
    // Given — `a.rst` deleted, then created again, during the scan
    let mut folder = folder();
    folder.forget("a.rst");
    folder.record("a.rst".to_string(), labelled("a.rst", "recreated"));

    // When
    folder.record_scanned([("a.rst".to_string(), labelled("a.rst", "stale"))]);

    // Then — the newer analysis stays
    let index = folder.project_index();
    assert!(index.targets.contains_key(&TargetName::new("recreated")));
    assert!(!index.targets.contains_key(&TargetName::new("stale")));
}

#[test]
fn test_documents_under_lists_a_directory_and_a_document() {
    // Given
    let mut folder = folder();
    folder.record("guide/a.rst".to_string(), labelled("guide/a.rst", "a"));
    folder.record("guide/b.rst".to_string(), labelled("guide/b.rst", "b"));
    folder.record("guides.rst".to_string(), labelled("guides.rst", "c"));

    // When / Then — `guides.rst` is not under `guide`
    assert_eq!(
        folder.documents_under(Path::new("/work/docs/guide")),
        ["guide/a.rst", "guide/b.rst"]
    );
    assert_eq!(
        folder.documents_under(Path::new("/work/docs/guides.rst")),
        ["guides.rst"]
    );
}

#[test]
fn test_includers_of_lists_the_documents_that_read_a_file() {
    // Given
    let mut folder = folder();
    let fragment = PathBuf::from("/work/docs/_shared/note.rst");
    let mut includer = labelled("a.rst", "a");
    includer.reads.insert(fragment.clone());
    folder.record("a.rst".to_string(), includer);
    folder.record("b.rst".to_string(), labelled("b.rst", "b"));

    // When / Then — by the file, and by the directory holding it
    assert_eq!(folder.includers_of(&fragment), ["a.rst"]);
    assert_eq!(
        folder.includers_of(Path::new("/work/docs/_shared")),
        ["a.rst"]
    );
}

/// The analysis of a document named `doc_path` whose text is `text`.
fn written(doc_path: &str, text: &str) -> IndexedDocument {
    let parsed = rinx_parser::parse(doc_path, text, &rinx_parser::ParseCtx::default());
    IndexedDocument::of(
        Path::new("/work/docs").join(doc_path),
        &parsed.document,
        &parsed.source_map,
        BTreeSet::new(),
    )
}

/// The codes the fold found about the document `doc_path`.
fn index_codes(project: &mut Project, doc_path: &str) -> Vec<&'static str> {
    project
        .index_diagnostics(doc_path)
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

#[test]
fn test_index_diagnostics_reports_what_only_the_whole_project_reveals() {
    // Given a toctree naming a missing document, and a label two documents
    // define
    let mut folder = folder();
    folder.record(
        "index.rst".to_string(),
        written("index.rst", "Home\n====\n\n.. toctree::\n\n   a\n   gone\n"),
    );
    folder.record("a.rst".to_string(), labelled("a.rst", "setup"));
    folder.record("b.rst".to_string(), labelled("b.rst", "setup"));

    // When / Then — each finding is about the document it names
    assert_eq!(
        index_codes(&mut folder, "index.rst"),
        ["toctree.missing-document"]
    );
    assert_eq!(index_codes(&mut folder, "a.rst"), ["target.duplicate-name"]);
    assert_eq!(
        index_codes(&mut folder, "b.rst"),
        ["toctree.orphan-document", "target.duplicate-name"]
    );
    assert!(folder.index_diagnostics("absent.rst").is_empty());
}

#[test]
fn test_index_diagnostics_are_folded_again_when_a_document_changes() {
    // Given
    let mut folder = folder();
    folder.record(
        "index.rst".to_string(),
        written("index.rst", "Home\n====\n\n.. toctree::\n\n   gone\n"),
    );
    assert_eq!(
        index_codes(&mut folder, "index.rst"),
        ["toctree.missing-document"]
    );

    // When — the missing document is written
    folder.record("gone.rst".to_string(), written("gone.rst", "Gone\n====\n"));

    // Then
    assert!(folder.index_diagnostics("index.rst").is_empty());
}
