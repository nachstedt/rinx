use serde::{Deserialize, Serialize};

use crate::non_empty_vector::NonEmptyVector;

/// A version comparison operator from a `:pyversion:` option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VersionComparison {
    Lt,
    Le,
    Eq,
    Ne,
    Ge,
    Gt,
}

impl VersionComparison {
    /// Operators longest-first, so `>=` is matched before `>` when scanning a
    /// clause's prefix.
    const BY_DESCENDING_LENGTH: [(&'static str, Self); 6] = [
        (">=", Self::Ge),
        ("<=", Self::Le),
        ("==", Self::Eq),
        ("!=", Self::Ne),
        (">", Self::Gt),
        ("<", Self::Lt),
    ];

    /// The operator's spelling, as written in the option and as handed back to
    /// the Python runner.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Ge => ">=",
            Self::Gt => ">",
        }
    }
}

/// A dotted release version such as `3`, `3.11` or `3.11.2`.
///
/// Serializes as its dotted string form (see the hand-written [`Serialize`]
/// impl below) so an `.ast` file stays readable, and re-parses on the way back
/// in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct PythonVersion(NonEmptyVector<u32>);

impl PythonVersion {
    /// Parses dot-separated release segments, e.g. `"3.11"`.
    ///
    /// # Errors
    ///
    /// Returns the offending text when a segment is empty or not a number.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let mut segments = Vec::new();
        for segment in raw.split('.') {
            let parsed = segment
                .parse::<u32>()
                .map_err(|_| format!("invalid version segment {segment:?} in {raw:?}"))?;
            segments.push(parsed);
        }
        let non_empty = NonEmptyVector::try_from(segments)
            .map_err(|_| format!("version {raw:?} has no release segments"))?;
        Ok(Self(non_empty))
    }

    /// The release segments, most significant first.
    #[must_use]
    pub fn segments(&self) -> &[u32] {
        self.0.as_slice()
    }
}

impl std::fmt::Display for PythonVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rendered: Vec<String> = self.segments().iter().map(u32::to_string).collect();
        write!(f, "{}", rendered.join("."))
    }
}

impl TryFrom<String> for PythonVersion {
    type Error = String;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(&raw)
    }
}

impl Serialize for PythonVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// One comparison clause, e.g. `>= 3.5` or `!= 3.7.*`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PyVersionClause {
    pub comparison: VersionComparison,
    pub version: PythonVersion,
    /// Whether the version carried a trailing `.*` wildcard (`==3.7.*`).
    pub wildcard: bool,
}

impl std::fmt::Display for PyVersionClause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let suffix = if self.wildcard { ".*" } else { "" };
        write!(f, "{}{}{}", self.comparison.as_str(), self.version, suffix)
    }
}

/// The `:pyversion:` option — a comma-separated set of version clauses, all of
/// which must hold.
///
/// # Deliberately not evaluated here
///
/// Sphinx compares the spec against the *running* interpreter
/// (`sys.version_info`) and skips the block when it does not match. Evaluating
/// at parse time would bake one interpreter's answer into the `.ast`, so this
/// type only ever *parses* the spec into typed intent; the Python runner
/// decides. That also keeps the interpreter version part of the test action's
/// cache key rather than of the parsed document.
///
/// # Supported subset
///
/// Sphinx delegates to `packaging.SpecifierSet`, which accepts all of PEP 440.
/// This type covers the six comparison operators over dotted release segments,
/// plus the `.*` wildcard — which spans the realistic surface for Python
/// version checks. Anything beyond that is rejected so the parser can report it,
/// rather than being silently dropped: silently ignoring a `:pyversion:` would
/// *run* a block that Sphinx would have skipped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PyVersionSpec(NonEmptyVector<PyVersionClause>);

