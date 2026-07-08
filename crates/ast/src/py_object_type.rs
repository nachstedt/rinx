use serde::{Deserialize, Serialize};

/// Object types defined by the `py` domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PyObjectType {
    Function,
    Module,
    Data,
}

impl PyObjectType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Module => "module",
            Self::Data => "data",
        }
    }
}

impl std::str::FromStr for PyObjectType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "function" => Ok(Self::Function),
            "module" => Ok(Self::Module),
            "data" => Ok(Self::Data),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_py_object_type_from_str_accepts_module() {
        // Given / When / Then
        assert_eq!("module".parse::<PyObjectType>(), Ok(PyObjectType::Module));
    }

    #[test]
    fn test_py_object_type_from_str_accepts_data() {
        // Given / When / Then
        assert_eq!("data".parse::<PyObjectType>(), Ok(PyObjectType::Data));
    }
}
