//! Integration tests for Rusty Sphinx public API.

use rusty_sphinx::greeting;

#[test]
fn integration_greeting() {
    let result = greeting("Integration");
    assert_eq!(result, "Hello, Integration!");
}
