//! The editor's diagnostics against the build's, over this repository's own
//! documents: ADR-038 promises they are the same, and this is where that is
//! checked.
//!
//! Both sides run as the processes they are — `rinx parse` as a Bazel action
//! runs it, `rinx lsp` as an editor does — with the defaults both assume
//! today: the `py` domain, no entity schema, no Jinja. Project configuration
//! reaches the server only with workspace awareness (roadmap #4, #10, #19),
//! and from then on this test should parse each document with its library's
//! flags.
//!
//! The comparison is file by file: what the build warns about inside an
//! `.. include::`d fragment, the server must publish on that fragment.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::lsp_client::Server;

/// One reported problem: 0-based line, 0-based character column, code.
type Finding = (u32, u32, String);

/// What was reported, by the absolute path of the file it is in.
type Findings = BTreeMap<PathBuf, BTreeSet<Finding>>;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root exists")
}

/// Every reStructuredText document under `examples/` and `docs/`, plus this
/// test's own fixtures (`crates/worker/tests/parity/`), sorted. The fixtures
/// hold the mistakes a published page must not: the corpus alone has none
/// inside an included fragment, so without them that path would pass for
/// want of anything to compare.
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
    collect(&root.join("crates/worker/tests/parity"), &mut found);
    found.sort();
    found
}

/// What `rinx parse` warns about in `document` and the files it includes,
/// read off the `warning: path:line:column: code: message` lines it prints —
/// with ` (included from …)` after the position for an included file, whose
/// path is absolute because `document`'s is.
fn build_findings(document: &Path, scratch: &Path) -> Findings {
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
    let mut findings = Findings::new();
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        if let Some((path, finding)) = parse_warning(line, document) {
            findings.entry(path).or_default().insert(finding);
        }
    }
    findings
}

/// The file and finding of one line `rinx parse` printed about `document`,
/// or `None` for a line that is no warning.
fn parse_warning(line: &str, document: &Path) -> Option<(PathBuf, Finding)> {
    let rest = line.strip_prefix("warning: ")?;
    let own = format!("{}:", document.display());
    if let Some(rest) = rest.strip_prefix(&own) {
        return Some((document.to_path_buf(), parse_warning_location(rest)));
    }
    let included_from = format!(" (included from {}):", document.display());
    let (location, after) = rest.split_once(&included_from)?;
    let (path, position) = location.split_once(':')?;
    let finding = parse_warning_location(&format!("{position}:{after}"));
    Some((PathBuf::from(path), finding))
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

/// What the server publishes, by file, leaving out files with nothing to
/// report as the build's warnings do — and leaving out the summary an
/// `.. include::` carries of its file's problems, which has no code and no
/// counterpart in the build: those problems are compared on the file itself.
fn editor_findings(publishes: &[Value]) -> Findings {
    let as_u32 = |value: &Value| u32::try_from(value.as_u64().expect("a number")).unwrap();
    let mut findings = Findings::new();
    for published in publishes {
        let uri = published["uri"].as_str().expect("a uri");
        let path = PathBuf::from(uri.strip_prefix("file://").expect("a file uri"));
        let found: BTreeSet<Finding> = published["diagnostics"]
            .as_array()
            .expect("a diagnostics array")
            .iter()
            .filter(|diagnostic| diagnostic.get("code").is_some())
            .map(|diagnostic| {
                let start = &diagnostic["range"]["start"];
                (
                    as_u32(&start["line"]),
                    as_u32(&start["character"]),
                    diagnostic["code"].as_str().expect("a code").to_string(),
                )
            })
            .collect();
        if !found.is_empty() {
            findings.insert(path, found);
        }
    }
    findings
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
fn test_parse_warning_reads_a_warning_about_the_document() {
    // Given / When
    let parsed = parse_warning(
        "warning: /d/index.rst:3:1: directive.unknown: unknown",
        Path::new("/d/index.rst"),
    );

    // Then
    assert_eq!(
        parsed,
        Some((
            PathBuf::from("/d/index.rst"),
            (2, 0, "directive.unknown".to_string())
        ))
    );
}

#[test]
fn test_parse_warning_reads_a_warning_about_an_included_file() {
    // Given / When
    let parsed = parse_warning(
        "warning: /d/part.rst:3:5 (included from /d/index.rst): directive.unknown: unknown",
        Path::new("/d/index.rst"),
    );

    // Then
    assert_eq!(
        parsed,
        Some((
            PathBuf::from("/d/part.rst"),
            (2, 4, "directive.unknown".to_string())
        ))
    );
}

#[test]
fn test_parse_warning_skips_a_line_that_is_no_warning() {
    // Given / When / Then
    assert_eq!(
        parse_warning("error: cannot read", Path::new("/d/index.rst")),
        None
    );
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
        let editor = editor_findings(&server.published_until(&uri));
        server.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        );
        // Closing clears the fragments as well as the document.
        server.published_until(&uri);
        let build = build_findings(document, &scratch);

        // Then — collected, so one run lists every document that disagrees
        if build != editor {
            let relative = document.strip_prefix(&root).unwrap_or(document);
            let _ = writeln!(
                mismatches,
                "{}:\n  build:  {build:?}\n  editor: {editor:?}",
                relative.display(),
            );
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(
        mismatches.is_empty(),
        "the editor and the build disagree:\n{mismatches}"
    );
}
