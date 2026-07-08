use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
