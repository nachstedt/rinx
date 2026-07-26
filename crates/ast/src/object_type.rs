use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
///
/// `Ord`/`PartialOrd` are derived (in declaration order) so `ObjectType` can
/// be used as a `BTreeMap` key (the second level of
/// `ProjectIndex::domain_objects`). `Serialize`/`Deserialize` are hand-written
/// rather than derived: `ObjectType` has non-unit variants, so a derived impl
/// would serialize as `{"py":"class"}`, and `serde_json` rejects non-string
/// map keys — the hand-written impl renders/parses the flat `"py:class"`
/// form instead, which both stays a valid map key and matches the
/// `domain:objtype` shape used elsewhere (e.g. [`crate::domain_object_body::build_domain_object_key`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectType {
    Py(PyObjectType),
    C(CObjectType),
}

impl Serialize for ObjectType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}:{}", self.domain().as_str(), self.as_str()))
    }
}

impl<'de> Deserialize<'de> for ObjectType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let (domain_str, objtype_str) = s
            .split_once(':')
            .ok_or_else(|| D::Error::custom(format!("invalid object type key: {s:?}")))?;
        let domain: Domain = domain_str
            .parse()
            .map_err(|()| D::Error::custom(format!("unknown domain: {domain_str:?}")))?;
        Self::from_directive_name(domain, objtype_str)
            .ok_or_else(|| D::Error::custom(format!("unknown object type: {s:?}")))
    }
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

    /// The `"domain:objtype"` form (e.g. `"py:class"`) used in diagnostics
    /// where the domain needs to be visible alongside the object type — bare
    /// `as_str()` alone is ambiguous between domains that happen to share an
    /// object-type name (though today only `function` does: `py:function`
    /// vs `c:function`). Distinct from
    /// [`crate::domain_object_body::build_domain_object_key`]'s
    /// `"domain:objtype:name"` anchor-key form, which additionally includes
    /// a specific object's name.
    #[must_use]
    pub fn domain_qualified_str(&self) -> String {
        format!("{}:{}", self.domain().as_str(), self.as_str())
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
            (Domain::Py, "attr") => Some(Self::Py(PyObjectType::Attribute)),
            (Domain::Py, "exc") => Some(Self::Py(PyObjectType::Exception)),
            (Domain::C, "func") => Some(Self::C(CObjectType::Function)),
            (Domain::C, "macro") => Some(Self::C(CObjectType::Macro)),
            (Domain::C, "data" | "var") => Some(Self::C(CObjectType::Data)),
            _ => None,
        }
    }

    /// The object types a reference asking for `self` is also willing to
    /// accept, most-preferred (itself) first — models real Sphinx's small,
    /// static `py` domain aliasing between `class`/`exception`/`obj`
    /// (`CPython`'s own docs freely mix `.. class::` definitions with `:exc:`
    /// references and vice versa, and Sphinx never warns). Deliberately an
    /// exhaustive match, not a wildcard fallback arm: adding a new object
    /// type later forces a decision about whether it aliases anything.
    ///
    /// `C(Macro)`/`C(Data)` alias for a different reason than the `py` pair
    /// above: real Sphinx's C domain `resolve_xref`
    /// (`sphinx/domains/c/__init__.py`, `_resolve_xref_inner`) looks up a
    /// declaration by name only and never checks the role's requested type
    /// against the declaration's actual object type (there's a literal
    /// `# TODO: check role type vs. object type` at the point where it
    /// would) — so a `:c:data:` role resolving against a `.. c:macro::`
    /// definition, as `CPython`'s `c-api/module.rst` does for `Py_mod_exec`,
    /// is a known real-Sphinx looseness rather than a deliberate aliasing
    /// rule. This models only that one confirmed collision rather than
    /// dropping type-checking for the whole domain (which would also let
    /// `:c:func:` blindly match unrelated `data`/`macro` names).
    #[must_use]
    pub const fn role_alias_candidates(self) -> &'static [Self] {
        match self {
            Self::Py(PyObjectType::Class) => &[
                Self::Py(PyObjectType::Class),
                Self::Py(PyObjectType::Exception),
            ],
            Self::Py(PyObjectType::Exception) => &[
                Self::Py(PyObjectType::Exception),
                Self::Py(PyObjectType::Class),
            ],
            Self::Py(PyObjectType::Function) => &[Self::Py(PyObjectType::Function)],
            Self::Py(PyObjectType::Module) => &[Self::Py(PyObjectType::Module)],
            Self::Py(PyObjectType::Data) => &[Self::Py(PyObjectType::Data)],
            Self::Py(PyObjectType::Method) => &[Self::Py(PyObjectType::Method)],
            Self::Py(PyObjectType::Attribute) => &[Self::Py(PyObjectType::Attribute)],
            Self::C(CObjectType::Function) => &[Self::C(CObjectType::Function)],
            Self::C(CObjectType::Macro) => {
                &[Self::C(CObjectType::Macro), Self::C(CObjectType::Data)]
            }
            Self::C(CObjectType::Data) => {
                &[Self::C(CObjectType::Data), Self::C(CObjectType::Macro)]
            }
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
    fn test_object_type_domain_qualified_str_prefixes_domain() {
        // Given / When / Then
        assert_eq!(
            ObjectType::Py(PyObjectType::Class).domain_qualified_str(),
            "py:class"
        );
        assert_eq!(
            ObjectType::Py(PyObjectType::Exception).domain_qualified_str(),
            "py:exception"
        );
        assert_eq!(
            ObjectType::C(CObjectType::Function).domain_qualified_str(),
            "c:function"
        );
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
        assert_eq!(
            ObjectType::from_directive_name(Domain::C, "macro"),
            Some(ObjectType::C(CObjectType::Macro))
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
    fn test_object_type_from_role_name_resolves_py_data_and_const_to_same_type() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::Py, "data"),
            Some(ObjectType::Py(PyObjectType::Data))
        );
        assert_eq!(
            ObjectType::from_role_name(Domain::Py, "const"),
            Some(ObjectType::Py(PyObjectType::Data))
        );
        assert_eq!(ObjectType::from_role_name(Domain::C, "const"), None);
    }

    #[test]
    fn test_object_type_from_role_name_resolves_data_and_var_to_same_type_only_for_c() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::C, "data"),
            Some(ObjectType::C(CObjectType::Data))
        );
        assert_eq!(
            ObjectType::from_role_name(Domain::C, "var"),
            Some(ObjectType::C(CObjectType::Data))
        );
        assert_eq!(ObjectType::from_role_name(Domain::Py, "var"), None);
    }

    #[test]
    fn test_object_type_from_role_name_resolves_attr_only_for_py() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::Py, "attr"),
            Some(ObjectType::Py(PyObjectType::Attribute))
        );
        assert_eq!(ObjectType::from_role_name(Domain::C, "attr"), None);
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
    fn test_object_type_from_directive_name_resolves_exception_for_py() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_directive_name(Domain::Py, "exception"),
            Some(ObjectType::Py(PyObjectType::Exception))
        );
        assert_eq!(
            ObjectType::from_directive_name(Domain::C, "exception"),
            None
        );
    }

    #[test]
    fn test_object_type_from_role_name_resolves_macro_only_for_c() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::C, "macro"),
            Some(ObjectType::C(CObjectType::Macro))
        );
        assert_eq!(ObjectType::from_role_name(Domain::Py, "macro"), None);
    }

    #[test]
    fn test_object_type_from_role_name_resolves_exc_only_for_py() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::Py, "exc"),
            Some(ObjectType::Py(PyObjectType::Exception))
        );
        assert_eq!(ObjectType::from_role_name(Domain::C, "exc"), None);
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

    #[test]
    fn test_object_type_serializes_as_flat_domain_objtype_string() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Class);

        // When
        let json = serde_json::to_string(&object_type).expect("Failed to serialize");

        // Then — not the derived `{"py":"class"}` shape, which serde_json
        // would reject as a map key.
        assert_eq!(json, "\"py:class\"");
    }

    #[test]
    fn test_object_type_deserialize_rejects_malformed_string() {
        // Given
        let json = "\"not-a-valid-key\"";

        // When
        let result: Result<ObjectType, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_object_type_can_be_used_as_btreemap_key() {
        // Given
        let mut map = std::collections::BTreeMap::new();

        // When
        map.insert(ObjectType::Py(PyObjectType::Class), "fault.rst");
        map.insert(ObjectType::Py(PyObjectType::Exception), "other.rst");

        // Then
        assert_eq!(
            map.get(&ObjectType::Py(PyObjectType::Class)),
            Some(&"fault.rst")
        );
        assert_eq!(
            map.get(&ObjectType::Py(PyObjectType::Exception)),
            Some(&"other.rst")
        );
    }

    #[test]
    fn test_role_alias_candidates_aliases_class_and_exception_both_directions() {
        // Given / When / Then
        assert_eq!(
            ObjectType::Py(PyObjectType::Class).role_alias_candidates(),
            &[
                ObjectType::Py(PyObjectType::Class),
                ObjectType::Py(PyObjectType::Exception)
            ]
        );
        assert_eq!(
            ObjectType::Py(PyObjectType::Exception).role_alias_candidates(),
            &[
                ObjectType::Py(PyObjectType::Exception),
                ObjectType::Py(PyObjectType::Class)
            ]
        );
    }

    #[test]
    fn test_role_alias_candidates_aliases_c_macro_and_data_both_directions() {
        // Given / When / Then
        assert_eq!(
            ObjectType::C(CObjectType::Macro).role_alias_candidates(),
            &[
                ObjectType::C(CObjectType::Macro),
                ObjectType::C(CObjectType::Data)
            ]
        );
        assert_eq!(
            ObjectType::C(CObjectType::Data).role_alias_candidates(),
            &[
                ObjectType::C(CObjectType::Data),
                ObjectType::C(CObjectType::Macro)
            ]
        );
    }

    #[test]
    fn test_role_alias_candidates_is_self_only_for_non_aliased_types() {
        // Given / When / Then
        assert_eq!(
            ObjectType::Py(PyObjectType::Function).role_alias_candidates(),
            &[ObjectType::Py(PyObjectType::Function)]
        );
        assert_eq!(
            ObjectType::Py(PyObjectType::Module).role_alias_candidates(),
            &[ObjectType::Py(PyObjectType::Module)]
        );
        assert_eq!(
            ObjectType::Py(PyObjectType::Data).role_alias_candidates(),
            &[ObjectType::Py(PyObjectType::Data)]
        );
        assert_eq!(
            ObjectType::Py(PyObjectType::Method).role_alias_candidates(),
            &[ObjectType::Py(PyObjectType::Method)]
        );
        assert_eq!(
            ObjectType::Py(PyObjectType::Attribute).role_alias_candidates(),
            &[ObjectType::Py(PyObjectType::Attribute)]
        );
        assert_eq!(
            ObjectType::C(CObjectType::Function).role_alias_candidates(),
            &[ObjectType::C(CObjectType::Function)]
        );
    }
}