impl PyVersionSpec {
    /// Parses a `:pyversion:` value such as `">= 3.5"` or `">=3.5, <3.9"`.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending clause when it uses an operator
    /// outside the supported subset, omits an operator, or carries a malformed
    /// version.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let mut clauses = Vec::new();
        for clause in raw.split(',') {
            let trimmed = clause.trim();
            if trimmed.is_empty() {
                return Err(format!("empty version clause in {raw:?}"));
            }
            clauses.push(Self::parse_clause(trimmed)?);
        }
        let non_empty = NonEmptyVector::try_from(clauses)
            .map_err(|_| format!("version spec {raw:?} has no clauses"))?;
        Ok(Self(non_empty))
    }

    /// Parses a single clause, e.g. `">= 3.5"`.
    fn parse_clause(clause: &str) -> Result<PyVersionClause, String> {
        let (comparison, rest) = VersionComparison::BY_DESCENDING_LENGTH
            .iter()
            .find_map(|(spelling, comparison)| {
                clause
                    .strip_prefix(spelling)
                    .map(|rest| (*comparison, rest))
            })
            .ok_or_else(|| {
                format!("version clause {clause:?} must start with one of >=, <=, ==, !=, >, <")
            })?;

        let version_text = rest.trim();
        let (version_text, wildcard) = match version_text.strip_suffix(".*") {
            Some(stripped) => (stripped, true),
            None => (version_text, false),
        };

        Ok(PyVersionClause {
            comparison,
            version: PythonVersion::parse(version_text)?,
            wildcard,
        })
    }

    /// The clauses, in the order written.
    #[must_use]
    pub fn clauses(&self) -> &[PyVersionClause] {
        self.0.as_slice()
    }
}

