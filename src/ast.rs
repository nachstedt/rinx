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
    Hyperlink { text: String, target: String },
    AnonymousReference(String),
    AnonymousHyperlink { text: String, target: String },
    Emphasis(String),
    Strong(String),
    Literal(String),
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

    /// Returns the text of the first level 1 heading, if any.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.nodes.iter().find_map(|n| {
            if let Node::Heading { level: 1, text } = n {
                Some(text.as_str())
            } else {
                None
            }
        })
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
    fn test_target_name_normalization() {
        // Given
        let raw = "  My   Target  Name  ";

        // When
        let target = TargetName::new(raw);

        // Then
        assert_eq!(target.as_str(), "my target name");
    }
}
