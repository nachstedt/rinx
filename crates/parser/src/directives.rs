use super::admonitions::{parse_admonition, parse_seealso, parse_version_change};
use super::blocks::{
    collect_argument_continuation_lines, collect_directive_body, indent_width, join_body_lines,
    strip_common_indent,
};
use super::domains::parse_domain_object;
use super::glossary::parse_glossary;
use super::headings::Adornment;
use super::index_directive::parse_index_directive;
use super::list_table::parse_list_table;
use rusty_sphinx_ast::{Directive, Domain, Node};

/// The domain-object directive names the parser recognizes, resolved from a
/// directive's `domain:objtype` (or bare, default-domain-resolved) name.
/// Deliberately independent of `ast::ObjectType`: directive-name syntax
/// (including the legacy `classmethod`/`staticmethod` aliases, and
/// `decorator`/`decoratormethod`, none of which have an `ast::ObjectType` of
/// their own — they're just `py:function`/`py:method` with a flag forced) is
/// a parser concern, so it's modeled entirely here rather than borrowing the
/// shared object-type vocabulary the analyzer/renderer use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DirectiveObjectType {
    PyFunction,
    PyDecorator,
    PyModule,
    PyData,
    PyMethod,
    PyClassmethod,
    PyStaticmethod,
    PyDecoratorMethod,
    PyClass,
    PyAttribute,
    PyException,
    CFunction,
    CMacro,
    CStruct,
    CUnion,
    CMember,
    CType,
}

pub(super) fn parse_toctree(body_lines: &[&str], diagnostics: &mut Vec<String>) -> Directive {
    let mut paths = Vec::new();
    let mut maxdepth = None;
    let mut ignored_options = Vec::new();

    for l in body_lines {
        let line = l.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with(':') {
            let opt_name = line.split(':').nth(1).unwrap_or("");
            match opt_name {
                "maxdepth" => {
                    if let Some(rest) = line.strip_prefix(":maxdepth:")
                        && let Ok(depth) = rest.trim().parse::<usize>()
                    {
                        maxdepth = Some(depth);
                    }
                }
                "numbered" | "caption" | "name" | "titlesonly" | "glob" | "reversed" | "hidden"
                | "includehidden" => {
                    ignored_options.push(line.to_string());
                }
                _ => {
                    diagnostics.push(format!(
                        "Invalid or non-standard Sphinx toctree option encountered: {line}"
                    ));
                }
            }
            continue;
        }
        paths.push(line.to_string());
    }
    Directive::Toctree {
        paths,
        maxdepth,
        ignored_options,
    }
}

