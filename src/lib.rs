//! Rusty Sphinx Support Library
//!
//! This module provides the foundational pieces for building a high-performance,
//! Bazel-compatible re-implementation of the Sphinx documentation framework.

pub mod analyzer;
pub mod ast;
pub mod parser;
pub mod renderer;

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
pub fn process_rst(input: &str) -> String {
    let doc = parser::parse(input);
    let index = analyzer::analyze(&doc);
    renderer::render(&doc, &index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greeting() {
        assert_eq!(greeting("Rustacean"), "Hello, Rustacean!");
    }
}
