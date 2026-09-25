use serde::{Deserialize, Serialize};

/// Extracts the referenceable name from a `c:*` domain object signature by
/// crude string munging, e.g. `"int foo(int bar)"` -> `"foo"`.
///
/// Takes the text before the first `(` and its last whitespace-separated
/// token, then strips a pointer-return-type sigil (`*`, `**`, ...) that ends
/// up glued to the front of the name when the author writes `Type *name(...)`
/// rather than `Type* name(...)` — e.g. `CPython`'s
/// `"PyObject *PyUnicode_FromString(const char *str)"` ->
/// `"PyUnicode_FromString"`.
///
/// This is **only** the fallback for [`CSignature::parse`], used when
/// `rinx_cdecl` cannot parse a signature at all. It is not real C
/// declarator parsing and gets whole shapes wrong — most visibly
/// function-pointer typedefs, where the name lives inside a `(*name)` group
/// the heuristic never looks at. Prefer the parser; this exists so that an
/// unparseable signature still yields *some* cross-reference target rather
/// than none.
#[must_use]
pub fn extract_c_object_name(signature: &str) -> String {
    let before_parens = signature.split('(').next().unwrap_or(signature).trim();
    let last_token = before_parens
        .split_whitespace()
        .next_back()
        .unwrap_or(before_parens);
    last_token.trim_start_matches('*').to_string()
}

/// How a [`CSignature`]'s name was arrived at.
///
/// Recorded rather than discarded because it is what the parser reports as a
/// diagnostic: a `Fallback` signature is one whose grammar this project does
/// not yet cover, and counting those against a real corpus is how that
/// coverage gets measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NameSource {
    /// `rinx_cdecl` parsed the declaration and returned its name.
    Parsed,
    /// The declaration did not parse; [`extract_c_object_name`] guessed.
    Fallback,
}

/// A `c:*` domain object signature together with the name it declares.
///
/// The name is derived once, when the document is parsed, and travels in the
/// serialized AST — so the analyze and render phases read it instead of
/// re-deriving it, and a signature is never parsed differently in two places.
///
/// Follows the same "parse, don't validate" pattern as
/// [`crate::HashedContent`]: the invariant (`name` and `name_source` are what
/// re-deriving them from `text` produces) is established by the smart
/// constructor and re-checked on deserialization, so a stale or hand-edited
/// `.ast` cannot smuggle in a name that disagrees with its signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CSignatureRaw")]
pub struct CSignature {
    text: String,
    name: String,
    name_source: NameSource,
}

impl CSignature {
    /// Derives the declared name from `text`, preferring the real declaration
    /// parser and falling back to [`extract_c_object_name`].
    ///
    /// Deliberately infallible. A signature this project cannot parse is
    /// still a documented object that other pages link to, so dropping it
    /// would turn an imperfect anchor into a broken one.
    #[must_use]
    pub fn parse(text: String) -> Self {
        let Ok(name) = rinx_cdecl::declared_name(&text) else {
            let name = extract_c_object_name(&text);
            return Self {
                text,
                name,
                name_source: NameSource::Fallback,
            };
        };
        Self {
            text,
            name,
            name_source: NameSource::Parsed,
        }
    }

    /// The signature exactly as written, which is what gets rendered.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The declared name, used as the cross-reference target key.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether [`Self::name`] was parsed or guessed.
    #[must_use]
    pub fn name_source(&self) -> NameSource {
        self.name_source
    }
}

impl From<&str> for CSignature {
    fn from(text: &str) -> Self {
        Self::parse(text.to_string())
    }
}

impl From<String> for CSignature {
    fn from(text: String) -> Self {
        Self::parse(text)
    }
}

/// Private helper for validated deserialization of [`CSignature`].
#[derive(Deserialize)]
struct CSignatureRaw {
    text: String,
    name: String,
    name_source: NameSource,
}

impl TryFrom<CSignatureRaw> for CSignature {
    type Error = String;

