//! Abstract Syntax Tree representations for the Rusty-Sphinx Document.

use serde::{Deserialize, Serialize};

/// A string bundled with its SHA-256 content hash.
///
/// The hash is computed from the body during construction and validated
/// during deserialization, so the invariant `hash == sha256(body)` always
/// holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "HashedContentRaw")]
pub struct HashedContent {
    hash: String,
    body: String,
}

impl HashedContent {
    /// Creates a new `HashedContent` by computing the SHA-256 hash of `body`.
    #[must_use]
    pub fn new(body: String) -> Self {
        use sha2::{Digest, Sha256};
        use std::fmt::Write as _;
        let mut hasher = Sha256::new();
        hasher.update(body.as_bytes());
        let hash = hasher.finalize().iter().fold(String::new(), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        });
        Self { hash, body }
    }

    /// Returns the hex-encoded SHA-256 hash of the body.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Returns the original body content.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }
}

/// Private helper for validated deserialization of [`HashedContent`].
#[derive(Deserialize)]
struct HashedContentRaw {
    hash: String,
    body: String,
}

impl TryFrom<HashedContentRaw> for HashedContent {
    type Error = String;

    fn try_from(raw: HashedContentRaw) -> Result<Self, Self::Error> {
        let reconstructed = Self::new(raw.body);
        if reconstructed.hash != raw.hash {
            return Err(format!(
                "Hash mismatch: expected {}, got {}",
                reconstructed.hash, raw.hash
            ));
        }
        Ok(reconstructed)
    }
}

/// A normalized reST target name.
///
/// reST target names are case-insensitive and all internal whitespace
/// is collapsed to a single space. This opaque type enforces that invariant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TargetName(String);

impl TargetName {
    /// Creates a new `TargetName`, applying whitespace collapse and lowercasing.
    #[must_use]
    pub fn new(raw: &str) -> Self {
        let normalized = raw
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        Self(normalized)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdmonitionKind {
    Attention,
    Caution,
    Danger,
    Error,
    Hint,
    Important,
    Note,
    Tip,
    Warning,
    Admonition,
}

impl AdmonitionKind {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Attention => "attention",
            Self::Caution => "caution",
            Self::Danger => "danger",
            Self::Error => "error",
            Self::Hint => "hint",
            Self::Important => "important",
            Self::Note => "note",
            Self::Tip => "tip",
            Self::Warning => "warning",
            Self::Admonition => "admonition",
        }
    }
}

impl std::str::FromStr for AdmonitionKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "attention" => Ok(Self::Attention),
            "caution" => Ok(Self::Caution),
            "danger" => Ok(Self::Danger),
            "error" => Ok(Self::Error),
            "hint" => Ok(Self::Hint),
            "important" => Ok(Self::Important),
            "note" => Ok(Self::Note),
            "tip" => Ok(Self::Tip),
            "warning" => Ok(Self::Warning),
            "admonition" => Ok(Self::Admonition),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for AdmonitionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Distinguishes the three Sphinx version-change directives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionChangeKind {
    Added,
    Changed,
    Deprecated,
}

impl VersionChangeKind {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Added => "versionadded",
            Self::Changed => "versionchanged",
            Self::Deprecated => "deprecated",
        }
    }
}

impl std::str::FromStr for VersionChangeKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "versionadded" => Ok(Self::Added),
            "versionchanged" => Ok(Self::Changed),
            "deprecated" => Ok(Self::Deprecated),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for VersionChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single entry in a `.. glossary::` directive.
///
/// Each entry groups one or more terms (all sharing the same definition)
/// together with the parsed definition body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlossaryEntry {
    /// One or more terms that share this definition.
    pub terms: Vec<String>,
    /// The definition body, parsed as block-level RST nodes.
    pub definition: Vec<Node>,
}

