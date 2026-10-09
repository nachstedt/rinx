//! How the reader records what changes a bound value without rebinding it.

use super::test_support::*;

#[test]
fn test_read_module_records_an_augmented_assignment() {
    // When
    let reading = read_module("nitpick_ignore = [('a', 'b')]\n\nnitpick_ignore += [('c', 'd')]\n");

    // Then
    assert!(literal(&reading, "nitpick_ignore").is_some());
    assert_eq!(modification_lines(&reading, "nitpick_ignore"), [3]);
}

#[test]
fn test_read_module_records_a_method_call_on_a_name() {
    // When
    let reading = read_module(
        "exclude_patterns = ['a']\nexclude_patterns.append('b')\nexclude_patterns[0] = 'c'\n",
    );

    // Then
    assert!(literal(&reading, "exclude_patterns").is_some());
    assert_eq!(modification_lines(&reading, "exclude_patterns"), [2, 3]);
}

#[test]
fn test_read_module_records_an_attribute_or_item_assignment() {
    // When
    let reading = read_module("opts = {}\nopts['k'] = 1\nopts.x += 2\n");

    // Then
    assert_eq!(modification_lines(&reading, "opts"), [2, 3]);
}

#[test]
fn test_read_module_records_a_modification_of_an_unbound_name_as_unread() {
    // When
    let reading = read_module("extensions += ['x']\nsys.path.append('.')\n");

    // Then
    assert!(is_unread(&reading, "extensions"));
    assert!(is_unread(&reading, "sys"));
}

#[test]
fn test_read_module_records_a_modification_through_an_unpacking_target() {
    // When
    let reading = read_module("d = {}\nd['a'], b = 1, 2\n");

    // Then
    assert_eq!(modification_lines(&reading, "d"), [2]);
    assert!(is_unread(&reading, "b"));
}
