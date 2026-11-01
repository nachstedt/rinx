//! The role and link regexes [`super::text::parse_inline_text`] scans with,
//! plus the table pairing each one with the `kind` tag
//! [`super::dispatch::handle_inline_match`] dispatches on.
//!
//! Every role's pattern is built from its [`RoleSpec`] by one function, so
//! the names a role is listed under are exactly the names it is matched by.

use regex::Regex;
use rinx_ast::Domain;
use std::sync::LazyLock;

use super::roles::c::{macro_, struct_, type_, union};
use super::roles::py::{attr, class, data, exc, func, meth, mod_};
use super::roles::spec::RoleSpec;
use super::roles::std_::{option, program, ref_, term};
use super::roles::{
    any, code, doc, download, index, math, numref, pep_reference, registry, rfc_reference, script,
    title_reference,
};

/// Sphinx's optional `external:` / `external+name:` role prefix, spliced in
/// right after the leading colon of every role that can resolve through
/// another site's inventory. Part of each role's own pattern — rather than a
/// separate token — so the match starts at the prefix, which the earliest-match
/// rule then prefers over the bare role inside it. Its meaning is read back by
/// [`EXTERNAL_PREFIX_REGEX`]; the name excludes exactly what
/// `rinx_ast::InventoryName` refuses.
const EXTERNAL_PREFIX: &str = r"(?:external(?:\+[^:`<>\s+]+)?:)?";

