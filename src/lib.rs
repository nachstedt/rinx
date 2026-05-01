//! Rusty Sphinx Support Library
//!
//! This module provides the foundational pieces for building a high-performance,
//! Bazel-compatible re-implementation of the Sphinx documentation framework.

pub mod analyzer;
pub mod ast;
pub mod config;
pub mod parser;
pub mod renderer;
pub mod validator;

/// Returns a welcoming greeting for the Rusty Sphinx application.
///
/// # Examples
///
/// ```
/// use rusty_sphinx::greeting;
/// let message = greeting("World");
/// assert_eq!(message, "Hello, World!");
/// ```
#[must_use]
pub fn greeting(name: &str) -> String {
    format!("Hello, {name}!")
}

/// Processes an RST text block through the full pipeline (parse, analyze, render)
/// and outputs an HTML string.
#[must_use]
pub fn process_rst(path: &str, input: &str) -> String {
    let doc = parser::parse(path, input);
    let index = analyzer::analyze(&doc);
    renderer::render(&doc, &index, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greeting_returns_formatted_message() {
        // Given
        let name = "Rustacean";

        // When
        let result = greeting(name);

        // Then
        assert_eq!(result, "Hello, Rustacean!");
    }

    #[test]
    fn test_process_rst_renders_html_from_rst() {
        // Given
        let rst = "Introduction\n============\n\nThis is a paragraph.";

        // When
        let html = process_rst("test.rst", rst);

        // Then
        assert_eq!(html, "<h1>Introduction</h1>\n<p>This is a paragraph.</p>\n");
    }
}
