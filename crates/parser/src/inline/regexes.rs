//! The role and link regexes [`super::text::parse_inline_text`] scans with,
//! plus the table pairing each one with the `kind` tag
//! [`super::dispatch::handle_inline_match`] dispatches on.

use regex::Regex;
use std::sync::LazyLock;

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
pub(super) static REF_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r":{EXTERNAL_PREFIX}ref:`(?P<target>[^`]+)`")).unwrap());
pub(super) static ANY_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r":{EXTERNAL_PREFIX}any:`(?P<target>[^`]+)`")).unwrap());
/// `:doc:`, also spelled `:std:doc:` — the only `std` role here Sphinx
/// documents with its domain written out as often as without.
pub(super) static DOC_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":{EXTERNAL_PREFIX}(?:std:)?doc:`(?P<target>[^`]+)`"
    ))
    .unwrap()
});
/// `:numref:`, also spelled `:std:numref:`. Matches an [`EXTERNAL_PREFIX`]
/// only so the handler can refuse it by name: an inventory holds no numbers.
pub(super) static NUMREF_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":(?P<external>{EXTERNAL_PREFIX})(?:std:)?numref:`(?P<target>[^`]+)`"
    ))
    .unwrap()
});
/// `:download:`, also spelled `:std:download:`. No [`EXTERNAL_PREFIX`]: it
/// names a file this site serves, which no inventory lists.
pub(super) static DOWNLOAD_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:std:)?download:`(?P<target>[^`]+)`").unwrap());
pub(super) static PROGRAM_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":program:`(?P<name>[^`]+)`").unwrap());
pub(super) static TERM_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r":{EXTERNAL_PREFIX}term:`(?P<content>[^`]+)`")).unwrap());
pub(super) static OPTION_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r":{EXTERNAL_PREFIX}option:`(?P<content>[^`]+)`")).unwrap()
});
pub(super) static MATH_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":math:`(?P<latex>[^`]+)`").unwrap());
/// `:code:`. No [`EXTERNAL_PREFIX`] and no domain: it is markup, not a
/// reference. A role *derived* from it by `.. role::` has a name only the
/// document knows, so it is matched by [`NAMED_ROLE_REGEX`]'s shape instead.
pub(super) static CODE_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":code:`(?P<code>[^`]+)`").unwrap());
/// `:pep:`, `:rfc:`, `:cve:` and `:cwe:`, the registry roles: one pattern,
/// since they are one construct whose `registry` capture names the registry.
/// No [`EXTERNAL_PREFIX`] and no domain: they link outside the site by
/// construction, through a registry's own address rather than an inventory.
pub(super) static REGISTRY_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?P<registry>pep|rfc|cve|cwe):`(?P<target>[^`]+)`").unwrap());
/// docutils' `:pep-reference:`, `:pep:`'s plain sibling. No
/// [`EXTERNAL_PREFIX`] and no domain, for the reason `:pep:` has none.
pub(super) static PEP_REFERENCE_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":pep-reference:`(?P<target>[^`]+)`").unwrap());
/// docutils' `:rfc-reference:`, `:rfc:`'s plain sibling, for the same reason
/// without a prefix or a domain.
pub(super) static RFC_REFERENCE_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":rfc-reference:`(?P<target>[^`]+)`").unwrap());
/// `:index:`, which indexes the place it is written and shows its text. No
/// [`EXTERNAL_PREFIX`] and no domain: it names no target to look up.
pub(super) static INDEX_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":index:`(?P<content>[^`]+)`").unwrap());
/// `:sub:`/`:subscript:` and `:sup:`/`:superscript:`: one pattern, since
/// they are one construct whose `role` capture says which side of the line.
/// No [`EXTERNAL_PREFIX`] and no domain: they are markup, not a reference.
pub(super) static SCRIPT_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r":(?P<role>sub|subscript|sup|superscript):`(?P<text>[^`]+)`").unwrap()
});
/// `:title-reference:`, also spelled `:title:` and `:t:`. No
/// [`EXTERNAL_PREFIX`] and no domain: it is markup, not a reference.
pub(super) static TITLE_REFERENCE_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:title-reference|title|t):`(?P<text>[^`]+)`").unwrap());
pub(super) static EQ_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":eq:`(?P<label>[^`]+)`").unwrap());
pub(super) static FUNC_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":{EXTERNAL_PREFIX}(?:(?P<domain>py|c):)?func:`(?P<name>[^`]+)`"
    ))
    .unwrap()
});
pub(super) static MOD_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":{EXTERNAL_PREFIX}(?:(?P<domain>py):)?mod:`(?P<name>[^`]+)`"
    ))
    .unwrap()
});
pub(super) static DATA_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":{EXTERNAL_PREFIX}(?:(?P<domain>py|c):)?(?P<role>data|const|var|member):`(?P<name>[^`]+)`"
    ))
    .unwrap()
});
pub(super) static METH_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":{EXTERNAL_PREFIX}(?:(?P<domain>py):)?meth:`(?P<name>[^`]+)`"
    ))
    .unwrap()
});
pub(super) static CLASS_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":{EXTERNAL_PREFIX}(?:(?P<domain>py):)?class:`(?P<name>[^`]+)`"
    ))
    .unwrap()
});
pub(super) static ATTR_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":{EXTERNAL_PREFIX}(?:(?P<domain>py):)?attr:`(?P<name>[^`]+)`"
    ))
    .unwrap()
});
pub(super) static EXC_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":{EXTERNAL_PREFIX}(?:(?P<domain>py):)?exc:`(?P<name>[^`]+)`"
    ))
    .unwrap()
});
pub(super) static MACRO_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":{EXTERNAL_PREFIX}(?:(?P<domain>c):)?macro:`(?P<name>[^`]+)`"
    ))
    .unwrap()
});
pub(super) static STRUCT_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":{EXTERNAL_PREFIX}(?:(?P<domain>c):)?struct:`(?P<name>[^`]+)`"
    ))
    .unwrap()
});
pub(super) static UNION_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":{EXTERNAL_PREFIX}(?:(?P<domain>c):)?union:`(?P<name>[^`]+)`"
    ))
    .unwrap()
});
pub(super) static TYPE_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r":{EXTERNAL_PREFIX}(?:(?P<domain>c):)?type:`(?P<name>[^`]+)`"
    ))
    .unwrap()
});
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

