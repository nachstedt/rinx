use serde::{Deserialize, Serialize};

/// Object types defined by the `py` domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PyObjectType {
    Function,
    Module,
    Data,
    Method,
    Class,
    Attribute,
    Exception,
}

impl PyObjectType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Module => "module",
            Self::Data => "data",
            Self::Method => "method",
            Self::Class => "class",
            Self::Attribute => "attribute",
            Self::Exception => "exception",
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
            "method" => Ok(Self::Method),
            "class" => Ok(Self::Class),
            "attribute" => Ok(Self::Attribute),
            "exception" => Ok(Self::Exception),
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
        let input = "struct";

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

    #[test]
    fn test_py_object_type_from_str_accepts_method() {
        // Given / When / Then
        assert_eq!("method".parse::<PyObjectType>(), Ok(PyObjectType::Method));
    }

    #[test]
    fn test_py_object_type_as_str_returns_method() {
        // Given
        let object_type = PyObjectType::Method;

        // When
        let s = object_type.as_str();

        // Then
        assert_eq!(s, "method");
    }

    #[test]
    fn test_py_object_type_from_str_accepts_class() {
        // Given / When / Then
        assert_eq!("class".parse::<PyObjectType>(), Ok(PyObjectType::Class));
    }

    #[test]
    fn test_py_object_type_as_str_returns_class() {
        // Given
        let object_type = PyObjectType::Class;

        // When
        let s = object_type.as_str();

        // Then
        assert_eq!(s, "class");
    }

    #[test]
    fn test_py_object_type_from_str_accepts_attribute() {
        // Given / When / Then
        assert_eq!(
            "attribute".parse::<PyObjectType>(),
            Ok(PyObjectType::Attribute)
        );
    }

    #[test]
    fn test_py_object_type_as_str_returns_attribute() {
        // Given
        let object_type = PyObjectType::Attribute;

        // When
        let s = object_type.as_str();

        // Then
        assert_eq!(s, "attribute");
    }

    #[test]
    fn test_py_object_type_from_str_accepts_exception() {
        // Given / When / Then
        assert_eq!(
            "exception".parse::<PyObjectType>(),
            Ok(PyObjectType::Exception)
        );
    }

    #[test]
    fn test_py_object_type_as_str_returns_exception() {
        // Given
        let object_type = PyObjectType::Exception;

        // When
        let s = object_type.as_str();

        // Then
        assert_eq!(s, "exception");
    }
}
