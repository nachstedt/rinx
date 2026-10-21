//! The editor's diagnostics against the build's, over this repository's own
//! documents: ADR-038 promises they are the same, and this is where that is
//! checked.
//!
//! Both sides run as the processes they are — `rinx parse` as a Bazel action
//! runs it, `rinx lsp` as an editor does. Over the repository's documents
//! both use the defaults they assume without a project: the `py` domain, no
//! entity schema, no Jinja. Two small projects of their own check what only
//! a project reveals: a Sphinx one, whose whole index the build's `index`
//! action and `rinx lsp --check` both read, and a Bazel site, whose
//! undeclared toctree entries, diagrams, images and downloads fail the
//! build's actions and are the editor's errors under its manifest, and one of
//! whose libraries renders its document as a Jinja template, whose warnings
//! both place on the lines they were written on. A last check compares
//! pages rather than diagnostics: what `rinx/preview` draws for a document is
//! the page `rinx render` writes for it.
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

/// Runs `rinx` with `args` in `directory`, returning what it printed to
/// stdout and stderr, and failing the test when it fails.
fn rinx(directory: &Path, args: &[&str]) -> (String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_rinx"))
        .args(args)
        .current_dir(directory)
        .output()
        .expect("rinx runs");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "rinx {args:?} failed:\n{stderr}");
    (String::from_utf8_lossy(&output.stdout).into_owned(), stderr)
}

/// What the build warns about in `project`'s documents, parsing each and
/// indexing them all as a site does — run in `project`, so a document is
/// named as the server names it, against the entity schema `rinx needs_schema`
/// writes from its `ubproject.toml`.
fn project_build_findings(project: &Path, documents: &[&str], scratch: &Path) -> Findings {
    let schema = scratch.join("entities.toml");
    let schema = schema.to_str().expect("a UTF-8 path");
    rinx(
        project,
        &[
            "needs_schema",
            "--ubproject",
            "ubproject.toml",
            "--output",
            schema,
        ],
    );
    let mut warnings = String::new();
    let mut asts = Vec::new();
    for document in documents {
        let ast = scratch.join(format!("{document}.ast"));
        let ast = ast.to_str().expect("a UTF-8 path").to_string();
        let (_, stderr) = rinx(
            project,
            &[
                "parse",
                "--input",
                document,
                "--output",
                &ast,
                "--entity-schema",
                schema,
            ],
        );
        warnings.push_str(&stderr);
        asts.push(ast);
    }
    let index = scratch.join("project.index");
    let mut args = vec![
        "index",
        "--output",
        index.to_str().expect("a UTF-8 path"),
        "--entity-schema",
        schema,
        "--inputs",
    ];
    args.extend(asts.iter().map(String::as_str));
    let (_, stderr) = rinx(project, &args);
    warnings.push_str(&stderr);

    let mut findings = Findings::new();
    for line in warnings.lines() {
        let Some(rest) = line.strip_prefix("warning: ") else {
            continue;
        };
        let (document, after) = rest.split_once(':').expect("a path");
        findings
            .entry(project.join(document))
            .or_default()
            .insert(parse_warning_location(after));
    }
    findings
}

