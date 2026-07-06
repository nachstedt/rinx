//! Rusty Sphinx Support Library
//!
//! This module provides the foundational pieces for building a high-performance,
//! Bazel-compatible re-implementation of the Sphinx documentation framework.

pub mod validator;

/// Processes an RST text block through the full pipeline (parse, analyze, render)
/// and outputs an HTML string.
#[must_use]
pub fn process_rst(path: &str, input: &str) -> String {
    let doc = rusty_sphinx_parser::parse(path, input);
    let index = rusty_sphinx_analyzer::analyze(&doc);
    rusty_sphinx_renderer::render(&doc, &index, path)
}

#[cfg(test)]
mod tests {
    use super::*;

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
