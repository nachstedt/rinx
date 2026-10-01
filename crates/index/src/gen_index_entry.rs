use serde::{Deserialize, Serialize};

/// One entry in the site-wide general index (`genindex.html`), sourced
/// either from a `.. index::` directive or automatically from a domain
/// object definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenIndexEntry {
    /// The main, alphabetized term (e.g. `"execution"`, `"Greeter.greet (method)"`).
    pub primary: String,
    /// An optional nested sub-term (e.g. `"context"` in `single: execution; context`).
    pub subentry: Option<String>,
    /// Whether this occurrence should be emphasized as the entry's primary
    /// definition (from a leading `!` in a `.. index::` entry).
    pub main: bool,
    /// The document this entry's anchor lives on.
    pub doc_path: String,
    /// The HTML anchor `id` on `doc_path` this entry links to.
    pub anchor: String,
}

/// A `see:` or `seealso:` entry in the general index: it sends the reader
/// from `primary` to another entry, `target`, rather than to a place in a
/// document, so it has no location — which is why it is a type of its own
/// rather than a [`GenIndexEntry`] with an optional anchor.
///
/// Sphinx files it under `primary` as an unlinked subentry reading
/// `see <target>` (or `see also <target>`); [`Self::subentry_text`] is that
/// text. `target` is not checked against the index, as Sphinx does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenIndexRedirect {
    /// The term the redirect is listed under.
    pub primary: String,
    /// Whether the reader is sent away (`see`) or also elsewhere (`seealso`).
    pub kind: GenIndexRedirectKind,
    /// The entry the reader is sent to.
    pub target: String,
}

/// Which of Sphinx's two redirecting entry types a [`GenIndexRedirect`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenIndexRedirectKind {
    See,
    SeeAlso,
}

impl GenIndexRedirect {
    /// The subentry the general index lists under `primary`: Sphinx's
    /// `see %s` or `see also %s`.
    #[must_use]
    pub fn subentry_text(&self) -> String {
        match self.kind {
            GenIndexRedirectKind::See => format!("see {}", self.target),
            GenIndexRedirectKind::SeeAlso => format!("see also {}", self.target),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redirect(kind: GenIndexRedirectKind) -> GenIndexRedirect {
        GenIndexRedirect {
            primary: "goto".to_string(),
            kind,
            target: "jump".to_string(),
        }
    }

    #[test]
    fn test_subentry_text_reads_see_for_a_see_redirect() {
        // Given / When / Then
        assert_eq!(
            redirect(GenIndexRedirectKind::See).subentry_text(),
            "see jump"
        );
    }

    #[test]
    fn test_subentry_text_reads_see_also_for_a_seealso_redirect() {
        // Given / When / Then
        assert_eq!(
            redirect(GenIndexRedirectKind::SeeAlso).subentry_text(),
            "see also jump"
        );
    }

    #[test]
    fn test_gen_index_redirect_serialization_roundtrip() {
        // Given
        let original = redirect(GenIndexRedirectKind::SeeAlso);

        // When
        let json = serde_json::to_string(&original).unwrap();
        let loaded: GenIndexRedirect = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(loaded, original);
    }
}
