//! Where a registry role's link points when the site may choose: the PEP
//! index `:pep:` and `:pep-reference:` link into and the RFC index `:rfc:`
//! and `:rfc-reference:` link into, as `rinx.toml`'s `pep_base_url` and
//! `rfc_base_url` name them — docutils' settings of those names, which a
//! Sphinx project can only change in a `docutils.conf`. (`:cve:` and `:cwe:`
//! link to addresses Sphinx hard-codes, so they have no setting.)
//!
//! Both are URLs the *browser* follows, never paths the build reads, so they
//! respect the config's no-paths rule. One type serves both, told apart by a
//! [`BaseUrlSetting`] marker, so a PEP index cannot be passed where an RFC
//! index is expected and each setting's error still names its own key.

use std::marker::PhantomData;

use serde::{Deserialize, Deserializer};

use crate::config::check_browser_address;

/// One configurable registry index: its `rinx.toml` key, its default, and
/// what is appended to it, for the error a missing trailing slash reports.
pub trait BaseUrlSetting {
    /// The `rinx.toml` key the URL is read from.
    const KEY: &'static str;
    /// The URL when the key is absent.
    const DEFAULT: &'static str;
    /// What each link appends to the URL, as an error message names it.
    const APPENDED: &'static str;
}

/// The PEP index: docutils' `pep_base_url`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PepIndex {}

impl BaseUrlSetting for PepIndex {
    const KEY: &'static str = "pep_base_url";
    const DEFAULT: &'static str = "https://peps.python.org/";
    const APPENDED: &'static str = "each PEP's page";
}

/// The RFC index: docutils' `rfc_base_url`, defaulting to the address Sphinx
/// sets in place of docutils' own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RfcIndex {}

impl BaseUrlSetting for RfcIndex {
    const KEY: &'static str = "rfc_base_url";
    const DEFAULT: &'static str = "https://datatracker.ietf.org/doc/html/";
    const APPENDED: &'static str = "each RFC's page";
}

/// The address of a registry index, ending in `/` so a document's page can
/// be appended to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryBaseUrl<S> {
    url: String,
    setting: PhantomData<S>,
}

/// The PEP index a `:pep:` or `:pep-reference:` links into.
pub type PepBaseUrl = RegistryBaseUrl<PepIndex>;

/// The RFC index an `:rfc:` or `:rfc-reference:` links into.
pub type RfcBaseUrl = RegistryBaseUrl<RfcIndex>;

impl<S: BaseUrlSetting> RegistryBaseUrl<S> {
    /// The setting's default.
    pub const DEFAULT: &'static str = S::DEFAULT;

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
                "'{raw}' must end with '/', since {} is appended to it",
                S::APPENDED
            ));
        }
        Ok(Self {
            url: raw.to_string(),
            setting: PhantomData,
        })
    }

    /// The URL as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.url
    }
}

impl<S: BaseUrlSetting> Default for RegistryBaseUrl<S> {
    fn default() -> Self {
        Self {
            url: S::DEFAULT.to_string(),
            setting: PhantomData,
        }
    }
}

impl<'de, S: BaseUrlSetting> Deserialize<'de> for RegistryBaseUrl<S> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(|error| D::Error::custom(format!("invalid {}: {error}", S::KEY)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_each_setting_default() {
        // Given / When / Then
        assert_eq!(PepBaseUrl::default().as_str(), "https://peps.python.org/");
        assert_eq!(
            RfcBaseUrl::default().as_str(),
            "https://datatracker.ietf.org/doc/html/"
        );
        assert_eq!(
            PepBaseUrl::parse(PepBaseUrl::DEFAULT),
            Ok(PepBaseUrl::default())
        );
        assert_eq!(
            RfcBaseUrl::parse(RfcBaseUrl::DEFAULT),
            Ok(RfcBaseUrl::default())
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
    fn test_parse_refuses_a_missing_trailing_slash_naming_what_is_appended() {
        // Given / When
        let pep = PepBaseUrl::parse("https://peps.python.org").unwrap_err();
        let rfc = RfcBaseUrl::parse("https://rfc.example").unwrap_err();

        // Then
        assert!(pep.contains("must end with '/'"), "{pep}");
        assert!(pep.contains("each PEP's page"), "{pep}");
        assert!(rfc.contains("each RFC's page"), "{rfc}");
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

    #[test]
    fn test_deserialize_names_the_setting_key() {
        // Given / When
        let error = serde_json::from_str::<RfcBaseUrl>("\"https://x.org\"").unwrap_err();

        // Then
        assert!(
            error.to_string().contains("invalid rfc_base_url"),
            "{error}"
        );
    }
}
