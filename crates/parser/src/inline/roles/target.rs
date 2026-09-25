use crate::explicit_title::split_explicit_title;
use crate::inline::escapes::unescape;
use rinx_ast::{Domain, InlineNode, ObjectType, TargetSearchOrder};

/// The name/display/link-behavior of a domain-object role target, after
/// stripping the optional `!` (suppress link), `~` (shorten display to the
/// last dotted component) and `.` (search the enclosing scope first) prefixes.
pub(super) struct DomainObjectTarget {
    pub(super) name: String,
    pub(super) display: String,
    pub(super) link: bool,
    pub(super) search_order: TargetSearchOrder,
}

impl DomainObjectTarget {
    /// Builds the reference node for this target under `object_type`. Every
    /// role handler ends this way, so the node's shape is spelled out once
    /// here rather than once per role.
    pub(super) fn into_inline_node(self, object_type: ObjectType) -> InlineNode {
        InlineNode::DomainObjectReference {
            object_type,
            name: self.name,
            display: self.display,
            link: self.link,
            search_order: self.search_order,
            span: None,
            inventory: rinx_ast::InventorySelector::Any,
        }
    }
}

/// Parses a domain-object role's raw backtick-quoted target, resolving the
/// `!`/`~`/`.` prefix modifiers documented at
/// <https://www.sphinx-doc.org/en/master/usage/referencing.html> and
/// <https://www.sphinx-doc.org/en/master/usage/domains/python.html#target-resolution>,
/// as well as the explicit-title syntax (`` `Display text <target>` ``,
/// e.g. `` :func:`spawn\* <spawnl>` ``) shared with `:ref:`/`:term:` via
/// [`split_explicit_title`].
///
/// `!` is checked first and returns immediately, mirroring real Sphinx's
/// `XRefRole.run`: a suppressed reference becomes a plain literal before the
/// other prefixes are ever examined, so `` :func:`!~foo` `` displays a literal
/// `~foo`.
///
/// When an explicit title is present, it always wins for `display` —
/// regardless of a `~` prefix on the target — since there is nothing left to
/// shorten; `~`/`.` still apply to the target itself for `name`/
/// `search_order` either way.
///
/// A leading `.` never survives into `name` — no indexed object name can
/// contain an empty dotted segment, so leaving it in would guarantee a miss.
/// Unlike Sphinx, which strips exactly one dot from the target while stripping
/// all of them from the title, both are stripped fully here: the two disagree
/// only for degenerate input like `..foo`, where Sphinx's asymmetry just makes
/// the lookup unresolvable while displaying a name that looks resolvable.
///
/// A trailing `()` — written so the reference reads as a call at the point of
/// use, as in `` :c:func:`Py_TYPE()` `` — gets the same treatment: kept out of
/// `name`, kept in `display`, exactly as Sphinx renders it. Both domains strip
/// it but with different strictness, which is why `domain` has to be known
/// here; see [`strip_trailing_call_parens`]. A `!`-suppressed target is
/// excluded along with everything else, since it is never looked up.
pub(super) fn parse_domain_object_target(raw: &str, domain: Domain) -> DomainObjectTarget {
    // A role sees its content already un-escaped: docutils un-escapes the
    // captured text before handing it to the role, so `foo\(\)` reaches the
    // domain as `foo()` and strips its parens like any other call-shaped
    // target, and a `\~` genuinely does act as the shorten sigil. Matching on
    // the escaped form and un-escaping here (rather than earlier) is what
    // keeps an escaped backtick from ending the role's content too soon.
    let unescaped = unescape(raw);
    let raw = unescaped.as_str();

    if let Some(name) = raw.strip_prefix('!') {
        return DomainObjectTarget {
            name: name.to_string(),
            display: name.to_string(),
            link: false,
            search_order: TargetSearchOrder::default(),
        };
    }

    let (explicit_title, target_raw) = match split_explicit_title(raw) {
        Some((title, target)) => (Some(title), target),
        None => (None, raw.to_string()),
    };

    let (shorten_display, after_tilde) = match target_raw.strip_prefix('~') {
        Some(rest) => (true, rest),
        None => (false, target_raw.as_str()),
    };
    let name = after_tilde.trim_start_matches('.');
    let search_order = if name.len() == after_tilde.len() {
        TargetSearchOrder::LeastQualifiedFirst
    } else {
        TargetSearchOrder::MostQualifiedFirst
    };
    let display = match &explicit_title {
        Some(title) => title.clone(),
        None if shorten_display => name.rsplit('.').next().unwrap_or(name).to_string(),
        None => name.to_string(),
    };

    DomainObjectTarget {
        name: strip_trailing_call_parens(name, domain).to_string(),
        display,
        link: true,
        search_order,
    }
}

