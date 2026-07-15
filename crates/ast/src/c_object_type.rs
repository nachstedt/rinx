use serde::{Deserialize, Serialize};

/// Object types defined by the `c` domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CObjectType {
    Function,
    Macro,
}

impl CObjectType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Macro => "macro",
        }
    }
}

impl std::str::FromStr for CObjectType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "function" => Ok(Self::Function),
            "macro" => Ok(Self::Macro),
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

    #[test]
    fn test_c_object_type_from_str_accepts_macro() {
        // Given / When / Then
        assert_eq!("macro".parse::<CObjectType>(), Ok(CObjectType::Macro));
    }

    #[test]
    fn test_c_object_type_as_str_returns_macro() {
        // Given / When / Then
        assert_eq!(CObjectType::Macro.as_str(), "macro");
    }

    #[test]
    fn test_c_object_type_ord_orders_variants_by_declaration_order() {
        // Given
        let function = CObjectType::Function;
        let macro_ = CObjectType::Macro;

        // When / Then
        assert!(function < macro_);
    }
}
