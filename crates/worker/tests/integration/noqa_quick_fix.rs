//! The quick fix writing a `.. noqa:`, applied as an editor applies it: for
//! every finding the server publishes on a document, take the fix, apply
//! its edit, and check that the finding is gone and no finding of another
//! code is.
//!
//! The documents are `crates/worker/tests/noqa_fix/`, written to hold a
//! finding in every kind of block a comment is written before differently —
//! after a title, in a directive's options and body, in a list item, a
//! definition, a block quote, a table cell, and below a `.. noqa:` it
//! extends — and the parity fixtures, which hold the mistakes a published
//! page must not.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lsp_server::Message;
use serde_json::{Value, json};

use crate::lsp_client::Server;

/// The documents to suppress every finding of.
fn documents() -> Vec<PathBuf> {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut found = Vec::new();
    for directory in ["noqa_fix", "parity"] {
        for entry in std::fs::read_dir(tests.join(directory)).expect("reads the fixtures") {
            let path = entry.expect("an entry").path();
            if path.extension().is_some_and(|extension| extension == "rst") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// How many findings of each code `published` holds.
fn counts(published: &Value) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for diagnostic in published["diagnostics"].as_array().expect("diagnostics") {
        if let Some(code) = diagnostic["code"].as_str() {
            *counts.entry(code.to_string()).or_default() += 1;
        }
    }
    counts
}

/// `text` with the one insertion `edits` holds applied.
fn applied(text: &str, edits: &Value) -> String {
    let [edit] = edits.as_array().expect("edits").as_slice() else {
        panic!("one edit, got {edits}");
    };
    let start = &edit["range"]["start"];
    assert_eq!(edit["range"]["start"], edit["range"]["end"], "an insertion");
    let line = usize::try_from(start["line"].as_u64().expect("a line")).unwrap();
    let character = usize::try_from(start["character"].as_u64().expect("a column")).unwrap();
    let offset: usize = text
        .split_inclusive('\n')
        .take(line)
        .map(str::len)
        .sum::<usize>()
        + text
            .split_inclusive('\n')
            .nth(line)
            .unwrap_or_default()
            .chars()
            .take(character)
            .map(char::len_utf8)
            .sum::<usize>();
    let mut edited = text.to_string();
    edited.insert_str(
        offset,
        edit["new_text"]
            .as_str()
            .or(edit["newText"].as_str())
            .expect("text"),
    );
    edited
}

/// The `.. noqa:` fixes the server offers for `diagnostic` in `uri`.
fn noqa_fixes(server: &mut Server, id: i32, uri: &str, diagnostic: &Value) -> Vec<Value> {
    server.request(
        id,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": uri },
            "range": diagnostic["range"],
            "context": { "diagnostics": [diagnostic] },
        }),
    );
    let answer = loop {
        match server.receive() {
            Message::Response(response) => break response,
            Message::Notification(_) | Message::Request(_) => {}
        }
    };
    let actions = answer.response_result.expect("code actions");
    actions
        .as_array()
        .expect("an array")
        .iter()
        .filter(|action| {
            action["title"]
                .as_str()
                .is_some_and(|title| title.starts_with("Suppress "))
        })
        .cloned()
        .collect()
}

/// Sends `text` as the new content of `uri`, at `version`, and returns what
/// the server then publishes for it.
fn change(server: &mut Server, uri: &str, version: i32, text: &str) -> Value {
    server.notify(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": version },
            "contentChanges": [{ "text": text }],
        }),
    );
    server.published_until(uri).pop().expect("a publish")
}

#[test]
fn test_every_finding_is_silenced_by_its_noqa_fix_alone() {
    let mut server = Server::spawn();
    server.initialize(&json!({}));
    let mut id = 2;
    let mut version = 1;
    let mut silenced = 0;
    for path in documents() {
        // Given the document open, and what the server shows on it
        let text = std::fs::read_to_string(&path).expect("reads a fixture");
        let uri = format!("file://{}", path.display());
        server.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "restructuredtext",
                    "version": version,
                    "text": text,
                }
            }),
        );
        let opened = server.published_until(&uri).pop().expect("a publish");
        let before = counts(&opened);
        let findings: Vec<Value> = opened["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .filter(|diagnostic| diagnostic["code"].is_string())
            .cloned()
            .collect();
        for diagnostic in findings {
            let code = diagnostic["code"].as_str().expect("a code").to_string();
            if code == "noqa.unknown-code" {
                // Written on the comment, which no comment covers.
                continue;
            }

            // When the fix is applied
            id += 1;
            let fixes = noqa_fixes(&mut server, id, &uri, &diagnostic);
            let [fix] = fixes.as_slice() else {
                panic!(
                    "{}: one fix for {diagnostic}, got {fixes:?}",
                    path.display()
                );
            };
            let edited = applied(&text, &fix["edit"]["changes"][&uri]);
            version += 1;
            let after = counts(&change(&mut server, &uri, version, &edited));

            // Then — that finding is gone, and nothing of another code: a
            // block that can only be written before as part of its
            // container takes the container's other findings of the code
            // along
            let mut expected = before.clone();
            expected.remove(&code);
            let mut others = after.clone();
            let left = others.remove(&code).unwrap_or_default();
            assert!(
                left < before[&code] && others == expected,
                "{}: fixing {diagnostic} left {after:?} of {before:?} in\n{edited}",
                path.display()
            );
            silenced += 1;
            version += 1;
            change(&mut server, &uri, version, &text);
        }
        server.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        );
        version += 1;
    }
    // Every finding `noqa_fix/placements.rst` holds was fixed, at least.
    assert!(silenced >= 15, "{silenced}");
}