/// Strips the trailing `()` an author writes so a cross-reference reads as a
/// call at the point of use — `` :c:func:`Py_TYPE()` `` — from the name the
/// lookup is keyed by. The parens can never be part of that key: every domain
/// object is registered under the name its *declaration* provides, and both
/// `CSignature::parse` and `extract_python_object_name` cut that at the first
/// `(`.
///
/// Real Sphinx strips in both domains, at resolution time rather than in the
/// role, and the two spell it differently — which is the whole reason this
/// needs to know the `domain`:
///
/// - `py`: `PythonDomain.find_obj` opens with `name = name.removesuffix('()')`
///   (`sphinx/domains/python/__init__.py`, under the comment `# skip parens`).
/// - `c`: `DefinitionParser.parse_xref_object` parses a nested name and then
///   runs `skip_ws(); skip_string('()'); assert_end()`
///   (`sphinx/domains/c/_parser.py`, under the comment `# if there are '()'
///   left, just skip them`).
///
/// The `skip_ws()` is the whole difference: `` :c:func:`Py_TYPE ()` ``
/// resolves under real Sphinx, `` :py:func:`sys.exit ()` `` does not.
/// Python's `removesuffix` *does* strip that target's parens, but leaves the
/// space behind, and `find_obj` then misses on a dict keyed by exact names.
/// Porting `removesuffix` literally would not reproduce that miss here:
/// `TargetName` collapses and trims whitespace on the way into the index, so
/// `"sys.exit "` would quietly resolve. The `py` arm therefore declines to
/// strip at all when a space precedes the parens — the same unresolved
/// outcome, and the diagnostic still shows the target the author typed.
///
/// Exactly one pair is stripped, and only as a suffix. Every consequence of
/// that agrees with what real Sphinx ends up doing:
///
/// - `foo()()` loses the outer pair and then misses, as it does under
///   Python's single `removesuffix` and under C's `assert_end()`, which
///   rejects the leftover.
/// - `foo(a)` and `foo()bar` are left alone: neither Python's `removesuffix`
///   nor C's `skip_string('()')`/`assert_end()` accepts those either.
/// - A target that is *nothing but* `()` keeps its text rather than collapsing
///   to the empty string, so a broken-link diagnostic still has something to
///   name. Both domains refuse such a target anyway (`if not name: return []`
///   in `find_obj`; `_parse_nested_name` raises in `c`), so only the wording
///   of the warning differs.
pub(super) fn strip_trailing_call_parens(name: &str, domain: Domain) -> &str {
    let Some(head) = name.strip_suffix("()") else {
        return name;
    };
    let head = match domain {
        Domain::C => head.trim_end(),
        Domain::Py if head.ends_with(char::is_whitespace) => return name,
        Domain::Py => head,
        // Only ever reached via a role's explicit `py:`/`c:` prefix or
        // `default_domain` — the role regexes never capture `std` as an
        // explicit prefix, and `default_domain` is restricted to `py`/`c` at
        // the CLI/Bazel-attribute boundary (`parse_default_domain_flag`,
        // `rules/library.bzl`'s `values = ["py", "c"]`) precisely because
        // other bare-role code paths make the same assumption. `:option:`
        // (the one `std`-domain role) has its own `InlineNode::OptionReference`
        // parse path and never calls this function.
        Domain::Std => unreachable!("strip_trailing_call_parens called with Domain::Std"),
    };
    if head.is_empty() { name } else { head }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::TargetSearchOrder;

    /// Escapes `raw` the way `parse_inline_text` does before any of the
    /// helpers below see it, so a unit test exercises the form those helpers
    /// are actually handed rather than a raw backslash they never meet.
    fn escaped(raw: &str) -> String {
        crate::inline::escapes::EscapedText::new(raw)
            .as_str()
            .to_string()
    }

    #[test]
    fn test_parse_domain_object_target_plain_name() {
        let target = parse_domain_object_target("foo", Domain::Py);
        assert_eq!(target.name, "foo");
        assert_eq!(target.display, "foo");
        assert!(target.link);
    }
    #[test]
    fn test_parse_domain_object_target_bang_prefix_suppresses_link() {
        let target = parse_domain_object_target("!foo", Domain::Py);
        assert_eq!(target.name, "foo");
        assert_eq!(target.display, "foo");
        assert!(!target.link);
    }
    #[test]
    fn test_parse_domain_object_target_tilde_prefix_shortens_dotted_name() {
        let target = parse_domain_object_target("~pkg.mod.foo", Domain::Py);
        assert_eq!(target.name, "pkg.mod.foo");
        assert_eq!(target.display, "foo");
        assert!(target.link);
    }
    #[test]
    fn test_parse_domain_object_target_tilde_prefix_on_bare_name_is_a_no_op() {
        let target = parse_domain_object_target("~foo", Domain::Py);
        assert_eq!(target.name, "foo");
        assert_eq!(target.display, "foo");
        assert!(target.link);
        assert_eq!(target.search_order, TargetSearchOrder::LeastQualifiedFirst);
    }
    #[test]
    fn test_parse_domain_object_target_plain_name_searches_least_qualified_first() {
        // Given / When — no leading dot, Sphinx's default order.
        let target = parse_domain_object_target("open", Domain::Py);

        // Then
        assert_eq!(target.search_order, TargetSearchOrder::LeastQualifiedFirst);
    }
    #[test]
    fn test_parse_domain_object_target_dot_prefix_reverses_search_order() {
        // Given / When — the `:py:func:`.open`` form from the Sphinx docs.
        let target = parse_domain_object_target(".open", Domain::Py);

        // Then — the dot is markup: it drives the order and is shown to
        // nobody.
        assert_eq!(target.name, "open");
        assert_eq!(target.display, "open");
        assert!(target.link);
        assert_eq!(target.search_order, TargetSearchOrder::MostQualifiedFirst);
    }
    #[test]
    fn test_parse_domain_object_target_dot_prefix_keeps_inner_dots() {
        // Given / When — only the *leading* dot is a modifier; the ones
        // inside the name are what make it dotted at all.
        let target = parse_domain_object_target(".datetime.strptime", Domain::Py);

        // Then
        assert_eq!(target.name, "datetime.strptime");
        assert_eq!(target.display, "datetime.strptime");
        assert_eq!(target.search_order, TargetSearchOrder::MostQualifiedFirst);
    }
    #[test]
    fn test_parse_domain_object_target_strips_every_leading_dot() {
        // Given / When — degenerate input; an empty leading segment can
        // never match an index key, so no dot may survive.
        let target = parse_domain_object_target("..foo", Domain::Py);

        // Then
        assert_eq!(target.name, "foo");
        assert_eq!(target.display, "foo");
        assert_eq!(target.search_order, TargetSearchOrder::MostQualifiedFirst);
    }
    #[test]
    fn test_parse_domain_object_target_combines_tilde_and_dot_prefixes() {
        // Given / When — `~` shortens the display, `.` reverses the search
        // order, and they compose in that written order.
        let target = parse_domain_object_target("~.pkg.mod.foo", Domain::Py);

        // Then
        assert_eq!(target.name, "pkg.mod.foo");
        assert_eq!(target.display, "foo");
        assert!(target.link);
        assert_eq!(target.search_order, TargetSearchOrder::MostQualifiedFirst);
    }
    #[test]
    fn test_parse_domain_object_target_bang_prefix_shows_remaining_prefixes_verbatim() {
        // Given / When — real Sphinx turns a `!` target into a plain literal
        // before it ever looks at `~`/`.`, so those are just text here.
        let target = parse_domain_object_target("!.foo", Domain::Py);

        // Then
        assert_eq!(target.name, ".foo");
        assert_eq!(target.display, ".foo");
        assert!(!target.link);
        assert_eq!(target.search_order, TargetSearchOrder::LeastQualifiedFirst);
    }
    #[test]
    fn test_parse_domain_object_target_explicit_title_splits_display_from_target() {
        // Given / When — Sphinx's `Display text <target>` syntax.
        let target = parse_domain_object_target("spawn text <spawnl>", Domain::Py);

        // Then
        assert_eq!(target.name, "spawnl");
        assert_eq!(target.display, "spawn text");
        assert!(target.link);
    }
    #[test]
    fn test_parse_domain_object_target_explicit_title_unescapes_display() {
        // Given / When — the confirmed known_bugs.md example:
        // `:func:`spawn\* <spawnl>`` displays "spawn*", resolves "spawnl".
        // The role content arrives escaped, as it does from `parse_inline_text`.
        let target = parse_domain_object_target(&escaped(r"spawn\* <spawnl>"), Domain::Py);

        // Then
        assert_eq!(target.name, "spawnl");
        assert_eq!(target.display, "spawn*");
    }
    #[test]
    fn test_parse_domain_object_target_explicit_title_keeps_dotted_target_unshortened() {
        // Given / When — an explicit title is given, so `name`'s dots are
        // never used to shorten the display (there's nothing to shorten).
        let target = parse_domain_object_target("compat32 <email.policy.Compat32>", Domain::Py);

        // Then
        assert_eq!(target.name, "email.policy.Compat32");
        assert_eq!(target.display, "compat32");
    }
    #[test]
    fn test_parse_domain_object_target_explicit_title_overrides_tilde_shortening() {
        // Given / When — `~` still strips from the target for `name`, but
        // the explicit title always wins for `display`.
        let target = parse_domain_object_target("Custom <~pkg.mod.foo>", Domain::Py);

        // Then
        assert_eq!(target.name, "pkg.mod.foo");
        assert_eq!(target.display, "Custom");
        assert!(target.link);
    }
    #[test]
    fn test_strip_trailing_call_parens_strips_one_pair_in_the_c_domain() {
        // Given — `CPython`'s `c-api/object.rst` writes the reference as a
        // call: :c:func:`Py_TYPE()`.
        let name = "Py_TYPE()";

        // When
        let stripped = strip_trailing_call_parens(name, Domain::C);

        // Then — what is left is the name the `.. c:function::` declaration
        // registered.
        assert_eq!(stripped, "Py_TYPE");
    }
    #[test]
    fn test_strip_trailing_call_parens_strips_one_pair_in_the_py_domain() {
        // Given — `library/compileall.rst` writes
        // :py:func:`sys.getrecursionlimit()`; Sphinx's `PythonDomain.find_obj`
        // strips it too, so this is not a `c`-only rule.
        let name = "sys.getrecursionlimit()";

        // When
        let stripped = strip_trailing_call_parens(name, Domain::Py);

        // Then
        assert_eq!(stripped, "sys.getrecursionlimit");
    }
    #[test]
    fn test_strip_trailing_call_parens_leaves_a_paren_free_name_untouched() {
        // Given / When / Then — the overwhelmingly common case.
        assert_eq!(strip_trailing_call_parens("Py_TYPE", Domain::C), "Py_TYPE");
        assert_eq!(strip_trailing_call_parens("Py_TYPE", Domain::Py), "Py_TYPE");
    }
    #[test]
    fn test_strip_trailing_call_parens_tolerates_whitespace_only_in_the_c_domain() {
        // Given — the one place the two domains disagree: C parses the name
        // and then runs `skip_ws(); skip_string("()")`, while Python's
        // `removesuffix("()")` sees the space and gives up.
        let name = "Py_TYPE ()";

        // When / Then
        assert_eq!(strip_trailing_call_parens(name, Domain::C), "Py_TYPE");
        assert_eq!(strip_trailing_call_parens(name, Domain::Py), "Py_TYPE ()");
    }
    #[test]
    fn test_strip_trailing_call_parens_strips_at_most_one_pair() {
        // Given — degenerate input. Python strips once and misses; C strips
        // once and then fails `assert_end()`. Either way it stays
        // unresolvable, so leaving the inner pair in is the faithful answer.
        let name = "foo()()";

        // When / Then
        assert_eq!(strip_trailing_call_parens(name, Domain::C), "foo()");
        assert_eq!(strip_trailing_call_parens(name, Domain::Py), "foo()");
    }
    #[test]
    fn test_strip_trailing_call_parens_leaves_a_non_empty_argument_list_alone() {
        // Given — only *empty* parens are the "reads as a call" markup; a
        // real argument list is not a reference target in either domain.
        assert_eq!(strip_trailing_call_parens("foo(a)", Domain::C), "foo(a)");
        assert_eq!(strip_trailing_call_parens("foo(a)", Domain::Py), "foo(a)");
    }
    #[test]
    fn test_strip_trailing_call_parens_leaves_parens_that_are_not_a_suffix_alone() {
        // Given / When / Then — C's `assert_end()` rejects the trailing junk
        // and Python's `removesuffix` never matches, so neither strips here.
        assert_eq!(
            strip_trailing_call_parens("foo()bar", Domain::C),
            "foo()bar"
        );
        assert_eq!(
            strip_trailing_call_parens("foo()bar", Domain::Py),
            "foo()bar"
        );
    }
    #[test]
    fn test_strip_trailing_call_parens_keeps_a_target_that_is_only_parens() {
        // Given — stripping would leave an empty lookup name, and a
        // broken-link diagnostic naming `''` tells the author nothing. Both
        // domains refuse this target anyway, so only the wording differs.
        assert_eq!(strip_trailing_call_parens("()", Domain::C), "()");
        assert_eq!(strip_trailing_call_parens("()", Domain::Py), "()");
    }
    #[test]
    fn test_parse_domain_object_target_call_parens_leave_the_name_but_stay_in_the_display() {
        // Given / When — the bug's own reproducer: :c:func:`Py_TYPE()`
        // must reach the plain `.. c:function:: ... Py_TYPE(...)`
        // declaration.
        let target = parse_domain_object_target("Py_TYPE()", Domain::C);

        // Then — the parens are markup for the reader, not part of the key.
        assert_eq!(target.name, "Py_TYPE");
        assert_eq!(target.display, "Py_TYPE()");
        assert!(target.link);
    }
    #[test]
    fn test_parse_domain_object_target_call_parens_survive_tilde_shortening() {
        // Given / When — `~` shortens the *display*, which still has to end
        // in the parens the author wrote.
        let target = parse_domain_object_target("~pkg.mod.foo()", Domain::Py);

        // Then
        assert_eq!(target.name, "pkg.mod.foo");
        assert_eq!(target.display, "foo()");
    }
    #[test]
    fn test_parse_domain_object_target_call_parens_combine_with_a_dot_prefix() {
        // Given / When — the two sigils sit at opposite ends and are
        // independent: the dot picks the search order, the parens are
        // display-only.
        let target = parse_domain_object_target(".open()", Domain::Py);

        // Then
        assert_eq!(target.name, "open");
        assert_eq!(target.display, "open()");
        assert_eq!(target.search_order, TargetSearchOrder::MostQualifiedFirst);
    }
    #[test]
    fn test_parse_domain_object_target_bang_prefix_keeps_call_parens_in_the_name() {
        // Given / When — `!` short-circuits before any target processing in
        // Sphinx's `XRefRole.run`, and the target is never looked up, so
        // there is nothing to strip them *for*.
        let target = parse_domain_object_target("!foo()", Domain::Py);

        // Then
        assert_eq!(target.name, "foo()");
        assert_eq!(target.display, "foo()");
        assert!(!target.link);
    }
    #[test]
    fn test_parse_domain_object_target_explicit_title_keeps_its_own_parens_out_of_it() {
        // Given / When — the strip applies to the target inside the angle
        // brackets; the title is whatever the author wrote.
        let target = parse_domain_object_target("the type getter <Py_TYPE()>", Domain::C);

        // Then
        assert_eq!(target.name, "Py_TYPE");
        assert_eq!(target.display, "the type getter");
    }
    #[test]
    fn test_parse_domain_object_target_strips_call_parens_after_unescaping() {
        // Given / When — docutils un-escapes interpreted text before the role
        // ever sees it, so `foo\(\)` reaches Sphinx as `foo()` and strips
        // like any other call-shaped target.
        let target = parse_domain_object_target(&escaped(r"foo\(\)"), Domain::Py);

        // Then
        assert_eq!(target.name, "foo");
        assert_eq!(target.display, "foo()");
    }
}