/// Reads the prefix [`EXTERNAL_PREFIX`] let a role match with.
pub(super) static EXTERNAL_PREFIX_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^:external(?:\+(?P<inventory>[^:`<>\s+]+))?:").unwrap());

/// The pattern matching every spelling `spec` lists, written as
/// ``:name:`text` ``, with its text captured as `body`: the bare name or the
/// name qualified with one of its domains (captured as `domain`), the name
/// itself captured as `role`, and the [`EXTERNAL_PREFIX`] (captured as
/// `external`) for a role that takes one.
///
/// The bare name always matches, whatever the default domain: a bare role its
/// default domain lacks (`:macro:` in a `py` document) is the handler's to
/// turn back into text, which needs the domain the pattern cannot know.
fn role_regex(spec: &RoleSpec, body: &str) -> Regex {
    let external = if spec.external.is_read() {
        format!("(?P<external>{EXTERNAL_PREFIX})")
    } else {
        String::new()
    };
    let domain = if spec.domains.is_empty() {
        String::new()
    } else {
        let domains: Vec<&str> = spec.domains.iter().map(Domain::as_str).collect();
        format!("(?:(?P<domain>{}):)?", domains.join("|"))
    };
    let names: Vec<String> = spec.names.iter().map(|name| regex::escape(name)).collect();
    Regex::new(&format!(
        r":{external}{domain}(?P<role>{}):`(?P<{body}>[^`]+)`",
        names.join("|")
    ))
    .unwrap()
}

pub(super) static REF_REGEX: LazyLock<Regex> = LazyLock::new(|| role_regex(&ref_::SPEC, "target"));
pub(super) static ANY_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&any::SPEC, "target"));
pub(super) static DOC_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&doc::SPEC, "target"));
pub(super) static NUMREF_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&numref::SPEC, "target"));
pub(super) static DOWNLOAD_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&download::SPEC, "target"));
pub(super) static PROGRAM_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&program::SPEC, "name"));
pub(super) static TERM_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&term::SPEC, "content"));
pub(super) static OPTION_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&option::SPEC, "content"));
pub(super) static MATH_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&math::MATH_SPEC, "latex"));
pub(super) static CODE_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&code::SPEC, "code"));
pub(super) static REGISTRY_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&registry::SPEC, "target"));
pub(super) static PEP_REFERENCE_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&pep_reference::SPEC, "target"));
pub(super) static RFC_REFERENCE_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&rfc_reference::SPEC, "target"));
pub(super) static INDEX_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&index::SPEC, "content"));
pub(super) static SCRIPT_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&script::SPEC, "text"));
pub(super) static TITLE_REFERENCE_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&title_reference::SPEC, "text"));
pub(super) static EQ_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&math::EQ_SPEC, "label"));
pub(super) static FUNC_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&func::SPEC, "name"));
pub(super) static MOD_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&mod_::SPEC, "name"));
pub(super) static PY_DATA_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&data::PY_SPEC, "name"));
pub(super) static C_MEMBER_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&data::C_SPEC, "name"));
pub(super) static METH_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&meth::SPEC, "name"));
pub(super) static CLASS_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&class::SPEC, "name"));
pub(super) static ATTR_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&attr::SPEC, "name"));
pub(super) static EXC_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&exc::SPEC, "name"));
pub(super) static MACRO_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&macro_::SPEC, "name"));
pub(super) static STRUCT_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&struct_::SPEC, "name"));
pub(super) static UNION_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&union::SPEC, "name"));
pub(super) static TYPE_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| role_regex(&type_::SPEC, "name"));
pub(super) static PHRASED_LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`(?P<text>[^`]+)`_").unwrap());
pub(super) static SIMPLE_LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?P<name>[a-zA-Z0-9_.-]+)_\b").unwrap());
/// The embedded destination ending a phrase: `` `text <destination>`_ ``, or
/// a phrase that is nothing but one, `` `<destination>`_ ``. The text may wrap
/// over lines, and must be separated from the `<` by whitespace, as docutils
/// requires.
pub(super) static EMBEDDED_URI_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)^(?:(?P<text>.*?)\s+)?<(?P<uri>[^<>]+)>$").unwrap());
pub(super) static ANONYMOUS_PHRASED_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`(?P<text>[^`]+)`__").unwrap());
pub(super) static ANONYMOUS_SIMPLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?P<name>[a-zA-Z0-9_.-]+)__\b").unwrap());

/// One role of the table: its spec, the pattern built from it, and the
/// `kind` tag [`handle_inline_match`] dispatches it on.
pub(super) struct Role {
    pub(super) spec: &'static RoleSpec,
    pub(super) pattern: &'static LazyLock<Regex>,
    pub(super) kind: &'static str,
}

/// Every role with a fixed name this build implements: the one list pairing
/// spec, pattern and handler, so a role cannot be matched without a spec
/// saying it exists, nor listed without being matched.
///
/// `py`'s `:data:`/`:const:` and `c`'s `:member:`/`:data:`/`:var:` are two
/// entries rather than one, though they share a handler: they are not one set
/// of names, and a `:py:var:` is no role.
pub(super) static ROLES: &[Role] = &[
    role(&ref_::SPEC, &REF_REGEX, "ref"),
    role(&any::SPEC, &ANY_ROLE_REGEX, "any"),
    role(&doc::SPEC, &DOC_ROLE_REGEX, "doc"),
    role(&download::SPEC, &DOWNLOAD_ROLE_REGEX, "download"),
    role(&numref::SPEC, &NUMREF_ROLE_REGEX, "numref"),
    role(&program::SPEC, &PROGRAM_ROLE_REGEX, "program"),
    role(&term::SPEC, &TERM_ROLE_REGEX, "term"),
    role(&math::MATH_SPEC, &MATH_ROLE_REGEX, "math"),
    role(&math::EQ_SPEC, &EQ_ROLE_REGEX, "eq"),
    role(&code::SPEC, &CODE_ROLE_REGEX, "code"),
    role(&script::SPEC, &SCRIPT_ROLE_REGEX, "script"),
    role(
        &title_reference::SPEC,
        &TITLE_REFERENCE_ROLE_REGEX,
        "title-reference",
    ),
    role(&registry::SPEC, &REGISTRY_ROLE_REGEX, "registry"),
    role(
        &pep_reference::SPEC,
        &PEP_REFERENCE_ROLE_REGEX,
        "pep-reference",
    ),
    role(
        &rfc_reference::SPEC,
        &RFC_REFERENCE_ROLE_REGEX,
        "rfc-reference",
    ),
    role(&index::SPEC, &INDEX_ROLE_REGEX, "index"),
    role(&option::SPEC, &OPTION_ROLE_REGEX, "option"),
    role(&func::SPEC, &FUNC_ROLE_REGEX, "func"),
    role(&mod_::SPEC, &MOD_ROLE_REGEX, "mod"),
    role(&data::PY_SPEC, &PY_DATA_ROLE_REGEX, "data"),
    role(&data::C_SPEC, &C_MEMBER_ROLE_REGEX, "member"),
    role(&meth::SPEC, &METH_ROLE_REGEX, "meth"),
    role(&class::SPEC, &CLASS_ROLE_REGEX, "class"),
    role(&attr::SPEC, &ATTR_ROLE_REGEX, "attr"),
    role(&exc::SPEC, &EXC_ROLE_REGEX, "exc"),
    role(&macro_::SPEC, &MACRO_ROLE_REGEX, "macro"),
    role(&struct_::SPEC, &STRUCT_ROLE_REGEX, "struct"),
    role(&union::SPEC, &UNION_ROLE_REGEX, "union"),
    role(&type_::SPEC, &TYPE_ROLE_REGEX, "type"),
];

/// Every role with a fixed name this build implements, in the table's order.
pub fn role_specs() -> impl Iterator<Item = &'static RoleSpec> {
    ROLES.iter().map(|role| role.spec)
}

