//! A role no table, schema, `.. role::` or ignore list answers to: refused, so
//! the whole-document pass reports it, and shown as the source it was written
//! as.
//!
//! Two refusals, told apart by [`UNSUPPORTED_ROLES`]: a role docutils or
//! Sphinx defines that this build does not implement yet is no mistake of the
//! author's, and is reported as such; any other is unknown — a typo, or an
//! extension's.

use rinx_ast::{InlineNode, RoleRefusal};

/// The roles docutils and Sphinx define that this build does not implement,
/// each with the domain its name may be qualified with.
///
/// A role of a domain may be written bare too, as in the default domain;
/// a role of none only bare. A test refuses a role this build implements, so
/// implementing one means removing it here.
const UNSUPPORTED_ROLES: &[(Option<&str>, &[&str])] = &[
    // docutils' own.
    (
        None,
        &[
            "emphasis",
            "strong",
            "literal",
            "abbreviation",
            "ab",
            "acronym",
            "ac",
            "raw",
        ],
    ),
    // Sphinx's, outside any domain.
    (
        None,
        &[
            "abbr",
            "command",
            "dfn",
            "file",
            "guilabel",
            "kbd",
            "mailheader",
            "makevar",
            "manpage",
            "menuselection",
            "mimetype",
            "newsgroup",
            "regexp",
            "samp",
        ],
    ),
    (Some("std"), &["envvar", "keyword", "token", "confval"]),
    (Some("py"), &["obj", "deco", "type"]),
    (Some("c"), &["expr", "texpr", "enum", "enumerator"]),
];

/// Whether `name`, as written, is a docutils or Sphinx role this build does
/// not implement.
fn is_unsupported_role(name: &str) -> bool {
    UNSUPPORTED_ROLES.iter().any(|(domain, names)| {
        let bare = domain.map_or(name, |domain| {
            name.strip_prefix(domain)
                .and_then(|rest| rest.strip_prefix(':'))
                .unwrap_or(name)
        });
        names.contains(&bare)
    })
}

/// `source`, a role named `name` that nothing answers to, refused as
/// unsupported or unknown.
pub(super) fn refused_unknown_role(source: &str, name: &str) -> InlineNode {
    let name = name.to_string();
    let refusal = if is_unsupported_role(&name) {
        RoleRefusal::UnsupportedRole { name }
    } else {
        RoleRefusal::UnknownRole { name }
    };
    InlineNode::RefusedRole {
        text: source.to_string(),
        refusal,
        span: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline::is_fixed_role_name;

    #[test]
    fn test_no_unsupported_role_is_one_this_build_implements() {
        // Given / When
        let implemented: Vec<String> = UNSUPPORTED_ROLES
            .iter()
            .flat_map(|(domain, names)| {
                names.iter().map(move |name| {
                    domain.map_or_else(|| (*name).to_string(), |domain| format!("{domain}:{name}"))
                })
            })
            .filter(|spelled| is_fixed_role_name(spelled))
            .collect();

        // Then
        assert_eq!(implemented, Vec::<String>::new());
    }

    #[test]
    fn test_is_unsupported_role_takes_a_domain_role_bare_or_qualified() {
        // Given / When / Then
        assert!(is_unsupported_role("kbd"));
        assert!(is_unsupported_role("envvar"));
        assert!(is_unsupported_role("std:envvar"));
        assert!(is_unsupported_role("c:expr"));
        assert!(!is_unsupported_role("py:expr"));
        assert!(!is_unsupported_role("std:kbd"));
        assert!(!is_unsupported_role("issue"));
    }

    #[test]
    fn test_refused_unknown_role_tells_an_unsupported_role_from_an_unknown_one() {
        // Given / When
        let unsupported = refused_unknown_role(":kbd:`C-x`", "kbd");
        let unknown = refused_unknown_role(":kdb:`C-x`", "kdb");

        // Then
        assert_eq!(
            unsupported,
            InlineNode::RefusedRole {
                text: ":kbd:`C-x`".to_string(),
                refusal: RoleRefusal::UnsupportedRole {
                    name: "kbd".to_string()
                },
                span: None,
            }
        );
        assert!(
            matches!(
                &unknown,
                InlineNode::RefusedRole {
                    refusal: RoleRefusal::UnknownRole { name },
                    ..
                } if name == "kdb"
            ),
            "{unknown:?}"
        );
    }
}
