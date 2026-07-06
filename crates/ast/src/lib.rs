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

/// A Sphinx-style documentation domain (e.g. `py`, `c`).
///
/// Domains namespace directives and cross-reference roles so the same
/// object-type name (e.g. `function`) can mean different things in
/// different languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Domain {
    Py,
    C,
}

impl Domain {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Py => "py",
            Self::C => "c",
        }
    }
}

impl std::str::FromStr for Domain {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "py" => Ok(Self::Py),
            "c" => Ok(Self::C),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for Domain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Object types defined by the `py` domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PyObjectType {
    Function,
}

impl PyObjectType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
        }
    }
}

impl std::str::FromStr for PyObjectType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "function" => Ok(Self::Function),
            _ => Err(()),
        }
    }
}

/// Object types defined by the `c` domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CObjectType {
    Function,
}

impl CObjectType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
        }
    }
}

impl std::str::FromStr for CObjectType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "function" => Ok(Self::Function),
            _ => Err(()),
        }
    }
}

/// A domain together with one of its object types.
///
/// Each domain owns an independent object-type vocabulary (a `PyObjectType`
/// can never be mistaken for a `CObjectType`), and the domain is always
/// recoverable from the value itself via [`ObjectType::domain`] — there is
/// no separate `domain` field that could drift out of sync with the object
/// type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ObjectType {
    Py(PyObjectType),
    C(CObjectType),
}

impl ObjectType {
    #[must_use]
    pub const fn domain(&self) -> Domain {
        match self {
            Self::Py(_) => Domain::Py,
            Self::C(_) => Domain::C,
        }
    }

    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Py(t) => t.as_str(),
            Self::C(t) => t.as_str(),
        }
    }

    /// Parses a directive-style object-type name (e.g. `"function"` from
    /// `.. py:function::`) within a known domain.
    #[must_use]
    pub fn from_directive_name(domain: Domain, name: &str) -> Option<Self> {
        match domain {
            Domain::Py => name.parse::<PyObjectType>().ok().map(Self::Py),
            Domain::C => name.parse::<CObjectType>().ok().map(Self::C),
        }
    }

    /// Parses a role-style abbreviation (e.g. `"func"` from `:func:`) within
    /// a known domain. Roles use different (often abbreviated) names than
    /// their directive counterparts, matching real Sphinx.
    #[must_use]
    pub fn from_role_name(domain: Domain, role: &str) -> Option<Self> {
        match (domain, role) {
            (Domain::Py, "func") => Some(Self::Py(PyObjectType::Function)),
            (Domain::C, "func") => Some(Self::C(CObjectType::Function)),
            _ => None,
        }
    }
}

/// Extracts the referenceable name from a domain object signature.
///
/// Takes the text before the first `(` (or the whole string if there is
/// none), then its last whitespace-separated token — e.g. `"foo(bar)"` ->
/// `"foo"`, `"int foo(int bar)"` -> `"foo"`. A pointer return type like
/// `"char *foo(void)"` naively yields `"*foo"`, since real C declarator
/// parsing is out of scope (tracked in `spec_gaps.md`).
#[must_use]
pub fn extract_object_name(signature: &str) -> String {
    let before_parens = signature.split('(').next().unwrap_or(signature).trim();
    before_parens
        .split_whitespace()
        .next_back()
        .unwrap_or(before_parens)
        .to_string()
}

