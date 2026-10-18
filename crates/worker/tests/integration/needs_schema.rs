//! `rinx needs_schema`: the entity schema a sphinx-needs `ubproject.toml`
//! converts into, written by the binary as the benchmark runs it.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// A scratch directory holding `files`.
fn temp_tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rinx_needs_schema_{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    for (relative, text) in files {
        fs::write(dir.join(relative), text).expect("write file");
    }
    dir
}

#[test]
fn test_needs_schema_writes_the_schema_and_prints_the_report() {
    // Given a configuration naming its `schemas.json`, with a rule the
    // entity model cannot express
    let dir = temp_tree(
        "cli",
        &[
            (
                "ubproject.toml",
                "[needs]\nschema_definitions_from_json = \"schemas.json\"\n\n[[needs.types]]\ndirective = \"req\"\ntitle = \"Requirement\"\n",
            ),
            (
                "schemas.json",
                r#"{"schemas": [{"id": "conditional", "select": {"allOf": []}}]}"#,
            ),
        ],
    );
    let output_path = dir.join("entities.toml");

    // When
    let output = Command::new(env!("CARGO_BIN_EXE_rinx"))
        .arg("needs_schema")
        .arg("--ubproject")
        .arg(dir.join("ubproject.toml"))
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("rinx runs");

    // Then the schema is written, and the report lists the rule
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let schema = fs::read_to_string(&output_path).expect("the schema is written");
    assert!(schema.contains("name = \"req\""), "{schema}");
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the report is JSON");
    let categories: Vec<&str> = report
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|entry| entry["category"].as_str())
        .collect();
    assert!(
        categories.contains(&"conditional schema rule"),
        "{categories:?}"
    );
}

#[test]
fn test_needs_schema_fails_without_its_ubproject_toml() {
    // Given
    let dir = temp_tree("cli_missing", &[]);

    // When
    let output = Command::new(env!("CARGO_BIN_EXE_rinx"))
        .arg("needs_schema")
        .arg("--ubproject")
        .arg(dir.join("ubproject.toml"))
        .arg("--output")
        .arg(dir.join("entities.toml"))
        .output()
        .expect("rinx runs");

    // Then
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ubproject.toml"));
}
