use serde::{Deserialize, Serialize};

/// Object types defined by the `std` domain (constructs with no language
/// affiliation). Only `Cmdoption` (`.. option::`/`.. cmdoption::`) exists
/// today; more (e.g. `envvar`) can be added as further enum variants with no
/// other restructuring implied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StdObjectType {
    Cmdoption,
}

impl StdObjectType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Cmdoption => "cmdoption",
        }
    }
}

impl std::str::FromStr for StdObjectType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cmdoption" => Ok(Self::Cmdoption),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_std_object_type_from_str_accepts_cmdoption() {
        // Given / When / Then
        assert_eq!(
            "cmdoption".parse::<StdObjectType>(),
            Ok(StdObjectType::Cmdoption)
        );
    }

    #[test]
    fn test_std_object_type_from_str_rejects_unknown() {
        // Given
        let input = "envvar";

        // When
        let result = input.parse::<StdObjectType>();

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_std_object_type_as_str_returns_cmdoption() {
        // Given
        let object_type = StdObjectType::Cmdoption;

        // When
        let s = object_type.as_str();

        // Then
        assert_eq!(s, "cmdoption");
    }
}