const fn role(
    spec: &'static RoleSpec,
    pattern: &'static LazyLock<Regex>,
    kind: &'static str,
) -> Role {
    Role {
        spec,
        pattern,
        kind,
    }
}

/// The hyperlink references, which the scan tries after the roles. They are
/// no roles, so they have no spec.
static LINKS: &[(&LazyLock<Regex>, &str)] = &[
    (&ANONYMOUS_PHRASED_REGEX, "anon_phrased"),
    (&PHRASED_LINK_REGEX, "phrased"),
    (&ANONYMOUS_SIMPLE_REGEX, "anon_simple"),
    (&SIMPLE_LINK_REGEX, "simple"),
];

/// Every pattern tried by [`parse_inline_text`], paired with the `kind` tag
/// [`handle_inline_match`] dispatches on: the roles, the links, and last the
/// catch-all [`NAMED_ROLE_REGEX`].
///
/// The order matters only among equally-placed matches: the scan picks the
/// earliest (then longest) match, and among those the first pattern here.
/// That makes [`NAMED_ROLE_REGEX`]'s place last the precedence rule — it
/// matches *any* role spelling, so it would otherwise shadow every specific
/// role. Whether the name it captured is really a role — one the document
/// defined with `.. role::`, or an entity role — is decided in
/// `handle_inline_match`, not here: a regex cannot know a project's
/// vocabulary, nor a document's.
///
/// [`parse_inline_text`]: super::text::parse_inline_text
/// [`handle_inline_match`]: super::dispatch::handle_inline_match
pub(super) fn scan_patterns() -> impl Iterator<Item = (&'static Regex, &'static str)> {
    ROLES
        .iter()
        .map(|role| (&**role.pattern, role.kind))
        .chain(LINKS.iter().map(|(pattern, kind)| (&***pattern, *kind)))
        .chain(std::iter::once((&*NAMED_ROLE_REGEX, "named_role")))
}

/// The shape of a role name this scan can match: what [`NAMED_ROLE_REGEX`]
/// captures, and so what a `.. role::` must name for its role to be usable.
const ROLE_NAME: &str = "[a-zA-Z][a-zA-Z0-9_-]*";

/// Any ``:name:`target` `` role, for the roles whose names are not fixed: the
/// entity roles a schema declares, the roles a document defines with
/// `.. role::`, and the roles a library's parse configuration ignores.
///
/// A name may carry domain prefixes (``:cpp:class:``), so a role of a domain
/// this build does not have is read whole rather than as text followed by a
/// role of the same name in another domain. Neither a schema nor a `.. role::`
/// can name such a role; an ignored one can.
///
/// Deliberately one static pattern rather than a regex compiled per project:
/// the role names are configurable, but their *syntax* is not, so matching the
/// shape here and checking the name afterwards keeps this table static.
pub(super) static NAMED_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":(?P<role>{ROLE_NAME}(?::{ROLE_NAME})*):`(?P<target>[^`]+)`"
    ))
    .unwrap()
});

/// Whether `name` could be written as a role this scan recognizes.
pub(crate) fn is_writable_role_name(name: &str) -> bool {
    static WHOLE_NAME: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(&format!("^{ROLE_NAME}$")).unwrap());
    WHOLE_NAME.is_match(name)
}

/// Whether one of the fixed-spelling roles this scan implements would match
/// ``:name:`…` `` before a custom role of that name could — `code` included.
///
/// Asked with a sample role rather than of a list of names, so the answer
/// cannot drift from the table: a role added to [`ROLES`] is
/// reserved the moment it is.
pub(crate) fn is_fixed_role_name(name: &str) -> bool {
    let sample = format!(":{name}:`x`");
    ROLES
        .iter()
        .any(|role| role.pattern.find(&sample).is_some_and(|m| m.start() == 0))
}

#[cfg(test)]
mod tests {
    use rinx_ast::InlineNode;

    use super::*;
    use crate::context::ParseCtx;
    use crate::inline::dispatch::handle_inline_match;
    use crate::inline::find_role_kind;