/// Generates the HTML anchor `id` for a glossary term.
///
/// Lowercases the term and replaces runs of whitespace with hyphens,
/// then prepends `"term-"`. This is consistent with Sphinx's HTML output.
///
/// # Examples
///
/// ```
/// use rusty_sphinx::ast::term_id;
/// assert_eq!(term_id("Environment Variable"), "term-environment-variable");
/// assert_eq!(term_id("python"), "term-python");
/// ```
#[must_use]
pub fn term_id(term: &str) -> String {
    let normalized = term
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    format!("term-{normalized}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Directive {
    Toctree {
        paths: Vec<String>,
        maxdepth: Option<usize>,
        ignored_options: Vec<String>,
    },
    PlantUml(HashedContent),
    Admonition {
        kind: AdmonitionKind,
        title: Option<String>,
        collapsible: Option<bool>,
        body: Vec<Node>,
    },
    VersionChange {
        kind: VersionChangeKind,
        version: String,
        body: Vec<Node>,
    },
    SeeAlso {
        body: Vec<Node>,
    },
    Glossary {
        entries: Vec<GlossaryEntry>,
        sorted: bool,
    },
    Unknown {
        name: String,
        argument: String,
        body: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlineNode {
    Text(String),
    Reference(String),
    Hyperlink {
        text: String,
        target: String,
    },
    AnonymousReference(String),
    AnonymousHyperlink {
        text: String,
        target: String,
    },
    Emphasis(String),
    Strong(String),
    Literal(String),
    Program(String),
    /// An inline cross-reference produced by the term role, linking to a glossary entry.
    ///
    /// The `display` field is the visible link text and `term` is the glossary key.
    /// They differ when the role is written with an explicit display-text override,
    /// i.e. the angle-bracket form where the text before the angle bracket is shown
    /// and the text inside the angle brackets is looked up in the glossary index.
    TermReference {
        display: String,
        term: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BulletListItem {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Node {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph(Vec<InlineNode>),
    Directive(Directive),
    Target {
        name: TargetName,
        uri: Option<String>,
    },
    AnonymousTarget {
        uri: String,
    },
    BulletList {
        bullet: char,
        items: Vec<BulletListItem>,
    },
    LiteralBlock {
        /// The language hint (e.g. `"python"`), if specified via `.. code-block:: lang`.
        /// `None` for plain `::` paragraph-introduced blocks.
        language: Option<String>,
        /// Verbatim content with common leading indentation stripped.
        content: String,
    },
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub path: String,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

impl Document {
    #[must_use]
    pub const fn new(path: String, nodes: Vec<Node>) -> Self {
        Self {
            path,
            nodes,
            diagnostics: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{HashedContent, InlineNode};

    #[test]
    fn test_new_creates_document_with_given_nodes() {
        // Given
        let nodes = vec![
            Node::Heading {
                level: 1,
                text: "Title".to_string(),
            },
            Node::Paragraph(vec![InlineNode::Text("Body".to_string())]),
        ];

        // When
        let doc = Document::new("test.rst".to_string(), nodes);

        // Then
        assert_eq!(doc.nodes.len(), 2);
    }

    #[test]
    fn test_hashed_content_new_computes_consistent_hash() {
        // Given
        let body = "A -> B".to_string();

        // When
        let a = HashedContent::new(body.clone());
        let b = HashedContent::new(body);

        // Then
        assert_eq!(a.hash(), b.hash());
        assert!(!a.hash().is_empty());
    }

    #[test]
    fn test_hashed_content_deserialization_rejects_mismatched_hash() {
        // Given — JSON with a fabricated hash that does not match the body
        let json = r#"{"hash":"0000000000000000000000000000000000000000000000000000000000000000","body":"A -> B"}"#;

        // When
        let result: Result<HashedContent, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_hashed_content_deserialization_accepts_correct_hash() {
        // Given — construct via new(), serialize, then deserialize
        let original = HashedContent::new("hello world".to_string());
        let json = serde_json::to_string(&original).unwrap();

        // When
        let deserialized: HashedContent = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_hashed_content_try_from_success() {
        // Given
        let body = "Valid body".to_string();
        let valid_hash = HashedContent::new(body.clone()).hash;
        let raw = HashedContentRaw {
            hash: valid_hash.clone(),
            body: body.clone(),
        };

        // When
        let result = HashedContent::try_from(raw);

        // Then
        assert!(result.is_ok());
        let content = result.unwrap();
        assert_eq!(content.hash, valid_hash);
        assert_eq!(content.body, body);
    }

    #[test]
    fn test_hashed_content_try_from_mismatch() {
        // Given
        let body = "Valid body".to_string();
        let valid_hash = HashedContent::new(body.clone()).hash;
        let invalid_hash =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        let raw = HashedContentRaw {
            hash: invalid_hash.clone(),
            body,
        };

        // When
        let result = HashedContent::try_from(raw);

        // Then
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            format!("Hash mismatch: expected {valid_hash}, got {invalid_hash}")
        );
    }

    #[test]
    fn test_hashed_content_try_from_empty_body() {
        // Given
        let body = String::new();
        let valid_hash = HashedContent::new(body.clone()).hash;
        let raw = HashedContentRaw {
            hash: valid_hash.clone(),
            body: body.clone(),
        };

        // When
        let result = HashedContent::try_from(raw);

        // Then
        assert!(result.is_ok());
        let content = result.unwrap();
        assert_eq!(content.hash, valid_hash);
        assert_eq!(content.body, body);
    }

    #[test]
    fn test_target_name_normalization() {
        // Given
        let raw = "  My   Target  Name  ";

        // When
        let target = TargetName::new(raw);

        // Then
        assert_eq!(target.as_str(), "my target name");
    }

    #[test]
    fn test_target_name_equality() {
        // Given
        let raw1 = "My Target";
        let raw2 = "my   target";
        let raw3 = "  MY TARGET  ";
        let raw4 = "Different Target";

        // When
        let target1 = TargetName::new(raw1);
        let target2 = TargetName::new(raw2);
        let target3 = TargetName::new(raw3);
        let target4 = TargetName::new(raw4);

        // Then
        assert_eq!(target1, target2);
        assert_eq!(target1, target3);
        assert_eq!(target2, target3);

        assert_ne!(target1, target4);
    }

    #[test]
    fn test_inline_node_program_serialization_roundtrip() {
        // Given
        let node = InlineNode::Program("curl".to_string());

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_version_change_kind_serialization_roundtrip() {
        // Given
        let kind = VersionChangeKind::Deprecated;

        // When
        let json = serde_json::to_string(&kind).expect("Failed to serialize");
        let deserialized: VersionChangeKind =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(json, "\"deprecated\"");
        assert_eq!(kind, deserialized);
    }

    #[test]
    fn test_term_id_single_word() {
        // Given
        let term = "python";

        // When
        let id = term_id(term);

        // Then
        assert_eq!(id, "term-python");
    }

    #[test]
    fn test_term_id_multi_word_joins_with_hyphens() {
        // Given
        let term = "Environment Variable";

        // When
        let id = term_id(term);

        // Then
        assert_eq!(id, "term-environment-variable");
    }

    #[test]
    fn test_term_id_normalizes_to_lowercase() {
        // Given
        let term = "MY TERM";

        // When
        let id = term_id(term);

        // Then
        assert_eq!(id, "term-my-term");
    }

    #[test]
    fn test_term_id_collapses_extra_whitespace() {
        // Given
        let term = "  term   with   spaces  ";

        // When
        let id = term_id(term);

        // Then
        assert_eq!(id, "term-term-with-spaces");
    }

    #[test]
    fn test_glossary_entry_serialization_roundtrip() {
        // Given
        let entry = GlossaryEntry {
            terms: vec!["foo".to_string(), "bar".to_string()],
            definition: vec![Node::Paragraph(vec![InlineNode::Text(
                "A definition.".to_string(),
            )])],
        };

        // When
        let json = serde_json::to_string(&entry).expect("Failed to serialize");
        let deserialized: GlossaryEntry =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn test_glossary_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::Glossary {
            entries: vec![GlossaryEntry {
                terms: vec!["term".to_string()],
                definition: vec![],
            }],
            sorted: true,
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_term_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::TermReference {
            display: "the environment".to_string(),
            term: "environment".to_string(),
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_term_reference_display_equals_term_when_no_alias() {
        // Given
        let term_text = "environment";

        // When
        let node = InlineNode::TermReference {
            display: term_text.to_string(),
            term: term_text.to_string(),
        };

        // Then
        if let InlineNode::TermReference { display, term } = node {
            assert_eq!(display, term);
        } else {
            panic!("Expected TermReference");
        }
    }
}