/// Builds the qualified [`TargetName`] key shared by domain object
/// registration (analyzer) and cross-reference resolution (renderer), so
/// both always agree on the key for the same object.
#[must_use]
pub fn build_domain_object_key(object_type: ObjectType, name: &str) -> TargetName {
    TargetName::new(&format!(
        "{}:{}:{}",
        object_type.domain().as_str(),
        object_type.as_str(),
        name
    ))
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
/// use rusty_sphinx_ast::term_id;
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
    DomainObject {
        object_type: ObjectType,
        signature: String,
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
    /// An inline cross-reference produced by a domain role (e.g. `:func:`,
    /// `:py:func:`, `:c:func:`), linking to a `Directive::DomainObject`.
    ///
    /// `object_type` is always concrete by the time this node exists — the
    /// parser resolves a bare (unprefixed) role via the file's default
    /// domain immediately, mirroring how `Directive::DomainObject` is
    /// resolved.
    DomainObjectReference {
        object_type: ObjectType,
        name: String,
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
    /// An RST comment (`.. text` or `..` followed by an indented body).
    /// Comments produce no output and are discarded during rendering.
    Comment,
    /// A transition (horizontal rule): 4+ repeated punctuation characters on their own
    /// line, blank-line-delimited. Renders as `<hr />`.
    Transition,
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

    #[test]
    fn test_domain_from_str_accepts_known_domains() {
        // Given / When / Then
        assert_eq!("py".parse::<Domain>(), Ok(Domain::Py));
        assert_eq!("c".parse::<Domain>(), Ok(Domain::C));
    }

    #[test]
    fn test_domain_from_str_rejects_unknown_domain() {
        // Given
        let input = "rust";

        // When
        let result = input.parse::<Domain>();

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_domain_as_str_and_display_round_trip() {
        // Given
        let domain = Domain::C;

        // When
        let s = domain.as_str();
        let displayed = domain.to_string();

        // Then
        assert_eq!(s, "c");
        assert_eq!(displayed, "c");
        assert_eq!(s.parse::<Domain>().unwrap(), domain);
    }

    #[test]
    fn test_domain_serialization_roundtrip() {
        // Given
        let domain = Domain::Py;

        // When
        let json = serde_json::to_string(&domain).expect("Failed to serialize");
        let deserialized: Domain = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(json, "\"py\"");
        assert_eq!(domain, deserialized);
    }

    #[test]
    fn test_py_object_type_from_str_accepts_function() {
        // Given / When / Then
        assert_eq!(
            "function".parse::<PyObjectType>(),
            Ok(PyObjectType::Function)
        );
    }

    #[test]
    fn test_py_object_type_from_str_rejects_unknown() {
        // Given
        let input = "class";

        // When
        let result = input.parse::<PyObjectType>();

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_c_object_type_from_str_accepts_function() {
        // Given / When / Then
        assert_eq!("function".parse::<CObjectType>(), Ok(CObjectType::Function));
    }

    #[test]
    fn test_c_object_type_from_str_rejects_unknown() {
        // Given
        let input = "struct";

        // When
        let result = input.parse::<CObjectType>();

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_object_type_domain_recovers_originating_domain() {
        // Given
        let py_type = ObjectType::Py(PyObjectType::Function);
        let c_type = ObjectType::C(CObjectType::Function);

        // When / Then
        assert_eq!(py_type.domain(), Domain::Py);
        assert_eq!(c_type.domain(), Domain::C);
    }

    #[test]
    fn test_object_type_as_str_returns_object_type_name() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Function);

        // When
        let s = object_type.as_str();

        // Then
        assert_eq!(s, "function");
    }

    #[test]
    fn test_object_type_from_directive_name_resolves_per_domain() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_directive_name(Domain::Py, "function"),
            Some(ObjectType::Py(PyObjectType::Function))
        );
        assert_eq!(
            ObjectType::from_directive_name(Domain::C, "function"),
            Some(ObjectType::C(CObjectType::Function))
        );
    }

    #[test]
    fn test_object_type_from_directive_name_rejects_unknown_object_type() {
        // Given
        let domain = Domain::Py;
        let name = "class";

        // When
        let result = ObjectType::from_directive_name(domain, name);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_object_type_from_role_name_resolves_per_domain() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::Py, "func"),
            Some(ObjectType::Py(PyObjectType::Function))
        );
        assert_eq!(
            ObjectType::from_role_name(Domain::C, "func"),
            Some(ObjectType::C(CObjectType::Function))
        );
    }

    #[test]
    fn test_object_type_from_role_name_rejects_unknown_role() {
        // Given
        let domain = Domain::Py;
        let role = "meth";

        // When
        let result = ObjectType::from_role_name(domain, role);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_object_type_serialization_roundtrip() {
        // Given
        let object_type = ObjectType::C(CObjectType::Function);

        // When
        let json = serde_json::to_string(&object_type).expect("Failed to serialize");
        let deserialized: ObjectType = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(object_type, deserialized);
    }

    #[test]
    fn test_extract_object_name_simple_call() {
        // Given
        let signature = "foo(bar)";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_object_name_no_parens() {
        // Given
        let signature = "foo";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_object_name_no_args() {
        // Given
        let signature = "foo()";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_object_name_c_style_return_type_prefix() {
        // Given
        let signature = "int foo(int bar)";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_object_name_extra_whitespace() {
        // Given
        let signature = "  foo   (bar)";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_object_name_empty_string() {
        // Given
        let signature = "";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "");
    }

    #[test]
    fn test_extract_object_name_pointer_return_type_is_naive() {
        // Given — documented limitation: no real C declarator parsing
        let signature = "char *foo(void)";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "*foo");
    }

    #[test]
    fn test_build_domain_object_key_produces_expected_format() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Function);
        let name = "foo";

        // When
        let key = build_domain_object_key(object_type, name);

        // Then
        assert_eq!(key.as_str(), "py:function:foo");
    }

    #[test]
    fn test_build_domain_object_key_distinguishes_domains() {
        // Given
        let py_type = ObjectType::Py(PyObjectType::Function);
        let c_type = ObjectType::C(CObjectType::Function);
        let name = "foo";

        // When
        let py_key = build_domain_object_key(py_type, name);
        let c_key = build_domain_object_key(c_type, name);

        // Then
        assert_ne!(py_key, c_key);
    }

    #[test]
    fn test_domain_object_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Function),
            name: "foo".to_string(),
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_domain_object_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::DomainObject {
            object_type: ObjectType::C(CObjectType::Function),
            signature: "int add(int a, int b)".to_string(),
            body: vec![Node::Paragraph(vec![InlineNode::Text(
                "Adds two numbers.".to_string(),
            )])],
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }
}
