//! The editor's diagnostics against the build's, over this repository's own
//! documents: ADR-038 promises they are the same, and this is where that is
//! checked.
//!
//! Both sides run as the processes they are — `rinx parse` as a Bazel action
//! runs it, `rinx lsp` as an editor does — with the defaults both assume
//! today: the `py` domain, no entity schema, no Jinja. Project configuration
//! reaches the server only with workspace awareness (roadmap #4, #10, #19),
//! and from then on this test should parse each document with its library's
//! flags. One gap is known, relaxed here by name rather than by leaving
//! documents out, and closed by roadmap step #3: the server leaves out what it
//! finds inside an `.. include::`d fragment, so the build's warnings about
//! another file are not compared.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::lsp_client::Server;

/// One reported problem: 0-based line, 0-based character column, code.
type Finding = (u32, u32, String);

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root exists")
}

/// Every reStructuredText document under `examples/` and `docs/`, sorted.
fn corpus(root: &Path) -> Vec<PathBuf> {
    fn collect(directory: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory).expect("a readable directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                collect(&path, found);
            } else if path.extension().is_some_and(|extension| extension == "rst") {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    collect(&root.join("examples"), &mut found);
    collect(&root.join("docs"), &mut found);
    found.sort();
    found
}

/// What `rinx parse` warns about in `document` itself, read off the
/// `warning: path:line:column: code: message` lines it prints.
fn build_findings(document: &Path, scratch: &Path) -> BTreeSet<Finding> {
    let output = Command::new(env!("CARGO_BIN_EXE_rinx"))
        .arg("parse")
        .arg("--input")
        .arg(document)
        .arg("--output")
        .arg(scratch.join("parity.ast"))
        // So a diagram is parsed rather than refused: the server has no
        // library to ask whether diagrams are enabled.
        .arg("--diagrams")
        .output()
        .expect("rinx parse runs");
    let prefix = format!("warning: {}:", document.display());
    String::from_utf8_lossy(&output.stderr)
        .lines()
        // A warning naming another file is about an included fragment (#3).
        .filter_map(|line| line.strip_prefix(&prefix))
        .map(parse_warning_location)
        .collect()
}

/// The finding behind the rest of a warning line after `path:`, which is
/// either ` code: message` (no position) or `line:column: code: message`.
fn parse_warning_location(rest: &str) -> Finding {
    if let Some(positionless) = rest.strip_prefix(' ') {
        let code = positionless.split(": ").next().unwrap_or_default();
        // The server places a diagnostic without a span at the very start.
        return (0, 0, code.to_string());
    }
    let mut parts = rest.splitn(3, ':');
    let line: u32 = parts.next().and_then(|n| n.parse().ok()).expect("a line");
    let column: u32 = parts.next().and_then(|n| n.parse().ok()).expect("a column");
    let code = parts
        .next()
        .and_then(|after| after.trim_start().split(": ").next())
        .expect("a code");
    (line - 1, column - 1, code.to_string())
}

/// What the server publishes for the open document at `uri`.
fn editor_findings(published: &Value) -> BTreeSet<Finding> {
    let as_u32 = |value: &Value| u32::try_from(value.as_u64().expect("a number")).unwrap();
    published["diagnostics"]
        .as_array()
        .expect("a diagnostics array")
        .iter()
        .map(|diagnostic| {
            let start = &diagnostic["range"]["start"];
            (
                as_u32(&start["line"]),
                as_u32(&start["character"]),
                diagnostic["code"].as_str().expect("a code").to_string(),
            )
        })
        .collect()
}

#[test]
fn test_parse_warning_location_reads_a_positioned_warning() {
    // Given / When
    let finding = parse_warning_location("12:5: directive.unknown: unknown directive 'x'");

    // Then — converted to the protocol's 0-based positions
    assert_eq!(finding, (11, 4, "directive.unknown".to_string()));
}

#[test]
fn test_parse_warning_location_places_a_positionless_warning_at_the_start() {
    // Given / When
    let finding = parse_warning_location(" csv-table.empty: no rows: a: b");

    // Then
    assert_eq!(finding, (0, 0, "csv-table.empty".to_string()));
}

#[test]
fn test_the_editor_reports_what_the_build_reports() {
    // Given — the server, counting columns in characters as a warning does
    let root = repository_root();
    let documents = corpus(&root);
    assert!(
        documents.len() > 50,
        "the corpus went missing: {documents:?}"
    );
    let scratch = std::env::temp_dir().join(format!("rinx_lsp_parity_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    let mut server = Server::spawn();
    server.initialize(&json!({ "general": { "positionEncodings": ["utf-32"] } }));

    // When
    let mut mismatches = String::new();
    for document in &documents {
        let path = document.to_str().expect("a UTF-8 path");
        assert!(
            path.chars()
                .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c)),
            "{path} would need percent-encoding in its URI"
        );
        let uri = format!("file://{path}");
        let text = std::fs::read_to_string(document).expect("a readable document");
        server.notify(
            "textDocument/didOpen",
            json!({ "textDocument": {
                "uri": uri, "languageId": "restructuredtext", "version": 1, "text": text,
            }}),
        );
        let editor = editor_findings(&server.published());
        server.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        );
        server.published();
        let build = build_findings(document, &scratch);

        // Then — collected, so one run lists every document that disagrees
        if build != editor {
            let relative = document.strip_prefix(&root).unwrap_or(document);
            let _ = writeln!(
                mismatches,
                "{}:\n  build only:  {:?}\n  editor only: {:?}",
                relative.display(),
                build.difference(&editor).collect::<Vec<_>>(),
                editor.difference(&build).collect::<Vec<_>>(),
            );
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(
        mismatches.is_empty(),
        "the editor and the build disagree:\n{mismatches}"
    );
}