#[test]
fn test_the_editor_reports_what_the_index_reports() {
    // Given a project whose mistakes only the whole of it reveals: a toctree
    // entry naming no document, a label two documents define, and a need
    // linking to one no document declares
    let project = repository_root().join("crates/worker/tests/parity_project");
    let scratch =
        std::env::temp_dir().join(format!("rinx_lsp_parity_project_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("a scratch directory");

    // When — the build parses and indexes it; the server checks the folder
    let build = project_build_findings(
        &project,
        &["index.rst", "labels.rst", "needs.rst"],
        &scratch,
    );
    let (stdout, _) = rinx(
        &project,
        &["lsp", "--check", project.to_str().expect("a UTF-8 path")],
    );
    let check: Value = serde_json::from_str(&stdout).expect("--check prints JSON");
    let publishes: Vec<Value> = check["published"]
        .as_object()
        .expect("a map of publishes")
        .iter()
        // `conf.py` shows what the editor reads of the configuration, which
        // has no counterpart in the build.
        .filter(|(uri, _)| {
            Path::new(uri.as_str())
                .extension()
                .is_some_and(|ext| ext == "rst")
        })
        .map(|(uri, diagnostics)| json!({ "uri": uri, "diagnostics": diagnostics }))
        .collect();
    let editor = editor_findings(&publishes);
    let _ = std::fs::remove_dir_all(&scratch);

    // Then — the same, and something: a fixture that reported nothing would
    // pass for want of anything to compare
    let codes: BTreeSet<&str> = build
        .values()
        .flatten()
        .map(|(_, _, code)| code.as_str())
        .collect();
    assert_eq!(
        codes,
        BTreeSet::from([
            "entity.unknown-target",
            "target.duplicate-name",
            "toctree.missing-document"
        ])
    );
    // — each finding on the line to change, which a positionless one is not:
    // `:implements:` is `needs.rst`'s eleventh line
    assert!(
        build[&project.join("needs.rst")].contains(&(10, 3, "entity.unknown-target".to_string())),
        "{build:?}"
    );
    assert_eq!(editor, build);
}

/// Runs `rinx` with `args` in `directory`, expecting it to fail, and
/// returns what it printed to stderr.
fn failing_rinx(directory: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rinx"))
        .args(args)
        .current_dir(directory)
        .output()
        .expect("rinx runs");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "rinx {args:?} succeeded:\n{stderr}"
    );
    stderr
}

/// The `error: path:line:column: code: message` lines among `printed`, by
/// the file under `project` each names.
fn error_findings(project: &Path, printed: &str) -> Findings {
    let mut findings = Findings::new();
    for line in printed.lines() {
        let Some(rest) = line.trim_start().strip_prefix("error: ") else {
            continue;
        };
        let (document, after) = rest.split_once(':').expect("a path");
        findings
            .entry(project.join(document))
            .or_default()
            .insert(parse_warning_location(after));
    }
    findings
}

/// What the build's actions print as errors over the Bazel site in
/// `project`: parsing `drawn.rst` without `diagrams`, validating
/// `index.rst`'s toctree against only itself, and validating the assets of
/// `index.rst` and `guide.rst` against an `_images/` holding `logo.png`.
fn bazel_build_errors(project: &Path, scratch: &Path) -> (Findings, String) {
    let images = scratch.join("_images");
    std::fs::create_dir_all(&images).expect("a scratch directory");
    std::fs::write(images.join("logo.png"), "").expect("a bundled image");
    let ast = |name: &str| {
        scratch
            .join(name)
            .to_str()
            .expect("a UTF-8 path")
            .to_string()
    };
    let mut printed = failing_rinx(
        project,
        &[
            "parse",
            "--input",
            "drawn.rst",
            "--output",
            &ast("drawn.ast"),
        ],
    );
    for document in ["index", "guide"] {
        rinx(
            project,
            &[
                "parse",
                "--input",
                &format!("{document}.rst"),
                "--output",
                &ast(&format!("{document}.ast")),
            ],
        );
    }
    printed.push_str(&failing_rinx(
        project,
        &[
            "validate_toctree",
            "--input",
            &ast("index.ast"),
            "--output",
            &ast("index.validated.ast"),
            "--allowed",
            "index",
        ],
    ));
    printed.push_str(&failing_rinx(
        project,
        &[
            "validate_assets",
            "--inputs",
            &ast("index.ast"),
            &ast("guide.ast"),
            "--image-dir",
            images.to_str().expect("a UTF-8 path"),
        ],
    ));
    (error_findings(project, &printed), printed)
}

/// The errors `rinx lsp --check` shows over `project`, granted its
/// `site.lsp_manifest.json`.
fn editor_errors(project: &Path) -> Findings {
    let (stdout, _) = rinx(
        project,
        &[
            "lsp",
            "--check",
            project.to_str().expect("a UTF-8 path"),
            "--manifest",
            project
                .join("site.lsp_manifest.json")
                .to_str()
                .expect("a UTF-8 path"),
        ],
    );
    let check: Value = serde_json::from_str(&stdout).expect("--check prints JSON");
    let errors: Vec<Value> = check["published"]
        .as_object()
        .expect("a map of publishes")
        .iter()
        .map(|(uri, diagnostics)| {
            let errors: Vec<&Value> = diagnostics
                .as_array()
                .expect("a diagnostics array")
                .iter()
                .filter(|diagnostic| diagnostic["severity"] == 1)
                .collect();
            json!({ "uri": uri, "diagnostics": errors })
        })
        .collect();
    editor_findings(&errors)
}

#[test]
fn test_the_editor_fails_where_the_build_fails() {
    // Given a Bazel site whose documents use what their libraries did not
    // declare: a toctree entry outside `deps`, a diagram without
    // `diagrams = True`, an image and a download no library bundles — and an
    // image another library of the site bundles, which is no mistake
    let project = repository_root().join("crates/worker/tests/parity_bazel");
    let scratch =
        std::env::temp_dir().join(format!("rinx_lsp_parity_bazel_{}", std::process::id()));

    // When — the build's own actions fail, each on its own mistake, and the
    // server checks the folder under the site's manifest
    let (build, printed) = bazel_build_errors(&project, &scratch);
    let editor = editor_errors(&project);
    let _ = std::fs::remove_dir_all(&scratch);

    // Then — every mistake, each once, and nothing else
    let codes: Vec<&str> = build
        .values()
        .flatten()
        .map(|(_, _, code)| code.as_str())
        .collect();
    assert_eq!(
        codes,
        [
            "uml.diagrams-disabled",
            "toctree.undeclared-dependency",
            "image.undeclared",
            "download.undeclared",
        ],
        "{printed}"
    );
    assert_eq!(editor, build);
}

/// Every finding `rinx lsp --check` shows over `project`, granted its
/// `site.lsp_manifest.json`, in the files `files` names under it.
fn editor_findings_in(project: &Path, files: &[&str]) -> Findings {
    let (stdout, _) = rinx(
        project,
        &[
            "lsp",
            "--check",
            project.to_str().expect("a UTF-8 path"),
            "--manifest",
            project
                .join("site.lsp_manifest.json")
                .to_str()
                .expect("a UTF-8 path"),
        ],
    );
    let check: Value = serde_json::from_str(&stdout).expect("--check prints JSON");
    let publishes: Vec<Value> = check["published"]
        .as_object()
        .expect("a map of publishes")
        .iter()
        .map(|(uri, diagnostics)| json!({ "uri": uri, "diagnostics": diagnostics }))
        .collect();
    let wanted: BTreeSet<PathBuf> = files.iter().map(|file| project.join(file)).collect();
    editor_findings(&publishes)
        .into_iter()
        .filter(|(path, _)| wanted.contains(path))
        .collect()
}

#[test]
fn test_the_editor_places_a_templated_documents_warnings_as_the_build_does() {
    // Given a library rendering its document as a Jinja template, which
    // renders a three-line template in above a mistake of its own, the
    // template holding another
    let project = repository_root().join("crates/worker/tests/parity_bazel");
    let scratch =
        std::env::temp_dir().join(format!("rinx_lsp_parity_jinja_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("a scratch directory");

    // When — the build parses it as `rinx_library(jinja = True, ...)` has
    // it parsed, and the server checks the folder under the site's manifest
    let (_, printed) = rinx(
        &project,
        &[
            "parse",
            "--input",
            "templated.rst",
            "--output",
            scratch
                .join("templated.ast")
                .to_str()
                .expect("a UTF-8 path"),
            "--jinja",
            "--jinja-context",
            "release=0.1",
        ],
    );
    let mut build = Findings::new();
    for line in printed.lines() {
        if let Some((path, finding)) = parse_warning(line, Path::new("templated.rst")) {
            build.entry(project.join(path)).or_default().insert(finding);
        }
    }
    let editor = editor_findings_in(&project, &["templated.rst", "_templates/header.rst"]);
    let _ = std::fs::remove_dir_all(&scratch);

    // Then — each on the line it was written on, in the file it was
    // written in
    assert_eq!(
        build,
        Findings::from([
            (
                project.join("_templates/header.rst"),
                BTreeSet::from([(2, 0, "directive.unknown".to_string())])
            ),
            (
                project.join("templated.rst"),
                BTreeSet::from([(8, 0, "directive.unknown".to_string())])
            ),
        ]),
        "{printed}"
    );
    assert_eq!(editor, build);
}

/// The page `rinx lsp` previews for `document` of the folder `project`,
/// opened as it is on the disk.
fn previewed_page(project: &Path, document: &str) -> String {
    let uri = format!("file://{}", project.join(document).display());
    let mut server = Server::spawn();
    server.initialize_with(json!({
        "capabilities": {},
        "workspaceFolders": [{"uri": format!("file://{}", project.display()), "name": "docs"}],
    }));
    while !matches!(
        server.receive(),
        lsp_server::Message::Notification(notification)
            if notification.method == "rinx/status" && notification.params["state"] == "ready"
    ) {}
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": {
            "uri": uri, "languageId": "restructuredtext", "version": 1,
            "text": std::fs::read_to_string(project.join(document)).expect("the document"),
        }}),
    );
    server.request(2, "rinx/preview", json!({ "textDocument": { "uri": uri } }));
    let previewed = std::iter::from_fn(|| Some(server.receive()))
        .find_map(|message| match message {
            lsp_server::Message::Response(response) => Some(response),
            _ => None,
        })
        .expect("the preview");
    previewed.response_result.expect("a page")["html"]
        .as_str()
        .expect("html")
        .to_string()
}

