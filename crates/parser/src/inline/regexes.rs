//! The role and link regexes [`super::text::parse_inline_text`] scans with,
//! plus the table pairing each one with the `kind` tag
//! [`super::dispatch::handle_inline_match`] dispatches on.

use regex::Regex;
use std::sync::LazyLock;

pub(super) static REF_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":ref:`(?P<target>[^`]+)`").unwrap());
pub(super) static PROGRAM_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":program:`(?P<name>[^`]+)`").unwrap());
pub(super) static TERM_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":term:`(?P<content>[^`]+)`").unwrap());
pub(super) static OPTION_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":option:`(?P<content>[^`]+)`").unwrap());
pub(super) static FUNC_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py|c):)?func:`(?P<name>[^`]+)`").unwrap());
pub(super) static MOD_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py):)?mod:`(?P<name>[^`]+)`").unwrap());
pub(super) static DATA_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r":(?:(?P<domain>py|c):)?(?P<role>data|const|var|member):`(?P<name>[^`]+)`").unwrap()
});
pub(super) static METH_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py):)?meth:`(?P<name>[^`]+)`").unwrap());
pub(super) static CLASS_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py):)?class:`(?P<name>[^`]+)`").unwrap());
pub(super) static ATTR_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py):)?attr:`(?P<name>[^`]+)`").unwrap());
pub(super) static EXC_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py):)?exc:`(?P<name>[^`]+)`").unwrap());
pub(super) static MACRO_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>c):)?macro:`(?P<name>[^`]+)`").unwrap());
pub(super) static STRUCT_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>c):)?struct:`(?P<name>[^`]+)`").unwrap());
pub(super) static UNION_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>c):)?union:`(?P<name>[^`]+)`").unwrap());
pub(super) static TYPE_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>c):)?type:`(?P<name>[^`]+)`").unwrap());
pub(super) static PHRASED_LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`(?P<text>[^`]+)`_").unwrap());
pub(super) static SIMPLE_LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?P<name>[a-zA-Z0-9_.-]+)_\b").unwrap());
pub(super) static EMBEDDED_URI_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?P<text>.*)\s+<(?P<uri>[^>]+)>$").unwrap());
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
    (&PROGRAM_ROLE_REGEX, "program"),
    (&TERM_ROLE_REGEX, "term"),
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
];
