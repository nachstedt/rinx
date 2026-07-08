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

#[cfg(test)]
mod tests {
    use super::*;

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
}