/// `page` with its stylesheet's address replaced by `STYLESHEET`.
fn with_stylesheet_named(page: &str) -> String {
    let start = page
        .find("<link rel=\"stylesheet\" href=\"")
        .expect("a stylesheet link")
        + 29;
    let end = start + page[start..].find('"').expect("the end of the address");
    format!("{}STYLESHEET{}", &page[..start], &page[end..])
}

#[test]
fn test_the_preview_draws_the_page_the_build_writes() {
    // Given a folder of two documents, the root's toctree naming the other,
    // which links back by label
    let project =
        std::env::temp_dir().join(format!("rinx_lsp_parity_preview_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(&project).expect("a project directory");
    std::fs::write(
        project.join("index.rst"),
        ".. _home:\n\nHome\n====\n\nWelcome.\n\n.. toctree::\n   :numbered:\n\n   guide\n",
    )
    .expect("index.rst");
    std::fs::write(
        project.join("guide.rst"),
        "Guide\n=====\n\nBack to :ref:`home`.\n\nSection\n-------\n",
    )
    .expect("guide.rst");
    std::fs::write(project.join("rinx.toml"), "").expect("rinx.toml");
    let template = repository_root().join("templates/default.html");
    let template = template.to_str().expect("a UTF-8 path");

    // When — the build parses, indexes and renders `guide.rst` with the
    // default template, and the server previews it
    for document in ["index", "guide"] {
        rinx(
            &project,
            &[
                "parse",
                "--input",
                &format!("{document}.rst"),
                "--output",
                &format!("{document}.ast"),
            ],
        );
    }
    rinx(
        &project,
        &[
            "index",
            "--output",
            "project.index",
            "--inputs",
            "index.ast",
            "guide.ast",
        ],
    );
    rinx(
        &project,
        &[
            "render",
            "--input",
            "guide.ast",
            "--index",
            "project.index",
            "--doc-path",
            "guide.rst",
            "--output",
            "guide.html",
            "--config",
            "rinx.toml",
            "--template",
            template,
        ],
    );
    let built = std::fs::read_to_string(project.join("guide.html")).expect("the built page");
    for generated in ["index.ast", "guide.ast", "project.index", "guide.html"] {
        std::fs::remove_file(project.join(generated)).expect("a generated file");
    }
    let previewed = previewed_page(&project, "guide.rst");
    let _ = std::fs::remove_dir_all(&project);

    // Then — the same page, but for where it finds its stylesheet
    assert!(
        built.contains("<span class=\"section-number\">1.1. </span>Section"),
        "the section numbers are drawn: {built}"
    );
    assert_eq!(
        with_stylesheet_named(&previewed),
        with_stylesheet_named(&built)
    );
}