impl std::fmt::Display for PyVersionSpec {
    /// Renders a canonical form the Python runner can hand straight to
    /// `packaging.SpecifierSet`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rendered: Vec<String> = self.clauses().iter().map(ToString::to_string).collect();
        write!(f, "{}", rendered.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_version_parses_a_two_segment_version() {
        // Given
        let raw = "3.11";

        // When
        let version = PythonVersion::parse(raw).expect("should parse");

        // Then
        assert_eq!(version.segments(), &[3, 11]);
    }

    #[test]
    fn test_python_version_parses_a_single_segment_version() {
        // Given
        let raw = "3";

        // When
        let version = PythonVersion::parse(raw).expect("should parse");

        // Then
        assert_eq!(version.segments(), &[3]);
    }

    #[test]
    fn test_python_version_parses_a_three_segment_version() {
        // Given
        let raw = "3.11.2";

        // When
        let version = PythonVersion::parse(raw).expect("should parse");

        // Then
        assert_eq!(version.segments(), &[3, 11, 2]);
    }

    #[test]
    fn test_python_version_rejects_a_non_numeric_segment() {
        // Given
        let raw = "3.x";

        // When
        let result = PythonVersion::parse(raw);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_python_version_rejects_an_empty_segment() {
        // Given
        let raw = "3..1";

        // When
        let result = PythonVersion::parse(raw);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_python_version_displays_its_dotted_form() {
        // Given
        let version = PythonVersion::parse("3.11").expect("should parse");

        // When
        let rendered = version.to_string();

        // Then
        assert_eq!(rendered, "3.11");
    }

    #[test]
    fn test_python_version_serializes_transparently_as_a_string() {
        // Given
        let version = PythonVersion::parse("3.11").expect("should parse");

        // When
        let json = serde_json::to_string(&version).expect("Failed to serialize");

        // Then
        assert_eq!(json, "\"3.11\"");
    }

    #[test]
    fn test_python_version_serialization_roundtrip() {
        // Given
        let version = PythonVersion::parse("3.11.2").expect("should parse");

        // When
        let json = serde_json::to_string(&version).expect("Failed to serialize");
        let deserialized: PythonVersion =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(version, deserialized);
    }

    #[test]
    fn test_python_version_deserialization_rejects_a_malformed_value() {
        // Given
        let json = "\"3.x\"";

        // When
        let result: Result<PythonVersion, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_reads_a_single_clause_with_a_space_after_the_operator() {
        // Given — the spelling CPython's docs use.
        let raw = ">= 3.5";

        // When
        let spec = PyVersionSpec::parse(raw).expect("should parse");

        // Then
        assert_eq!(spec.clauses().len(), 1);
        assert_eq!(spec.clauses()[0].comparison, VersionComparison::Ge);
        assert_eq!(spec.clauses()[0].version.segments(), &[3, 5]);
        assert!(!spec.clauses()[0].wildcard);
    }

    #[test]
    fn test_parse_reads_a_single_clause_without_a_space() {
        // Given
        let raw = ">=3.5";

        // When
        let spec = PyVersionSpec::parse(raw).expect("should parse");

        // Then
        assert_eq!(spec.clauses()[0].comparison, VersionComparison::Ge);
    }

    #[test]
    fn test_parse_reads_several_comma_separated_clauses() {
        // Given
        let raw = ">=3.5, <3.9";

        // When
        let spec = PyVersionSpec::parse(raw).expect("should parse");

        // Then
        assert_eq!(spec.clauses().len(), 2);
        assert_eq!(spec.clauses()[0].comparison, VersionComparison::Ge);
        assert_eq!(spec.clauses()[1].comparison, VersionComparison::Lt);
        assert_eq!(spec.clauses()[1].version.segments(), &[3, 9]);
    }

    #[test]
    fn test_parse_prefers_the_two_character_operator_over_its_prefix() {
        // Given — `>=` must not be read as `>` followed by `=3.5`.
        let raw = ">=3.5";

        // When
        let spec = PyVersionSpec::parse(raw).expect("should parse");

        // Then
        assert_eq!(spec.clauses()[0].comparison, VersionComparison::Ge);
        assert_eq!(spec.clauses()[0].version.segments(), &[3, 5]);
    }

    #[test]
    fn test_parse_reads_a_trailing_wildcard() {
        // Given
        let raw = "!=3.7.*";

        // When
        let spec = PyVersionSpec::parse(raw).expect("should parse");

        // Then
        assert_eq!(spec.clauses()[0].comparison, VersionComparison::Ne);
        assert_eq!(spec.clauses()[0].version.segments(), &[3, 7]);
        assert!(spec.clauses()[0].wildcard);
    }

    #[test]
    fn test_parse_reads_every_supported_operator() {
        // Given
        let cases = [
            (">=3.5", VersionComparison::Ge),
            ("<=3.5", VersionComparison::Le),
            ("==3.5", VersionComparison::Eq),
            ("!=3.5", VersionComparison::Ne),
            (">3.5", VersionComparison::Gt),
            ("<3.5", VersionComparison::Lt),
        ];

        for (raw, expected) in cases {
            // When
            let spec = PyVersionSpec::parse(raw).expect("should parse");

            // Then
            assert_eq!(spec.clauses()[0].comparison, expected, "for {raw:?}");
        }
    }

    #[test]
    fn test_parse_rejects_a_clause_without_an_operator() {
        // Given — a plausible authoring mistake that must not be silently
        // ignored, since ignoring it would run a block Sphinx would skip.
        let raw = "3.5";

        // When
        let result = PyVersionSpec::parse(raw);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rejects_an_unsupported_operator() {
        // Given — PEP 440's compatible-release operator, outside the subset.
        let raw = "~=3.5";

        // When
        let result = PyVersionSpec::parse(raw);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rejects_an_empty_clause_between_commas() {
        // Given
        let raw = ">=3.5,,<3.9";

        // When
        let result = PyVersionSpec::parse(raw);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rejects_an_empty_spec() {
        // Given
        let raw = "";

        // When
        let result = PyVersionSpec::parse(raw);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_display_renders_a_form_packaging_can_consume() {
        // Given
        let spec = PyVersionSpec::parse(">= 3.5, < 3.9").expect("should parse");

        // When
        let rendered = spec.to_string();

        // Then — normalized: no spaces, operators preserved.
        assert_eq!(rendered, ">=3.5,<3.9");
    }

    #[test]
    fn test_display_preserves_a_wildcard() {
        // Given
        let spec = PyVersionSpec::parse("!= 3.7.*").expect("should parse");

        // When
        let rendered = spec.to_string();

        // Then
        assert_eq!(rendered, "!=3.7.*");
    }

    #[test]
    fn test_display_output_reparses_to_the_same_spec() {
        // Given
        let spec = PyVersionSpec::parse(">= 3.5, != 3.7.*").expect("should parse");

        // When
        let reparsed = PyVersionSpec::parse(&spec.to_string()).expect("should reparse");

        // Then
        assert_eq!(spec, reparsed);
    }

    #[test]
    fn test_serialization_roundtrip() {
        // Given
        let spec = PyVersionSpec::parse(">=3.5,<3.9").expect("should parse");

        // When
        let json = serde_json::to_string(&spec).expect("Failed to serialize");
        let deserialized: PyVersionSpec =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(spec, deserialized);
    }
}
