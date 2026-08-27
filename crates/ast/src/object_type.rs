use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::c_object_type::CObjectType;
use crate::domain::Domain;
use crate::py_object_type::PyObjectType;
use crate::std_object_type::StdObjectType;

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
    Std(StdObjectType),
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
            Self::Std(_) => Domain::Std,
        }
    }

    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Py(t) => t.as_str(),
            Self::C(t) => t.as_str(),
            Self::Std(t) => t.as_str(),
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
            Domain::Std => name.parse::<StdObjectType>().ok().map(Self::Std),
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
            (Domain::C, "member" | "data" | "var") => Some(Self::C(CObjectType::Member)),
            (Domain::C, "struct") => Some(Self::C(CObjectType::Struct)),
            (Domain::C, "union") => Some(Self::C(CObjectType::Union)),
            (Domain::C, "type") => Some(Self::C(CObjectType::Type)),
            (Domain::Std, "option") => Some(Self::Std(StdObjectType::Cmdoption)),
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
    /// The `c` pairs alias for a different reason than the `py` pair above:
    /// real Sphinx's C domain `resolve_xref`
    /// (`sphinx/domains/c/__init__.py`, `_resolve_xref_inner`) looks up a
    /// declaration by name only and never checks the role's requested type
    /// against the declaration's actual object type (there's a literal
    /// `# TODO: check role type vs. object type` at the point where it
    /// would). Every `c`-domain role/object-type collision is therefore a
    /// known real-Sphinx looseness rather than a deliberate aliasing rule,
    /// and the whole domain would alias if we mirrored it. We deliberately
    /// don't: the type check stays on every lookup path, and only the
    /// collisions the `CPython` corpus actually confirms are modelled, so an
    /// author's mistake is still reported instead of resolved to something
    /// plausible. Two pairs are confirmed:
    ///
    /// - `C(Macro)`/`C(Member)` — `c-api/module.rst` references the
    ///   macro-defined slot constant `Py_mod_exec` via `:c:data:`.
    /// - `C(Function)`/`C(Macro)` — `c-api/gcsupport.rst` references the
    ///   function-like macro `Py_VISIT` via `:c:func:`, and
    ///   `c-api/structures.rst` references the function `Py_REFCNT` (defined
    ///   `.. c:function::` in `c-api/refcounting.rst`) via `:c:macro:`.
    ///   Neither file is in `CPython`'s `Doc/tools/.nitignore`, so both
    ///   resolve under real Sphinx's nit-picky mode.
    ///
    /// The relation is symmetric but deliberately **not** transitive:
    /// `Function` and `Member` do not alias each other, because no evidence
    /// says they collide. That is a decision, not an oversight — each edge
    /// earns its place separately. `Struct`/`Union`/`Type` alias nothing.
    ///
    /// Order within a list is load-bearing: the resolver walks it and takes
    /// the first hit, with no ambiguity detection at this level. Self comes
    /// first so an exact type always beats an alias; the rest follow
    /// [`CObjectType`]'s declaration order, so the tie-break is stable and
    /// explainable rather than incidental.
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
            Self::C(CObjectType::Function) => {
                &[Self::C(CObjectType::Function), Self::C(CObjectType::Macro)]
            }
            Self::C(CObjectType::Macro) => &[
                Self::C(CObjectType::Macro),
                Self::C(CObjectType::Function),
                Self::C(CObjectType::Member),
            ],
            Self::C(CObjectType::Member) => {
                &[Self::C(CObjectType::Member), Self::C(CObjectType::Macro)]
            }
            Self::C(CObjectType::Struct) => &[Self::C(CObjectType::Struct)],
            Self::C(CObjectType::Union) => &[Self::C(CObjectType::Union)],
            Self::C(CObjectType::Type) => &[Self::C(CObjectType::Type)],
            Self::Std(StdObjectType::Cmdoption) => &[Self::Std(StdObjectType::Cmdoption)],
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
        let std_type = ObjectType::Std(StdObjectType::Cmdoption);

        // When / Then
        assert_eq!(py_type.domain(), Domain::Py);
        assert_eq!(c_type.domain(), Domain::C);
        assert_eq!(std_type.domain(), Domain::Std);
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
        assert_eq!(
            ObjectType::from_directive_name(Domain::C, "type"),
            Some(ObjectType::C(CObjectType::Type))
        );
    }

    #[test]
    fn test_object_type_from_directive_name_resolves_cmdoption_for_std() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_directive_name(Domain::Std, "cmdoption"),
            Some(ObjectType::Std(StdObjectType::Cmdoption))
        );
        assert_eq!(
            ObjectType::from_directive_name(Domain::Py, "cmdoption"),
            None
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
            Some(ObjectType::C(CObjectType::Member))
        );
        assert_eq!(
            ObjectType::from_role_name(Domain::C, "var"),
            Some(ObjectType::C(CObjectType::Member))
        );
        assert_eq!(ObjectType::from_role_name(Domain::Py, "var"), None);
    }

    #[test]
    fn test_object_type_from_role_name_resolves_member_only_for_c() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::C, "member"),
            Some(ObjectType::C(CObjectType::Member))
        );
        assert_eq!(ObjectType::from_role_name(Domain::Py, "member"), None);
    }

    #[test]
    fn test_object_type_from_role_name_resolves_struct_and_union_only_for_c() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::C, "struct"),
            Some(ObjectType::C(CObjectType::Struct))
        );
        assert_eq!(
            ObjectType::from_role_name(Domain::C, "union"),
            Some(ObjectType::C(CObjectType::Union))
        );
        assert_eq!(ObjectType::from_role_name(Domain::Py, "struct"), None);
        assert_eq!(ObjectType::from_role_name(Domain::Py, "union"), None);
    }

    #[test]
    fn test_object_type_from_role_name_resolves_type_only_for_c() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::C, "type"),
            Some(ObjectType::C(CObjectType::Type))
        );
        assert_eq!(ObjectType::from_role_name(Domain::Py, "type"), None);
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
    fn test_object_type_from_role_name_resolves_option_only_for_std() {
        // Given / When / Then
        assert_eq!(
            ObjectType::from_role_name(Domain::Std, "option"),
            Some(ObjectType::Std(StdObjectType::Cmdoption))
        );
        assert_eq!(ObjectType::from_role_name(Domain::Py, "option"), None);
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
    fn test_role_alias_candidates_aliases_c_macro_and_member_both_directions() {
        // Given / When / Then
        assert_eq!(
            ObjectType::C(CObjectType::Macro).role_alias_candidates(),
            &[
                ObjectType::C(CObjectType::Macro),
                ObjectType::C(CObjectType::Function),
                ObjectType::C(CObjectType::Member)
            ]
        );
        assert_eq!(
            ObjectType::C(CObjectType::Member).role_alias_candidates(),
            &[
                ObjectType::C(CObjectType::Member),
                ObjectType::C(CObjectType::Macro)
            ]
        );
    }

    #[test]
    fn test_role_alias_candidates_aliases_c_function_and_macro_both_directions() {
        // Given / When / Then
        assert_eq!(
            ObjectType::C(CObjectType::Function).role_alias_candidates(),
            &[
                ObjectType::C(CObjectType::Function),
                ObjectType::C(CObjectType::Macro)
            ]
        );
        assert!(
            ObjectType::C(CObjectType::Macro)
                .role_alias_candidates()
                .contains(&ObjectType::C(CObjectType::Function))
        );
    }

    #[test]
    fn test_role_alias_candidates_does_not_alias_c_function_and_member() {
        // Given — `Function` aliases `Macro` and `Macro` aliases `Member`, but
        // the relation is deliberately not transitive: no corpus evidence says
        // a `:c:func:` role and a `.. c:member::` definition ever collide.
        let function = ObjectType::C(CObjectType::Function);
        let member = ObjectType::C(CObjectType::Member);

        // When / Then
        assert!(!function.role_alias_candidates().contains(&member));
        assert!(!member.role_alias_candidates().contains(&function));
    }

    #[test]
    fn test_role_alias_candidates_is_self_only_for_struct_and_union() {
        // Given / When / Then
        assert_eq!(
            ObjectType::C(CObjectType::Struct).role_alias_candidates(),
            &[ObjectType::C(CObjectType::Struct)]
        );
        assert_eq!(
            ObjectType::C(CObjectType::Union).role_alias_candidates(),
            &[ObjectType::C(CObjectType::Union)]
        );
    }

    #[test]
    fn test_role_alias_candidates_is_self_only_for_type() {
        // Given / When / Then
        assert_eq!(
            ObjectType::C(CObjectType::Type).role_alias_candidates(),
            &[ObjectType::C(CObjectType::Type)]
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
            ObjectType::Std(StdObjectType::Cmdoption).role_alias_candidates(),
            &[ObjectType::Std(StdObjectType::Cmdoption)]
        );
    }
    /// Every [`ObjectType`] variant, so the property tests below can assert
    /// facts about the whole alias table rather than one row at a time.
    /// Kept here rather than exposed as `ObjectType::ALL`: nothing in
    /// production iterates object types, so a public constant would exist
    /// only to serve its own tests.
    const ALL_OBJECT_TYPES: &[ObjectType] = &[
        ObjectType::Py(PyObjectType::Function),
        ObjectType::Py(PyObjectType::Module),
        ObjectType::Py(PyObjectType::Data),
        ObjectType::Py(PyObjectType::Method),
        ObjectType::Py(PyObjectType::Class),
        ObjectType::Py(PyObjectType::Attribute),
        ObjectType::Py(PyObjectType::Exception),
        ObjectType::C(CObjectType::Function),
        ObjectType::C(CObjectType::Macro),
        ObjectType::C(CObjectType::Member),
        ObjectType::C(CObjectType::Struct),
        ObjectType::C(CObjectType::Union),
        ObjectType::C(CObjectType::Type),
        ObjectType::Std(StdObjectType::Cmdoption),
    ];

    /// Compile-time guard for [`ALL_OBJECT_TYPES`]: the match below is
    /// exhaustive, so adding a new [`ObjectType`] variant stops this file
    /// compiling until whoever added it also extends the list above — which
    /// is what keeps the property tests covering the whole table.
    fn assert_listed_in_all_object_types(object_type: ObjectType) {
        match object_type {
            ObjectType::Py(
                PyObjectType::Function
                | PyObjectType::Module
                | PyObjectType::Data
                | PyObjectType::Method
                | PyObjectType::Class
                | PyObjectType::Attribute
                | PyObjectType::Exception,
            )
            | ObjectType::C(
                CObjectType::Function
                | CObjectType::Macro
                | CObjectType::Member
                | CObjectType::Struct
                | CObjectType::Union
                | CObjectType::Type,
            )
            | ObjectType::Std(StdObjectType::Cmdoption) => {}
        }
        assert!(
            ALL_OBJECT_TYPES.contains(&object_type),
            "{object_type:?} is missing from ALL_OBJECT_TYPES"
        );
    }

    #[test]
    fn test_all_object_types_lists_every_variant() {
        // Given / When / Then
        for object_type in ALL_OBJECT_TYPES {
            assert_listed_in_all_object_types(*object_type);
        }
    }

    #[test]
    fn test_role_alias_candidates_lists_itself_first() {
        // Given — the resolver walks the candidate list and takes the first
        // hit, so self coming first is what makes an exact object-type match
        // always beat an aliased one.
        for object_type in ALL_OBJECT_TYPES {
            // When
            let candidates = object_type.role_alias_candidates();

            // Then
            assert_eq!(
                candidates.first(),
                Some(object_type),
                "{object_type:?} does not list itself first"
            );
        }
    }

    #[test]
    fn test_role_alias_candidates_is_symmetric() {
        // Given — aliasing is a mutual willingness to match: if a role asking
        // for `a` accepts a definition of type `b`, the reverse must hold too.
        // Pins that a future one-sided edit to the table is caught.
        for a in ALL_OBJECT_TYPES {
            for b in ALL_OBJECT_TYPES {
                // When
                let a_accepts_b = a.role_alias_candidates().contains(b);
                let b_accepts_a = b.role_alias_candidates().contains(a);

                // Then
                assert_eq!(
                    a_accepts_b, b_accepts_a,
                    "aliasing between {a:?} and {b:?} is one-sided"
                );
            }
        }
    }
}
