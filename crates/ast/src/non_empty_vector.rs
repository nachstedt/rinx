use serde::{Deserialize, Serialize};

/// A vector guaranteed to hold at least one element.
///
/// [`Self::new`] takes the first element as its own parameter, so an empty
/// value cannot be constructed and no caller ever has to handle a failure
/// case — [`Self::first`] returns a `&T`, not an `Option`. Deserialization is
/// the only entry point that can encounter an invalid value (a stale or
/// hand-edited `.ast` file), and it re-validates, the same "parse, don't
/// validate" shape as [`crate::HashedContent`].
///
/// Serializes transparently as a plain array.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<T>")]
pub struct NonEmptyVector<T>(Vec<T>);

impl<T> NonEmptyVector<T> {
    /// Creates a vector holding `first` followed by `rest`.
    #[must_use]
    pub fn new(first: T, rest: Vec<T>) -> Self {
        let mut items = Vec::with_capacity(1 + rest.len());
        items.push(first);
        items.extend(rest);
        Self(items)
    }

    /// Creates a vector holding exactly one element.
    #[must_use]
    pub fn single(first: T) -> Self {
        Self(vec![first])
    }

    /// The first element. Infallible, unlike [`slice::first`].
    #[must_use]
    pub fn first(&self) -> &T {
        &self.0[0]
    }

    /// All elements, in order.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// Applies `f` to every element, carrying the non-empty guarantee through
    /// to the result rather than degrading it to a plain [`Vec`].
    #[must_use]
    pub fn map<U>(&self, f: impl FnMut(&T) -> U) -> NonEmptyVector<U> {
        NonEmptyVector(self.0.iter().map(f).collect())
    }
}

impl<T> TryFrom<Vec<T>> for NonEmptyVector<T> {
    type Error = String;

    fn try_from(items: Vec<T>) -> Result<Self, Self::Error> {
        if items.is_empty() {
            return Err("expected at least one element, got an empty vector".to_string());
        }
        Ok(Self(items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_puts_first_element_before_the_rest() {
        // Given
        let first = "AF_UNIX".to_string();
        let rest = vec!["AF_INET".to_string(), "AF_INET6".to_string()];

        // When
        let vector = NonEmptyVector::new(first, rest);

        // Then
        assert_eq!(vector.as_slice(), ["AF_UNIX", "AF_INET", "AF_INET6"]);
    }

    #[test]
    fn test_new_with_no_rest_holds_only_the_first_element() {
        // Given
        let first = "DEFAULT_TIMEOUT".to_string();

        // When
        let vector = NonEmptyVector::new(first, Vec::new());

        // Then
        assert_eq!(vector.as_slice(), ["DEFAULT_TIMEOUT"]);
    }

    #[test]
    fn test_single_holds_exactly_one_element() {
        // Given
        let only = "greet(name)".to_string();

        // When
        let vector = NonEmptyVector::single(only);

        // Then
        assert_eq!(vector.as_slice(), ["greet(name)"]);
        assert_eq!(vector.first(), "greet(name)");
    }

    #[test]
    fn test_first_returns_the_leading_element() {
        // Given
        let vector = NonEmptyVector::new(1, vec![2, 3]);

        // When
        let first = vector.first();

        // Then
        assert_eq!(*first, 1);
    }

    #[test]
    fn test_map_transforms_every_element_and_preserves_order() {
        // Given
        let vector = NonEmptyVector::new(1, vec![2, 3]);

        // When
        let doubled = vector.map(|n| n * 2);

        // Then
        assert_eq!(doubled.as_slice(), [2, 4, 6]);
    }

    #[test]
    fn test_map_can_change_the_element_type() {
        // Given
        let vector = NonEmptyVector::new("Greeter(Base)".to_string(), vec!["Other".to_string()]);

        // When
        let lengths = vector.map(String::len);

        // Then
        assert_eq!(lengths.as_slice(), [13, 5]);
    }

    #[test]
    fn test_try_from_accepts_a_populated_vector() {
        // Given
        let items = vec!["a".to_string(), "b".to_string()];

        // When
        let result = NonEmptyVector::try_from(items);

        // Then
        assert_eq!(result.expect("populated vector").as_slice(), ["a", "b"]);
    }

    #[test]
    fn test_try_from_rejects_an_empty_vector() {
        // Given
        let items: Vec<String> = Vec::new();

        // When
        let result = NonEmptyVector::try_from(items);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_serializes_transparently_as_a_plain_array() {
        // Given
        let vector = NonEmptyVector::new("AF_UNIX".to_string(), vec!["AF_INET".to_string()]);

        // When
        let json = serde_json::to_string(&vector).expect("serialization succeeds");

        // Then
        assert_eq!(json, r#"["AF_UNIX","AF_INET"]"#);
    }

    #[test]
    fn test_deserialization_accepts_a_populated_array() {
        // Given
        let json = r#"["AF_UNIX","AF_INET"]"#;

        // When
        let vector: NonEmptyVector<String> =
            serde_json::from_str(json).expect("deserialization succeeds");

        // Then
        assert_eq!(vector.as_slice(), ["AF_UNIX", "AF_INET"]);
    }

    #[test]
    fn test_deserialization_rejects_an_empty_array() {
        // Given — the invariant can only be violated through a stale or
        // hand-edited `.ast` file, so deserialization has to re-check it.
        let json = "[]";

        // When
        let result: Result<NonEmptyVector<String>, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_round_trips_through_serde() {
        // Given
        let vector = NonEmptyVector::new(1, vec![2, 3]);

        // When
        let json = serde_json::to_string(&vector).expect("serialization succeeds");
        let restored: NonEmptyVector<i32> =
            serde_json::from_str(&json).expect("deserialization succeeds");

        // Then
        assert_eq!(restored, vector);
    }
}
