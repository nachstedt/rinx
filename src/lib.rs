//! Rusty Sphinx Support Library
//!
//! This module provides the foundational pieces for building a high-performance,
//! Bazel-compatible re-implementation of the Sphinx documentation framework.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greeting() {
        assert_eq!(greeting("Rustacean"), "Hello, Rustacean!");
    }
}
