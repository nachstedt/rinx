use serde::{Deserialize, Serialize};

/// Object types defined by the `c` domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CObjectType {
    Function,
    Macro,
    /// The real Sphinx `ObjType` name is `"member"`; `data`/`var` are
    /// role-only spellings that resolve to this same type (see
    /// [`crate::object_type::ObjectType::from_role_name`]), and `.. c:var::`
    /// is a directive-name alias for the same definition (see the parser's
    /// `resolve_domain_object_type`) — neither is a variant of its own.
    Member,
    Struct,
    Union,
    Type,
}

impl CObjectType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Macro => "macro",
            Self::Member => "member",
            Self::Struct => "struct",
            Self::Union => "union",
            Self::Type => "type",
        }
    }
}

impl std::str::FromStr for CObjectType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "function" => Ok(Self::Function),
            "macro" => Ok(Self::Macro),
            "member" => Ok(Self::Member),
            "struct" => Ok(Self::Struct),
            "union" => Ok(Self::Union),
            "type" => Ok(Self::Type),
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
        let input = "enum";

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
    fn test_c_object_type_from_str_accepts_member() {
        // Given / When / Then
        assert_eq!("member".parse::<CObjectType>(), Ok(CObjectType::Member));
    }

    #[test]
    fn test_c_object_type_as_str_returns_member() {
        // Given / When / Then
        assert_eq!(CObjectType::Member.as_str(), "member");
    }

    #[test]
    fn test_c_object_type_from_str_rejects_data() {
        // Given — "data" is a role-only spelling, never a real object-type tag.
        let input = "data";

        // When
        let result = input.parse::<CObjectType>();

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_c_object_type_from_str_accepts_struct() {
        // Given / When / Then
        assert_eq!("struct".parse::<CObjectType>(), Ok(CObjectType::Struct));
    }

    #[test]
    fn test_c_object_type_as_str_returns_struct() {
        // Given / When / Then
        assert_eq!(CObjectType::Struct.as_str(), "struct");
    }

    #[test]
    fn test_c_object_type_from_str_accepts_union() {
        // Given / When / Then
        assert_eq!("union".parse::<CObjectType>(), Ok(CObjectType::Union));
    }

    #[test]
    fn test_c_object_type_as_str_returns_union() {
        // Given / When / Then
        assert_eq!(CObjectType::Union.as_str(), "union");
    }

    #[test]
    fn test_c_object_type_ord_orders_variants_by_declaration_order() {
        // Given
        let function = CObjectType::Function;
        let macro_ = CObjectType::Macro;
        let member = CObjectType::Member;
        let struct_ = CObjectType::Struct;
        let union_ = CObjectType::Union;
        let type_ = CObjectType::Type;

        // When / Then
        assert!(function < macro_);
        assert!(macro_ < member);
        assert!(member < struct_);
        assert!(struct_ < union_);
        assert!(union_ < type_);
    }

    #[test]
    fn test_c_object_type_from_str_accepts_type() {
        // Given / When / Then
        assert_eq!("type".parse::<CObjectType>(), Ok(CObjectType::Type));
    }

    #[test]
    fn test_c_object_type_as_str_returns_type() {
        // Given / When / Then
        assert_eq!(CObjectType::Type.as_str(), "type");
    }
}