/// Every role/link regex tried by [`parse_inline_text`], paired with the
/// `kind` tag [`handle_inline_match`] dispatches on. A table rather than one
/// `let X_match = ...; if let Some(m) = X_match { ... }` pair per regex
/// (which is what this used to be, and grew one clippy line-count warning
/// past its limit the moment a 20th regex — `:option:`'s — joined it): match
/// order here doesn't matter, since [`parse_inline_text`] always picks the
/// earliest (then longest) match regardless of table position.
pub(super) static SIMPLE_ROLE_REGEXES: &[(&LazyLock<Regex>, &str)] = &[
    (&REF_REGEX, "ref"),
    (&ANY_ROLE_REGEX, "any"),
    (&DOC_ROLE_REGEX, "doc"),
    (&DOWNLOAD_ROLE_REGEX, "download"),
    (&NUMREF_ROLE_REGEX, "numref"),
    (&PROGRAM_ROLE_REGEX, "program"),
    (&TERM_ROLE_REGEX, "term"),
    (&MATH_ROLE_REGEX, "math"),
    (&EQ_ROLE_REGEX, "eq"),
    (&CODE_ROLE_REGEX, "code"),
    (&SCRIPT_ROLE_REGEX, "script"),
    (&TITLE_REFERENCE_ROLE_REGEX, "title-reference"),
    (&REGISTRY_ROLE_REGEX, "registry"),
    (&PEP_REFERENCE_ROLE_REGEX, "pep-reference"),
    (&RFC_REFERENCE_ROLE_REGEX, "rfc-reference"),
    (&INDEX_ROLE_REGEX, "index"),
    (&OPTION_ROLE_REGEX, "option"),
    (&FUNC_ROLE_REGEX, "func"),
    (&MOD_ROLE_REGEX, "mod"),
    (&DATA_ROLE_REGEX, "data"),
    (&METH_ROLE_REGEX, "meth"),
    (&CLASS_ROLE_REGEX, "class"),
    (&ATTR_ROLE_REGEX, "attr"),
    (&EXC_ROLE_REGEX, "exc"),
    (&MACRO_ROLE_REGEX, "macro"),
    (&STRUCT_ROLE_REGEX, "struct"),
    (&UNION_ROLE_REGEX, "union"),
    (&TYPE_ROLE_REGEX, "type"),
    (&ANONYMOUS_PHRASED_REGEX, "anon_phrased"),
    (&PHRASED_LINK_REGEX, "phrased"),
    (&ANONYMOUS_SIMPLE_REGEX, "anon_simple"),
    (&SIMPLE_LINK_REGEX, "simple"),
    // Last on purpose. This one matches *any* role spelling, so it would
    // otherwise shadow every specific role above it: the matcher keeps the
    // first entry among equally-placed matches, which makes table order the
    // precedence rule. Whether the name it captured is really a role — one the
    // document defined with `.. role::`, or an entity role — is decided in
    // `handle_inline_match`, not here: a regex cannot know a project's
    // vocabulary, nor a document's.
    (&NAMED_ROLE_REGEX, "named_role"),
];

/// The shape of a role name this scan can match: what [`NAMED_ROLE_REGEX`]
/// captures, and so what a `.. role::` must name for its role to be usable.
const ROLE_NAME: &str = "[a-zA-Z][a-zA-Z0-9_-]*";

/// Any ``:name:`target` `` role, for the roles whose names are not fixed: the
/// entity roles a schema declares, and the roles a document defines with
/// `.. role::`.
///
/// Deliberately one static pattern rather than a regex compiled per project:
/// the role names are configurable, but their *syntax* is not, so matching the
/// shape here and checking the name afterwards keeps this table static.
pub(super) static NAMED_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r":(?P<role>{ROLE_NAME}):`(?P<target>[^`]+)`")).unwrap());

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
/// cannot drift from the table: a role added to [`SIMPLE_ROLE_REGEXES`] is
/// reserved the moment it is.
pub(crate) fn is_fixed_role_name(name: &str) -> bool {
    let sample = format!(":{name}:`x`");
    SIMPLE_ROLE_REGEXES.iter().any(|(regex, kind)| {
        *kind != "named_role" && regex.find(&sample).is_some_and(|m| m.start() == 0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
