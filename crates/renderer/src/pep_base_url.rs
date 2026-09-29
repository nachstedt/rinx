//! Where a `:pep:` role's link points: the PEP index every PEP's page sits
//! below, as `rinx.toml`'s `pep_base_url` names it — docutils'
//! `pep_base_url` setting, which a Sphinx project can only change in a
//! `docutils.conf`.
//!
//! A URL the *browser* follows, never a path the build reads, so it respects
//! the config's no-paths rule.

use serde::{Deserialize, Deserializer};

use crate::config::check_browser_address;

/// The address of a PEP index, ending in `/` so a PEP's page can be
/// appended to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PepBaseUrl(String);

impl PepBaseUrl {
    /// docutils' default, `https://peps.python.org/`.
    pub const DEFAULT: &'static str = "https://peps.python.org/";

    /// Accepts an `http(s)://` URL or a root-relative `/…` path ending in
    /// `/`.
    ///
    /// The trailing slash is required rather than added: docutils appends a
    /// page to the setting as written, so a value without one names a
    /// different address there, and guessing which was meant would hide it.
    ///
    /// # Errors
    ///
    /// Returns a message naming the accepted forms when `raw` is neither,
    /// when it does not end in `/`, or when it holds a character that could
    /// end an HTML attribute.
    pub fn parse(raw: &str) -> Result<Self, String> {
        check_browser_address(raw)?;
        if !raw.ends_with('/') {
            return Err(format!(
                "'{raw}' must end with '/', since each PEP's page is appended to it"
            ));
        }
        Ok(Self(raw.to_string()))
    }

    /// The URL as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for PepBaseUrl {
    fn default() -> Self {
        Self(Self::DEFAULT.to_string())
    }
}

impl<'de> Deserialize<'de> for PepBaseUrl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw)
            .map_err(|error| D::Error::custom(format!("invalid pep_base_url: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_docutils_default() {
        // Given / When / Then
        assert_eq!(PepBaseUrl::default().as_str(), "https://peps.python.org/");
        assert_eq!(
            PepBaseUrl::parse(PepBaseUrl::DEFAULT),
            Ok(PepBaseUrl::default())
        );
    }

    #[test]
    fn test_parse_accepts_an_absolute_or_root_relative_url() {
        // Given / When / Then
        for raw in ["https://example.org/peps/", "http://localhost/", "/peps/"] {
            assert_eq!(PepBaseUrl::parse(raw).unwrap().as_str(), raw);
        }
    }

    #[test]
    fn test_parse_refuses_a_missing_trailing_slash() {
        // Given / When
        let error = PepBaseUrl::parse("https://peps.python.org").unwrap_err();

        // Then
        assert!(error.contains("must end with '/'"), "{error}");
    }

    #[test]
    fn test_parse_refuses_a_page_relative_or_protocol_relative_address() {
        // Given / When / Then
        for raw in ["peps/", "//peps.python.org/", "ftp://peps/"] {
            assert!(PepBaseUrl::parse(raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn test_parse_refuses_a_character_that_ends_an_attribute() {
        // Given / When / Then
        assert!(PepBaseUrl::parse("https://a.org/\"x/").is_err());
        assert!(PepBaseUrl::parse("https://a.org/ x/").is_err());
    }
}
