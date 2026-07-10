use serde::{Deserialize, Serialize};

use crate::c_object_type::CObjectType;
use crate::domain::Domain;
use crate::py_object_type::PyObjectType;

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
    /// their directive counterparts, matching real Sphinx. Note `"data"` and
    /// `"const"` both resolve to [`PyObjectType::Data`]: real Sphinx has no
    /// separate `py:const` directive, `:const:` is just an alternate role
    /// spelling for referencing a `py:data` object as a constant.
    #[must_use]
    pub fn from_role_name(domain: Domain, role: &str) -> Option<Self> {
        match (domain, role) {
            (Domain::Py, "func") => Some(Self::Py(PyObjectType::Function)),
            (Domain::Py, "mod") => Some(Self::Py(PyObjectType::Module)),
            (Domain::Py, "data" | "const") => Some(Self::Py(PyObjectType::Data)),
            (Domain::Py, "meth") => Some(Self::Py(PyObjectType::Method)),
            (Domain::Py, "class") => Some(Self::Py(PyObjectType::Class)),
            (Domain::C, "func") => Some(Self::C(CObjectType::Function)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let name = "struct";

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
    fn test_object_type_from_role_name_resolves_mod_only_for_py() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::Py, "mod"),
            Some(ObjectType::Py(PyObjectType::Module))
        );
        assert_eq!(ObjectType::from_role_name(Domain::C, "mod"), None);
    }

    #[test]
    fn test_object_type_from_role_name_resolves_data_and_const_to_same_type_only_for_py() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::Py, "data"),
            Some(ObjectType::Py(PyObjectType::Data))
        );
        assert_eq!(
            ObjectType::from_role_name(Domain::Py, "const"),
            Some(ObjectType::Py(PyObjectType::Data))
        );
        assert_eq!(ObjectType::from_role_name(Domain::C, "data"), None);
        assert_eq!(ObjectType::from_role_name(Domain::C, "const"), None);
    }

    #[test]
    fn test_object_type_from_role_name_rejects_unknown_role() {
        // Given
        let domain = Domain::Py;
        let role = "struct";

        // When
        let result = ObjectType::from_role_name(domain, role);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_object_type_from_role_name_resolves_meth_only_for_py() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::Py, "meth"),
            Some(ObjectType::Py(PyObjectType::Method))
        );
        assert_eq!(ObjectType::from_role_name(Domain::C, "meth"), None);
    }

    #[test]
    fn test_object_type_from_role_name_resolves_class_only_for_py() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::Py, "class"),
            Some(ObjectType::Py(PyObjectType::Class))
        );
        assert_eq!(ObjectType::from_role_name(Domain::C, "class"), None);
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
}
