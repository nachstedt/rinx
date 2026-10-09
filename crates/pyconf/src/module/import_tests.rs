//! How the reader treats imports, which bind names to modules it does not
//! read.

use super::test_support::*;

#[test]
fn test_read_module_binds_imported_names_as_unread() {
    // When
    let reading = read_module(
        "import os, os.path as osp\nimport sphinx.ext\nfrom importlib import import_module\nfrom a import (b as c,\n    d)\n",
    );

    // Then
    for name in ["os", "osp", "sphinx", "import_module", "c", "d"] {
        assert!(is_unread(&reading, name), "{name}");
    }
    assert!(reading.binding("b").is_none());
}

#[test]
fn test_read_module_records_a_wildcard_import() {
    // When
    let reading = read_module("x = 1\nfrom base_conf import *\n");

    // Then
    let lines: Vec<usize> = reading
        .wildcard_imports
        .iter()
        .map(|span| span.start.line)
        .collect();
    assert_eq!(lines, [2]);
}

#[test]
fn test_read_module_rebinds_an_imported_name_assigned_later() {
    // When
    let reading = read_module("from base import project\nproject = 'Mine'\n");

    // Then
    assert_eq!(literal(&reading, "project"), Some(string("Mine")));
}

#[test]
fn test_read_module_binds_an_import_inside_a_block_conditionally() {
    // When
    let reading = read_module(
        "theme = 'a'\ntry:\n    from furo import theme\nexcept ImportError:\n    pass\n",
    );

    // Then
    assert!(literal(&reading, "theme").is_some());
    assert_eq!(modification_lines(&reading, "theme"), [3]);
}