pub(super) fn try_parse_directive(
    lines: &[&str],
    i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Option<(usize, Node)> {
    let line = lines[i].trim_end();
    if !(line.trim().starts_with(".. ") && line.contains("::")) {
        return None;
    }

    let trimmed = line.trim();
    let (name_part, arg_part) = trimmed.split_once("::")?;
    let name = name_part.strip_prefix(".. ")?.trim().to_string();
    let argument = arg_part.trim().to_string();

    let min_indent = indent_width(line);

    // Checked before the generic body collection below, because a domain
    // object splits the lines after its marker differently: any further
    // argument lines are peeled off as extra signatures first, and only what
    // remains is its body. No other directive name can reach this branch —
    // `resolve_domain_object_type` matches a disjoint set of names from the
    // ones handled afterwards.
    if let Some(object_type) = resolve_domain_object_type(&name, default_domain) {
        let (continuations_consumed, continuations) =
            if object_type_supports_multiple_signatures(object_type) {
                collect_argument_continuation_lines(lines, i + 1, min_indent)
            } else {
                (0, Vec::new())
            };
        let (consumed_lines, body_lines) =
            collect_directive_body(lines, i + 1 + continuations_consumed, min_indent);
        let domain_object = parse_domain_object(
            object_type,
            argument,
            continuations,
            &body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        );
        return Some((
            1 + continuations_consumed + consumed_lines,
            Node::Directive(Directive::DomainObject(domain_object)),
        ));
    }

    let (consumed_lines, body_lines) = collect_directive_body(lines, i + 1, min_indent);

    if name == "toctree" {
        let directive = parse_toctree(&body_lines, diagnostics);
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "plantuml" {
        let directive = Directive::PlantUml(rusty_sphinx_ast::HashedContent::new(join_body_lines(
            &body_lines,
        )));
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "code-block" {
        let node = parse_code_block(argument, &body_lines);
        return Some((1 + consumed_lines, node));
    }
    if let Ok(kind) = name.parse::<rusty_sphinx_ast::VersionChangeKind>() {
        let directive = parse_version_change(
            kind,
            argument,
            &body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        );
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "seealso" {
        let directive = parse_seealso(&body_lines, adornment_order, diagnostics, default_domain);
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if let Ok(kind) = name.parse::<rusty_sphinx_ast::AdmonitionKind>() {
        let directive = parse_admonition(
            kind,
            argument,
            &body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        );
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "glossary" {
        let directive = parse_glossary(&body_lines, adornment_order, diagnostics, default_domain);
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "list-table" {
        let directive = parse_list_table(
            argument,
            &body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        );
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "index" {
        let directive = parse_index_directive(&argument, &body_lines, diagnostics);
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if let Some(directive) = try_parse_scope_directive(&name, &argument, default_domain) {
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    let directive = Directive::Unknown {
        name,
        argument,
        body: join_body_lines(&body_lines),
    };
    Some((1 + consumed_lines, Node::Directive(directive)))
}

/// Resolves a directive name to a [`DirectiveObjectType`]: either an explicit
/// `domain:objtype` form (e.g. `py:function`), or a bare `objtype` name
/// (e.g. `function`) resolved via `default_domain`. The `classmethod`/
/// `staticmethod`/`decorator`/`decoratormethod` legacy aliases are recognized
/// here too, and only in the `py` domain — the domain/bare-name split gates
/// them for free, so a bare `.. classmethod::` under a `c` default domain
/// resolves `domain` to `Domain::C`, matches no arm, and falls through to
/// `Directive::Unknown`. Real Sphinx's `PythonDomain` registers `decorator`/
/// `decoratormethod` the same `py`-only way (`PyDecoratorFunction`/
/// `PyDecoratorMethod`, both delegating to `py:function`/`py:method`), so
/// there's no `c:decorator` to reject specially — it simply matches no arm.
fn resolve_domain_object_type(name: &str, default_domain: Domain) -> Option<DirectiveObjectType> {
    let (domain, objtype_str) = split_domain_qualified_name(name, default_domain)?;
    match (domain, objtype_str) {
        (Domain::Py, "function") => Some(DirectiveObjectType::PyFunction),
        (Domain::Py, "decorator") => Some(DirectiveObjectType::PyDecorator),
        (Domain::Py, "module") => Some(DirectiveObjectType::PyModule),
        (Domain::Py, "data") => Some(DirectiveObjectType::PyData),
        (Domain::Py, "method") => Some(DirectiveObjectType::PyMethod),
        (Domain::Py, "classmethod") => Some(DirectiveObjectType::PyClassmethod),
        (Domain::Py, "staticmethod") => Some(DirectiveObjectType::PyStaticmethod),
        (Domain::Py, "decoratormethod") => Some(DirectiveObjectType::PyDecoratorMethod),
        (Domain::Py, "class") => Some(DirectiveObjectType::PyClass),
        (Domain::Py, "attribute") => Some(DirectiveObjectType::PyAttribute),
        (Domain::Py, "exception") => Some(DirectiveObjectType::PyException),
        (Domain::C, "function") => Some(DirectiveObjectType::CFunction),
        (Domain::C, "macro") => Some(DirectiveObjectType::CMacro),
        (Domain::C, "struct") => Some(DirectiveObjectType::CStruct),
        (Domain::C, "union") => Some(DirectiveObjectType::CUnion),
        // `.. c:var::` is a pure directive-name alias for `.. c:member::` in
        // real Sphinx (both register the same handler) — no forced-flag
        // distinction to carry, unlike `classmethod`/`staticmethod` above.
        (Domain::C, "member" | "var") => Some(DirectiveObjectType::CMember),
        (Domain::C, "type") => Some(DirectiveObjectType::CType),
        _ => None,
    }
}

/// Whether a directive of this object type may declare more than one
/// signature, as several argument lines below its marker.
///
/// Every object type may except `py:module`: real Sphinx's `module`
/// directive takes exactly one argument, so a line below it is body content
/// even when it looks like a further name.
const fn object_type_supports_multiple_signatures(object_type: DirectiveObjectType) -> bool {
    !matches!(object_type, DirectiveObjectType::PyModule)
}

/// Parses a `.. code-block::` directive's argument (the language, if any)
/// and body into a [`Node::LiteralBlock`], stripping the common leading
/// indentation from `body_lines` (collected by [`collect_directive_body`])
/// to preserve relative indentation within the block (RST spec behaviour).
fn parse_code_block(argument: String, body_lines: &[&str]) -> Node {
    let language = if argument.is_empty() {
        None
    } else {
        Some(argument)
    };
    let content = strip_common_indent(body_lines);
    Node::LiteralBlock { language, content }
}

/// Tries to parse `name`/`argument` as one of the *scope* directives: the
/// content-less, domain-qualified directives that document nothing and only
/// move the current scope the analyzer and renderer qualify against —
/// `.. currentmodule::` in the `py` domain, and the `.. c:namespace::`
/// family in the `c` domain.
///
/// They share one function because they share one dispatch rule
/// ([`split_domain_qualified_name`]), which is also what gates each to its
/// own domain: a bare `.. currentmodule::` under a `c` default domain, or a
/// bare `.. namespace::` under a `py` one, matches no arm and falls through
/// to [`Directive::Unknown`].
///
/// The hyphenated namespace names need no special handling:
/// `split_domain_qualified_name` only splits on `:`, so the hyphen rides
/// along in the bare name exactly as `code-block`/`list-table` already do.
///
/// A `namespace-push` with no scope argument is malformed — it returns
/// `None` so the caller falls through to `Directive::Unknown`, rather than
/// being silently accepted as a no-op push that a later `namespace-pop`
/// would then unbalance.
fn try_parse_scope_directive(
    name: &str,
    argument: &str,
    default_domain: Domain,
) -> Option<Directive> {
    match split_domain_qualified_name(name, default_domain) {
        Some((Domain::Py, "currentmodule")) => Some(Directive::PyCurrentModule {
            module: parse_current_module_argument(argument),
        }),
        Some((Domain::C, "namespace")) => Some(Directive::CNamespace {
            namespace: parse_c_namespace_argument(argument),
        }),
        Some((Domain::C, "namespace-push")) => {
            (!argument.is_empty()).then(|| Directive::CNamespacePush {
                namespace: argument.to_string(),
            })
        }
        // Any argument is ignored: real Sphinx's pop takes none, and undoes
        // the previous push whatever it was.
        Some((Domain::C, "namespace-pop")) => Some(Directive::CNamespacePop),
        _ => None,
    }
}

/// Parses a `.. c:namespace::` argument. `NULL` and `0` (real Sphinx's two
/// documented spellings for "reset to global scope") and an empty argument
/// all clear the scope; anything else becomes the new scope verbatim.
///
/// Deliberately distinct from [`parse_current_module_argument`]'s `None`
/// sentinel — the two domains spell their reset differently, and neither
/// should accept the other's spelling as magic.
fn parse_c_namespace_argument(argument: &str) -> Option<String> {
    if argument.is_empty() || argument == "NULL" || argument == "0" {
        None
    } else {
        Some(argument.to_string())
    }
}

/// Splits a directive name into its domain and bare name — either an
/// explicit `domain:name` form (e.g. `py:function`), or a bare name (e.g.
/// `function`) resolved via `default_domain`. Shared by
/// [`resolve_domain_object_type`] and [`try_parse_scope_directive`], so both
/// obey the same domain rules: a bare `.. currentmodule::` under a `c`
/// default domain, or an explicit `.. c:currentmodule::`, must not match the
/// `py`-only arm that consumes it, and likewise a bare `.. namespace::`
/// under a `py` default domain must not match the `c`-only arms.
fn split_domain_qualified_name(name: &str, default_domain: Domain) -> Option<(Domain, &str)> {
    match name.split_once(':') {
        Some((domain_str, rest)) => Some((domain_str.parse::<Domain>().ok()?, rest)),
        None => Some((default_domain, name)),
    }
}

/// Parses a `.. currentmodule::`/`.. py:currentmodule::` argument. `None`
/// (the reset form Sphinx uses to clear the current module, written
/// `.. currentmodule:: None`) and an empty argument both clear the module;
/// anything else becomes the new module name verbatim.
fn parse_current_module_argument(argument: &str) -> Option<String> {
    if argument.is_empty() || argument == "None" {
        None
    } else {
        Some(argument.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::HashedContent;

    #[test]
    fn test_parse_creates_admonition() {
        // Given
        let input = ".. note::\n\n   This is a note.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition {
            kind, title, body, ..
        }) = &doc.nodes[0]
        {
            assert_eq!(kind, &rusty_sphinx_ast::AdmonitionKind::Note);
            assert_eq!(title, &None);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_admonition_with_title() {
        // Given
        let input = ".. admonition:: My Title\n\n   Custom content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { kind, title, .. }) = &doc.nodes[0] {
            assert_eq!(kind, &rusty_sphinx_ast::AdmonitionKind::Admonition);
            assert_eq!(title, &Some("My Title".to_string()));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_collapsible_admonition() {
        // Given
        let input = ".. note::\n   :collapsible:\n\n   Content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { collapsible, .. }) = &doc.nodes[0] {
            assert_eq!(collapsible, &Some(false));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_collapsible_open_admonition() {
        // Given
        let input = ".. note::\n   :collapsible: open\n\n   Content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { collapsible, .. }) = &doc.nodes[0] {
            assert_eq!(collapsible, &Some(true));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_directive() {
        // Given
        let input = ".. toctree::\n   \n   team_a/index\n   team_b/index\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
                maxdepth: None,
                ignored_options: vec![],
            })
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                "Next Para".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_creates_directive_with_maxdepth() {
        // Given
        let input =
            ".. toctree::\n   :maxdepth: 2\n   \n   team_a/index\n   team_b/index\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
                maxdepth: Some(2),
                ignored_options: vec![],
            })
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                "Next Para".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_directive_with_argument_and_trailing_indents() {
        // Given
        let input = ".. code-block:: rust\n\n   let x = 1;\n   \n   let y = 2;\n\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::LiteralBlock {
                language: Some("rust".to_string()),
                content: "let x = 1;\n\nlet y = 2;".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_creates_index_directive() {
        // Given
        let input = ".. index:: single: execution\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        if let Node::Directive(Directive::Index { entries, .. }) = &doc.nodes[0] {
            assert_eq!(entries.len(), 1);
        } else {
            panic!("Expected Index directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_plantuml_directive_with_hash() {
        // Given
        let input = ".. plantuml::\n\n   A -> B\n   B -> C\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);

        let expected = HashedContent::new("A -> B\nB -> C".to_string());
        assert_eq!(doc.nodes[0], Node::Directive(Directive::PlantUml(expected)));
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                "Next Para".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_toctree_collects_diagnostic_for_invalid_option() {
        // Given
        let input = ".. toctree::\n   :invalid_opt:\n\n   foo";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.diagnostics.len(), 1);
        assert!(doc.diagnostics[0].contains("Invalid or non-standard Sphinx toctree option"));
        assert!(doc.diagnostics[0].contains(":invalid_opt:"));

        if let Node::Directive(Directive::Toctree {
            paths,
            ignored_options,
            ..
        }) = &doc.nodes[0]
        {
            assert_eq!(paths.len(), 1);
            assert_eq!(paths[0], "foo");
            assert!(ignored_options.is_empty());
        } else {
            panic!("Expected Toctree directive");
        }
    }

    #[test]
    fn test_parse_toctree_whitelists_standard_options() {
        let standard_options = vec![
            "numbered",
            "caption: My Caption",
            "name: myname",
            "titlesonly",
            "glob",
            "reversed",
            "hidden",
            "includehidden",
        ];

        for opt in standard_options {
            // Given
            let input = format!(".. toctree::\n   :{opt}:\n\n   foo");

            // When
            let doc = parse("test.rst", &input);

            // Then
            assert!(
                doc.diagnostics.is_empty(),
                "Option :{opt} generated a diagnostic!"
            );

            if let Node::Directive(Directive::Toctree {
                ignored_options, ..
            }) = &doc.nodes[0]
            {
                assert_eq!(ignored_options.len(), 1);
                assert_eq!(ignored_options[0], format!(":{opt}:"));
            } else {
                panic!("Expected Toctree directive");
            }
        }
    }

    #[test]
    fn test_parse_toctree_collects_paths() {
        // Given
        let body_lines = vec!["path1", "path2/index"];
        let mut diagnostics = vec![];
        // When
        let directive = parse_toctree(&body_lines, &mut diagnostics);
        // Then
        if let Directive::Toctree {
            paths,
            maxdepth,
            ignored_options,
        } = directive
        {
            assert_eq!(paths, vec!["path1", "path2/index"]);
            assert_eq!(maxdepth, None);
            assert!(ignored_options.is_empty());
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_toctree_parses_maxdepth_option() {
        // Given
        let body_lines = vec![":maxdepth: 2", "path1"];
        let mut diagnostics = vec![];
        // When
        let directive = parse_toctree(&body_lines, &mut diagnostics);
        // Then
        if let Directive::Toctree {
            paths,
            maxdepth,
            ignored_options,
        } = directive
        {
            assert_eq!(paths, vec!["path1"]);
            assert_eq!(maxdepth, Some(2));
            assert!(ignored_options.is_empty());
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_toctree_ignores_known_options() {
        // Given
        let body_lines = vec![":hidden:", ":caption: Some text", "path1"];
        let mut diagnostics = vec![];
        // When
        let directive = parse_toctree(&body_lines, &mut diagnostics);
        // Then
        if let Directive::Toctree {
            paths,
            maxdepth,
            ignored_options,
        } = directive
        {
            assert_eq!(paths, vec!["path1"]);
            assert_eq!(maxdepth, None);
            assert_eq!(ignored_options.len(), 2);
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_toctree_emits_diagnostic_for_unknown_option() {
        // Given
        let body_lines = vec![":unknown_opt:", "path1"];
        let mut diagnostics = vec![];
        // When
        let directive = parse_toctree(&body_lines, &mut diagnostics);
        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("Invalid or non-standard Sphinx toctree option"));
        if let Directive::Toctree { paths, .. } = directive {
            assert_eq!(paths, vec!["path1"]);
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_toctree_skips_blank_lines() {
        // Given
        let body_lines = vec!["path1", "  ", "", "path2"];
        let mut diagnostics = vec![];
        // When
        let directive = parse_toctree(&body_lines, &mut diagnostics);
        // Then
        if let Directive::Toctree { paths, .. } = directive {
            assert_eq!(paths, vec!["path1", "path2"]);
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_code_block_directive_with_language() {
        // Given
        let input = ".. code-block:: python

    x = 1
";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::LiteralBlock { language, content } = &doc.nodes[0] {
            assert_eq!(language.as_deref(), Some("python"));
            assert_eq!(content, "x = 1");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_code_block_directive_without_language() {
        // Given
        let input = ".. code-block::

    x = 1
";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::LiteralBlock { language, content } = &doc.nodes[0] {
            assert!(language.is_none());
            assert_eq!(content, "x = 1");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_prefix_ignores_default_domain() {
        // Given
        let name = "c:function";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CFunction));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_name_uses_default_domain() {
        // Given
        let name = "function";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CFunction));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_c_struct_resolves() {
        // Given
        let name = "c:struct";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CStruct));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_c_union_resolves() {
        // Given
        let name = "c:union";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CUnion));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_c_member_resolves() {
        // Given
        let name = "c:member";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CMember));
    }

    #[test]
    fn test_resolve_domain_object_type_c_var_resolves_as_member_alias() {
        // Given — `.. c:var::` is a pure directive-name alias for `c:member`.
        let name = "c:var";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CMember));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_c_type_resolves() {
        // Given
        let name = "c:type";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CType));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_type_uses_default_domain() {
        // Given
        let name = "type";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CType));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_member_uses_default_domain() {
        // Given
        let name = "member";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CMember));
    }

    #[test]
    fn test_resolve_domain_object_type_rejects_unknown_domain_prefix() {
        // Given
        let name = "rust:function";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_domain_object_type_rejects_unknown_object_type() {
        // Given
        let name = "py:struct";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_domain_object_type_bare_classmethod_resolves_in_py_domain() {
        // Given — a bare `classmethod` directive name under the `py` default
        // domain (the legacy `py:method` alias)
        let name = "classmethod";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyClassmethod));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_staticmethod_resolves_in_py_domain() {
        // Given
        let name = "staticmethod";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyStaticmethod));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_py_classmethod_resolves() {
        // Given — the explicit `py:classmethod` domain-prefixed form
        let name = "py:classmethod";

        // When — resolved even when the default domain is `c`
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyClassmethod));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_classmethod_rejected_in_c_domain() {
        // Given — a bare `classmethod` under a `c` default domain: the alias
        // is `py`-only, so this must not resolve
        let name = "classmethod";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_domain_object_type_bare_decorator_resolves_in_py_domain() {
        // Given — a bare `decorator` directive name under the `py` default
        // domain (the legacy `py:function` alias; see `known_bugs.md` #1)
        let name = "decorator";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyDecorator));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_decoratormethod_resolves_in_py_domain() {
        // Given
        let name = "decoratormethod";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyDecoratorMethod));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_py_decorator_resolves() {
        // Given — the explicit `py:decorator` domain-prefixed form
        let name = "py:decorator";

        // When — resolved even when the default domain is `c`
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyDecorator));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_decorator_rejected_in_c_domain() {
        // Given — a bare `decorator` under a `c` default domain: the alias is
        // `py`-only, so this must not resolve (real Sphinx defines no
        // `c:decorator`)
        let name = "decorator";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_creates_py_current_module_directive_from_bare_form() {
        // Given — the `py` domain is the parser's default, so the bare form
        // is what CPython's docs actually write.
        let input = ".. currentmodule:: enum";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::PyCurrentModule {
                module: Some("enum".to_string())
            })]
        );
    }

    #[test]
    fn test_parse_creates_py_current_module_directive_from_explicit_domain_form() {
        // Given
        let input = ".. py:currentmodule:: enum";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::PyCurrentModule {
                module: Some("enum".to_string())
            })]
        );
    }

    #[test]
    fn test_parse_current_module_none_argument_clears_module() {
        // Given
        let input = ".. currentmodule:: None";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::PyCurrentModule { module: None })]
        );
    }

    #[test]
    fn test_parse_bare_current_module_under_c_default_domain_is_unknown() {
        // Given — `currentmodule` is `py`-only; a bare directive under a `c`
        // default domain must not resolve to it.
        let input = ".. currentmodule:: enum";

        // When
        let doc = crate::parse_with_domain("test.rst", input, rusty_sphinx_ast::Domain::C);

        // Then
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::Unknown { name, .. }) if name == "currentmodule"
        ));
    }

    #[test]
    fn test_parse_explicit_c_current_module_is_unknown() {
        // Given — `c:currentmodule` names no real directive.
        let input = ".. c:currentmodule:: enum";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::Unknown { name, .. }) if name == "c:currentmodule"
        ));
    }

    #[test]
    fn test_parse_creates_c_namespace_directive_from_explicit_domain_form() {
        // Given
        let input = ".. c:namespace:: A.B";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::CNamespace {
                namespace: Some("A.B".to_string())
            })]
        );
    }

    #[test]
    fn test_parse_creates_c_namespace_directive_from_bare_form_under_c_default_domain() {
        // Given — a library whose `default_domain` is `c` writes it bare.
        let input = ".. namespace:: A.B";

        // When
        let doc = crate::parse_with_domain("test.rst", input, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::CNamespace {
                namespace: Some("A.B".to_string())
            })]
        );
    }

    #[test]
    fn test_parse_c_namespace_null_argument_resets_to_global_scope() {
        // Given — the spelling CPython's `c-api/memory.rst` actually uses.
        let input = ".. c:namespace:: NULL";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::CNamespace { namespace: None })]
        );
    }

    #[test]
    fn test_parse_c_namespace_zero_argument_resets_to_global_scope() {
        // Given — real Sphinx documents `0` as an alternative to `NULL`.
        let input = ".. c:namespace:: 0";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::CNamespace { namespace: None })]
        );
    }

    #[test]
    fn test_parse_creates_c_namespace_push_directive() {
        // Given
        let input = ".. c:namespace-push:: C.D";

        // When
        let doc = parse("test.rst", input);

        // Then — the hyphenated name resolves despite the domain split only
        // ever splitting on ':'.
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::CNamespacePush {
                namespace: "C.D".to_string()
            })]
        );
    }

    #[test]
    fn test_parse_creates_c_namespace_pop_directive() {
        // Given
        let input = ".. c:namespace-pop::";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes, vec![Node::Directive(Directive::CNamespacePop)]);
    }

    #[test]
    fn test_parse_c_namespace_push_without_argument_is_unknown() {
        // Given — a push with no scope is malformed; accepting it as a no-op
        // would leave a later `namespace-pop` unbalanced.
        let input = ".. c:namespace-push::";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::Unknown { name, .. }) if name == "c:namespace-push"
        ));
    }

    #[test]
    fn test_parse_bare_c_namespace_under_py_default_domain_is_unknown() {
        // Given — the namespace family is `c`-only; there is no
        // `py:namespace`, and `py` is the parser's default domain.
        let input = ".. namespace:: A.B";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::Unknown { name, .. }) if name == "namespace"
        ));
    }
}