    fn try_from(raw: CSignatureRaw) -> Result<Self, Self::Error> {
        let reconstructed = Self::parse(raw.text);
        if reconstructed.name != raw.name {
            return Err(format!(
                "Signature name mismatch: expected {}, got {}",
                reconstructed.name, raw.name
            ));
        }
        if reconstructed.name_source != raw.name_source {
            return Err(format!(
                "Signature name source mismatch: expected {:?}, got {:?}",
                reconstructed.name_source, raw.name_source
            ));
        }
        Ok(reconstructed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uses_the_declaration_parser_for_a_well_formed_signature() {
        // Given — a function-pointer typedef, which only a real declarator
        // parser gets right.
        let text = "int (*Py_tracefunc)(PyObject *obj, int what)".to_string();

        // When
        let signature = CSignature::parse(text);

        // Then
        assert_eq!(signature.name(), "Py_tracefunc");
        assert_eq!(signature.name_source(), NameSource::Parsed);
    }

    #[test]
    fn test_parse_falls_back_to_the_heuristic_for_an_unparseable_signature() {
        // Given — prose rather than a declaration.
        let text = ">>> not a declaration <<<".to_string();

        // When
        let signature = CSignature::parse(text);

        // Then — a name is still produced, so the object stays referenceable,
        // even though the heuristic's guess is poor.
        assert_eq!(signature.name_source(), NameSource::Fallback);
        assert_eq!(signature.name(), "<<<");
    }

    #[test]
    fn test_parse_falls_back_when_the_declaration_declares_no_name() {
        // Given — parses as a type but names nothing.
        let text = "int".to_string();

        // When
        let signature = CSignature::parse(text);

        // Then
        assert_eq!(signature.name_source(), NameSource::Fallback);
        assert_eq!(signature.name(), "int");
    }

    #[test]
    fn test_parse_preserves_the_signature_text_verbatim() {
        // Given — spacing must survive untouched, since this is what the
        // renderer prints.
        let text = "PyObject  *  PyUnicode_FromString( const char *str )".to_string();

        // When
        let signature = CSignature::parse(text.clone());

        // Then
        assert_eq!(signature.text(), text);
    }

    #[test]
    fn test_parse_handles_an_object_like_macro() {
        // Given
        let text = "PY_SSIZE_T_MAX".to_string();

        // When
        let signature = CSignature::parse(text);

        // Then
        assert_eq!(signature.name(), "PY_SSIZE_T_MAX");
        assert_eq!(signature.name_source(), NameSource::Parsed);
    }

    #[test]
    fn test_from_str_matches_parse() {
        // Given
        let text = "int (*type_name)(int a)";

        // When
        let from_str = CSignature::from(text);
        let parsed = CSignature::parse(text.to_string());

        // Then
        assert_eq!(from_str, parsed);
    }

    #[test]
    fn test_from_string_matches_parse() {
        // Given
        let text = "Py_ssize_t ob_refcnt".to_string();

        // When
        let from_string = CSignature::from(text.clone());
        let parsed = CSignature::parse(text);

        // Then
        assert_eq!(from_string, parsed);
    }

    #[test]
    fn test_serde_round_trip_preserves_all_fields() {
        // Given
        let signature = CSignature::parse("PyObject *(*unaryfunc)(PyObject *)".to_string());

        // When
        let json = serde_json::to_string(&signature).expect("should serialize");
        let restored: CSignature = serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, signature);
        assert_eq!(restored.name(), "unaryfunc");
        assert_eq!(restored.name_source(), NameSource::Parsed);
    }

    #[test]
    fn test_deserialization_rejects_a_name_that_disagrees_with_the_text() {
        // Given — a hand-edited `.ast` claiming a name the signature does not
        // declare, which would silently misplace the cross-reference target.
        let json = r#"{"text":"int (*Py_tracefunc)(void)","name":"int","name_source":"Parsed"}"#;

        // When
        let result: Result<CSignature, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialization_rejects_a_name_source_that_disagrees_with_the_text() {
        // Given — the name is right but its provenance is not, which would
        // corrupt the parser-coverage diagnostics.
        let json = r#"{"text":"int (*Py_tracefunc)(void)","name":"Py_tracefunc","name_source":"Fallback"}"#;

        // When
        let result: Result<CSignature, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialization_accepts_a_consistent_payload() {
        // Given
        let json =
            r#"{"text":"int (*Py_tracefunc)(void)","name":"Py_tracefunc","name_source":"Parsed"}"#;

        // When
        let signature: CSignature = serde_json::from_str(json).expect("should deserialize");

        // Then
        assert_eq!(signature.name(), "Py_tracefunc");
    }

    // ── The fallback heuristic itself ────────────────────────────────────

    #[test]
    fn test_extract_c_object_name_simple_call() {
        // Given
        let signature = "foo(bar)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_c_object_name_no_parens() {
        // Given
        let signature = "foo";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_c_object_name_empty_string() {
        // Given
        let signature = "";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "");
    }

    #[test]
    fn test_extract_c_object_name_return_type_prefix() {
        // Given
        let signature = "int foo(int bar)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_c_object_name_strips_pointer_sigil_glued_to_name() {
        // Given
        let signature = "char *foo(void)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_c_object_name_strips_double_pointer_sigil() {
        // Given
        let signature = "int **foo(void)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_c_object_name_unaffected_when_sigil_glued_to_type() {
        // Given
        let signature = "PyObject* PyUnicode_FromStringAndSize(const char *str, Py_ssize_t size)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "PyUnicode_FromStringAndSize");
    }

    #[test]
    fn test_extract_c_object_name_matches_cpython_unicode_fromstring() {
        // Given — the real-world signature that surfaced the broken-link bug
        let signature = "PyObject *PyUnicode_FromString(const char *str)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "PyUnicode_FromString");
    }

    #[test]
    fn test_extract_c_object_name_bare_macro_name() {
        // Given — object-like macros have no parens and no return type
        let signature = "PY_SSIZE_T_MAX";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "PY_SSIZE_T_MAX");
    }

    #[test]
    fn test_extract_c_object_name_function_like_macro() {
        // Given — function-like macros have no return type to strip
        let signature = "MAX(a, b)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "MAX");
    }

    #[test]
    fn test_extract_c_object_name_misextracts_function_pointer_typedef() {
        // Given — the limitation that motivated `rinx_cdecl`: the
        // name lives inside the first parenthesized group, which this
        // heuristic never looks past. Pinned here because the heuristic is
        // still reachable as the fallback — `CSignature::parse` no longer
        // produces this answer (see
        // `test_parse_uses_the_declaration_parser_for_a_well_formed_signature`).
        let signature = "int (*type_name)(int arg1, int arg2)";

        // When
        let name = extract_c_object_name(signature);

        // Then — mis-extracts "int" instead of "type_name".
        assert_eq!(name, "int");
    }
}
