//! How the reader treats statements inside compound statements.

use super::test_support::*;

#[test]
fn test_read_module_records_an_assignment_in_an_if_as_a_modification() {
    // Given — the shape of CPython's `Doc/conf.py`
    let source = "exclude_patterns = ['includes/*.rst']\nvenvdir = os.getenv('VENVDIR')\nif venvdir is not None:\n    exclude_patterns.append(venvdir + '/*')\n";

    // When
    let reading = read_module(source);

    // Then
    assert!(literal(&reading, "exclude_patterns").is_some());
    assert_eq!(modification_lines(&reading, "exclude_patterns"), [4]);
}

#[test]
fn test_read_module_binds_a_name_first_assigned_in_a_block_as_unread() {
    // When
    let reading = read_module("if x:\n    html_theme = 'a'\nelse:\n    html_theme = 'b'\n");

    // Then
    assert!(is_unread(&reading, "html_theme"));
    assert_eq!(modification_lines(&reading, "html_theme"), [4]);
}

#[test]
fn test_read_module_reads_a_body_on_the_header_line() {
    // When
    let reading = read_module("x = 1\nif y: x = 2; z = 3\n");

    // Then
    assert_eq!(modification_lines(&reading, "x"), [2]);
    assert!(is_unread(&reading, "z"));
}

#[test]
fn test_read_module_records_a_loop_appending_to_a_list() {
    // Given — the shape of CPython's optional extensions
    let source = "extensions = ['a']\nfor ext in ('b',):\n    try:\n        import_module(ext)\n    except ImportError:\n        pass\n    else:\n        extensions.append(ext)\n";

    // When
    let reading = read_module(source);

    // Then
    assert!(literal(&reading, "extensions").is_some());
    assert_eq!(modification_lines(&reading, "extensions"), [8]);
    assert!(is_unread(&reading, "ext"));
}

#[test]
fn test_read_module_binds_a_loops_unpacked_targets() {
    // When
    let reading = read_module("for role, name in list(pairs):\n    pass\n");

    // Then
    assert!(is_unread(&reading, "role"));
    assert!(is_unread(&reading, "name"));
}

#[test]
fn test_read_module_binds_with_and_except_aliases() {
    // When
    let reading = read_module(
        "with open(p) as f, open(q) as g:\n    pass\ntry:\n    pass\nexcept E as error:\n    pass\n",
    );

    // Then
    for name in ["f", "g", "error"] {
        assert!(is_unread(&reading, name), "{name}");
    }
}

#[test]
fn test_read_module_skips_a_functions_body() {
    // When
    let reading = read_module(
        "exclude_patterns = ['a']\ndef setup(app):\n    exclude_patterns = []\n    if x:\n        project = 'y'\n    return {}\n\nclass C:\n    attr = 1\n",
    );

    // Then
    assert!(literal(&reading, "exclude_patterns").is_some());
    assert_eq!(
        modification_lines(&reading, "exclude_patterns"),
        Vec::<usize>::new()
    );
    assert!(reading.binding("project").is_none());
    assert!(reading.binding("attr").is_none());
    assert!(is_unread(&reading, "setup"));
    assert!(is_unread(&reading, "C"));
}

#[test]
fn test_read_module_skips_an_async_functions_body_and_its_same_line_body() {
    // When
    let reading = read_module("async def go(): x = 1\ndef f(): y = 2\n");

    // Then
    assert!(is_unread(&reading, "go"));
    assert!(reading.binding("x").is_none());
    assert!(reading.binding("y").is_none());
}

#[test]
fn test_read_module_reads_a_match_statement_as_a_block() {
    // When
    let reading = read_module("x = 1\nmatch y:\n    case 1:\n        x = 2\n");

    // Then
    assert_eq!(modification_lines(&reading, "x"), [4]);
}

#[test]
fn test_read_module_reads_top_level_statements_after_a_block() {
    // When
    let reading = read_module("if x:\n    pass\nroot_doc = 'index'\n");

    // Then
    assert_eq!(literal(&reading, "root_doc"), Some(string("index")));
}

#[test]
fn test_read_module_records_a_conditional_delete_as_a_modification() {
    // When
    let reading = read_module("x = 1\nif y:\n    del x\n");

    // Then
    assert_eq!(modification_lines(&reading, "x"), [3]);
}