    /// The node ``:spelling:`x` `` becomes in a document of
    /// `default_domain`, through whichever role the scan picks for it — or
    /// `None` when it picks none.
    fn node(spelling: &str, default_domain: Domain) -> Option<InlineNode> {
        let ctx = ParseCtx::with_domain(default_domain);
        let kind = find_role_kind(spelling, &ctx)?;
        Some(handle_inline_match(
            kind,
            &format!(":{spelling}:`x`"),
            None,
            default_domain,
            &ctx,
        ))
    }

    /// Whether some spec takes `name` bare in a document of `default_domain`.
    fn accepted_bare(name: &str, default_domain: Domain) -> bool {
        ROLES.iter().any(|role| {
            role.spec.names.contains(&name) && role.spec.accepts_bare_name(default_domain)
        })
    }

    #[test]
    fn test_every_spelling_a_spec_lists_is_read_as_a_role() {
        for default_domain in [Domain::Py, Domain::C] {
            for role in ROLES {
                for spelling in role.spec.spellings(default_domain) {
                    // When
                    let node = node(&spelling, default_domain);

                    // Then
                    assert!(
                        node.is_some_and(|node| !matches!(node, InlineNode::Text(_))),
                        "{spelling} in {default_domain}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_a_qualified_spelling_is_read_as_its_own_role() {
        let ctx = ParseCtx::with_domain(Domain::Py);
        for role in ROLES {
            for spelling in role.spec.spellings(Domain::Py).filter(|s| s.contains(':')) {
                // When / Then
                assert_eq!(
                    find_role_kind(&spelling, &ctx),
                    Some(role.kind),
                    "{spelling}"
                );
            }
        }
    }

    #[test]
    fn test_a_bare_name_no_spec_takes_stays_text() {
        for default_domain in [Domain::Py, Domain::C] {
            for name in ROLES
                .iter()
                .flat_map(|role| role.spec.names)
                .filter(|name| !accepted_bare(name, default_domain))
            {
                // When / Then
                assert!(
                    matches!(node(name, default_domain), Some(InlineNode::Text(_))),
                    "{name} in {default_domain}"
                );
            }
        }
    }

    #[test]
    fn test_only_a_spec_taking_an_external_prefix_matches_one() {
        let ctx = ParseCtx::with_domain(Domain::Py);
        for role in ROLES {
            for spelling in role.spec.spellings(Domain::Py) {
                // When
                let kind = find_role_kind(&format!("external:{spelling}"), &ctx);

                // Then
                assert_eq!(kind.is_some(), role.spec.external.is_read(), "{spelling}");
            }
        }
    }

    #[test]
    fn test_a_data_spelling_its_domain_lacks_is_no_role() {
        // Given — `var` and `member` are `c`'s, `const` is `py`'s
        let ctx = ParseCtx::with_domain(Domain::Py);

        // When / Then
        for spelling in ["py:var", "py:member", "c:const"] {
            assert_eq!(find_role_kind(spelling, &ctx), None, "{spelling}");
        }
    }

    #[test]
    fn test_role_specs_lists_every_role_of_the_table() {
        // When / Then
        assert_eq!(role_specs().count(), ROLES.len());
        assert!(role_specs().all(|spec| !spec.names.is_empty() && !spec.doc.is_empty()));
    }

    #[test]
    fn test_is_writable_role_name_accepts_the_matched_shape() {
        // Given / When / Then
        assert!(is_writable_role_name("python"));
        assert!(is_writable_role_name("my-role_2"));
    }

    #[test]
    fn test_is_writable_role_name_refuses_what_the_scan_cannot_match() {
        // Given / When / Then
        assert!(!is_writable_role_name(""));
        assert!(!is_writable_role_name("2py"));
        assert!(!is_writable_role_name("a.b"));
        assert!(!is_writable_role_name("a b"));
    }

    #[test]
    fn test_is_fixed_role_name_knows_the_built_in_roles() {
        // Given / When / Then
        for name in [
            "code",
            "ref",
            "math",
            "func",
            "doc",
            "download",
            "numref",
            "pep",
            "pep-reference",
            "rfc",
            "rfc-reference",
            "cve",
            "cwe",
            "index",
            "sub",
            "subscript",
            "sup",
            "superscript",
            "title-reference",
            "title",
            "t",
        ] {
            assert!(is_fixed_role_name(name), "{name}");
        }
    }

    #[test]
    fn test_is_fixed_role_name_leaves_other_names_free() {
        // Given / When / Then
        assert!(!is_fixed_role_name("python"));
        assert!(!is_fixed_role_name("codex"));
        assert!(!is_fixed_role_name("red"));
        assert!(!is_fixed_role_name("subs"));
        assert!(!is_fixed_role_name("titles"));
    }
}
