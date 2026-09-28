//! `.. role:: name(code)` — defining a role for the rest of the document.
//!
//! Only a role derived from `code` can be defined: it is what gives inline
//! code a language to be highlighted as, which `:code:` alone never has. Any
//! other base, or none, is refused by name rather than half-supported.
//!
//! The directive contributes no node at all. What it defines is recorded in
//! the document's [`crate::custom_roles::CustomRoles`] table as it is parsed,
//! so a role applies from its definition onwards, as in docutils — and no
//! later phase has anything to learn about it.

use rinx_ast::{Diagnostic, DiagnosticCode, Node, ResolvedLanguage, Span};
use std::sync::LazyLock;

use regex::Regex;

use crate::context::ParseCtx;
use crate::custom_roles::CodeRole;
use crate::diagnostics::Diagnostics;
use crate::indent::unindent_body_lines;
use crate::inline::{is_fixed_role_name, is_writable_role_name};

use super::classes::normalize_class_name;
use super::options::{OptionLine, report_unknown_options, scan_option_lines};

const DIRECTIVE: &str = "role";

/// The one base role a custom role may derive from.
const CODE_BASE: &str = "code";

/// A `.. role::` argument as written: the new role's name, and the base role
/// in parentheses after it, if any.
#[derive(Debug, PartialEq, Eq)]
struct RoleArgument<'a> {
    name: &'a str,
    base: Option<&'a str>,
}

/// Splits `name(base)` or a bare `name`, allowing whitespace around the
/// parentheses as docutils does. `None` when the argument has neither shape.
fn split_role_argument(argument: &str) -> Option<RoleArgument<'_>> {
    static ARGUMENT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<name>[^\s()]+)\s*(?:\(\s*(?P<base>[^\s()]*)\s*\))?$").unwrap()
    });
    let caps = ARGUMENT.captures(argument.trim())?;
    Some(RoleArgument {
        name: caps.name("name")?.as_str(),
        base: caps.name("base").map(|base| base.as_str()),
    })
}

/// Parses a `.. role::`, defining the role in the document's table when it
/// can be. Always answers with no nodes: a definition renders to nothing.
pub(in crate::directives) fn parse_role(
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<Node> {
    let Some(name) = read_role_name(argument, directive_span, diagnostics, ctx) else {
        return Vec::new();
    };

    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, _) = scan_option_lines(&unindented_lines);
    let role = read_code_role(name, &option_lines, diagnostics, ctx);

    if let Some(roles) = ctx.custom_roles() {
        roles.define(name, role);
    }
    Vec::new()
}

/// The name the argument defines, once it is known to be one this build can
/// honour: writable as a role, not already taken, and derived from `code`.
/// Reports and answers `None` otherwise.
fn read_role_name<'a>(
    argument: &'a str,
    directive_span: Option<Span>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<&'a str> {
    let refuse = |code: DiagnosticCode, message: String, diagnostics: &mut Diagnostics| {
        diagnostics.push(Diagnostic::at(code, message, directive_span));
    };

    let Some(RoleArgument { name, base }) = split_role_argument(argument) else {
        refuse(
            DiagnosticCode::RoleInvalidArgument,
            format!(
                "{DIRECTIVE}: '{argument}' is not a role name, optionally followed by '(base)'"
            ),
            diagnostics,
        );
        return None;
    };
    if !is_writable_role_name(name) {
        refuse(
            DiagnosticCode::RoleInvalidArgument,
            format!(
                "{DIRECTIVE}: ':{name}:' cannot be written as a role; a name starts with a letter \
                 and holds only letters, digits, '-' and '_'"
            ),
            diagnostics,
        );
        return None;
    }
    // Both checks ignore case because a custom role's name does: an entity
    // role spelled `Req` would otherwise lose `:req:` to a `.. role:: req`.
    if is_fixed_role_name(&name.to_lowercase()) || ctx.schema.has_role_ignoring_case(name) {
        refuse(
            DiagnosticCode::RoleBuiltinName,
            format!(
                "{DIRECTIVE}: ':{name}:' is already a role, which a custom role cannot replace"
            ),
            diagnostics,
        );
        return None;
    }
    match base {
        Some(base) if base.eq_ignore_ascii_case(CODE_BASE) => Some(name),
        Some(base) => {
            refuse(
                DiagnosticCode::RoleUnsupportedBase,
                format!(
                    "{DIRECTIVE}: ':{name}:' cannot be derived from ':{base}:'; only \
                     '{CODE_BASE}' is supported as a base role"
                ),
                diagnostics,
            );
            None
        }
        None => {
            refuse(
                DiagnosticCode::RoleUnsupportedBase,
                format!(
                    "{DIRECTIVE}: ':{name}:' has no base role; only roles derived from \
                     '{CODE_BASE}', written '{name}({CODE_BASE})', are supported"
                ),
                diagnostics,
            );
            None
        }
    }
}

/// Reads a code role's `:language:` and `:class:`, reporting anything else.
///
/// Without a `:class:`, the role's own name is its class, as docutils gives
/// it; the language is added as a class while rendering, as Sphinx does.
fn read_code_role(
    name: &str,
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> CodeRole {
    let mut language = ResolvedLanguage::None;
    let mut classes = None;
    let mut unrecognized = Vec::new();

    for line in option_lines {
        match line.name.as_str() {
            "language" => {
                language = ResolvedLanguage::parse(&line.value).unwrap_or_else(|_| {
                    diagnostics.push(Diagnostic::at(
                        DiagnosticCode::RoleEmptyLanguage,
                        format!("{DIRECTIVE}: ':language:' names no language; ':{name}:' is left unhighlighted"),
                        ctx.line_span(line.line_index, &line.raw),
                    ));
                    ResolvedLanguage::None
                });
            }
            "class" => classes = Some(read_classes(line, diagnostics, ctx)),
            _ => unrecognized.push(line),
        }
    }
    report_unknown_options(
        &unrecognized,
        DIRECTIVE,
        DiagnosticCode::DirectiveUnknownOption,
        diagnostics,
        ctx,
    );

    CodeRole {
        language,
        classes: classes.unwrap_or_else(|| normalize_class_name(name).into_iter().collect()),
    }
}

/// Normalizes each name of a `:class:` value, reporting and dropping any that
/// normalizes to nothing.
fn read_classes(
    line: &OptionLine,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<String> {
    line.value
        .split_whitespace()
        .filter_map(|written| {
            let normalized = normalize_class_name(written);
            if normalized.is_none() {
                diagnostics.push(Diagnostic::at(
                    DiagnosticCode::RoleInvalidClass,
                    format!("{DIRECTIVE}: '{written}' cannot be made into a class name"),
                    ctx.line_span(line.line_index, &line.raw),
                ));
            }
            normalized
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom_roles::CustomRoles;
    use rinx_ast::Domain;
    use rinx_entity::{EntitySchema, NoReservedNames, load_schema};

    /// Parses a `.. role::` with `argument` and option `body` under a fresh
    /// table, returning what it defined and the diagnostic codes.
    fn define(argument: &str, body: &[&str]) -> (Vec<Node>, CustomRoles, Vec<DiagnosticCode>) {
        define_with_schema(argument, body, &EntitySchema::empty())
    }

    fn define_with_schema(
        argument: &str,
        body: &[&str],
        schema: &EntitySchema,
    ) -> (Vec<Node>, CustomRoles, Vec<DiagnosticCode>) {
        let roles = CustomRoles::default();
        let base = ParseCtx::with_domain(Domain::Py).with_schema(schema);
        let ctx = base.with_custom_roles(&roles);
        let mut diagnostics = Diagnostics::default();
        let nodes = parse_role(argument, None, body, &mut diagnostics, &ctx);
        let (entries, _, _) = diagnostics.into_parts();
        (nodes, roles, entries.iter().map(|d| d.code).collect())
    }

    fn python() -> ResolvedLanguage {
        ResolvedLanguage::parse("python").unwrap()
    }

    #[test]
    fn test_split_role_argument_reads_name_and_base() {
        // Given / When / Then
        assert_eq!(
            split_role_argument("python(code)"),
            Some(RoleArgument {
                name: "python",
                base: Some("code")
            })
        );
    }

    #[test]
    fn test_split_role_argument_allows_whitespace_around_the_base() {
        // Given / When / Then
        assert_eq!(
            split_role_argument("  python ( code ) "),
            Some(RoleArgument {
                name: "python",
                base: Some("code")
            })
        );
    }

    #[test]
    fn test_split_role_argument_reads_a_bare_name() {
        // Given / When / Then
        assert_eq!(
            split_role_argument("red"),
            Some(RoleArgument {
                name: "red",
                base: None
            })
        );
    }

    #[test]
    fn test_split_role_argument_refuses_other_shapes() {
        // Given / When / Then
        assert_eq!(split_role_argument(""), None);
        assert_eq!(split_role_argument("a b"), None);
        assert_eq!(split_role_argument("a(code"), None);
    }

    #[test]
    fn test_parse_role_defines_a_code_role_with_its_options() {
        // Given
        let body = ["   :language: Python", "   :class: Foo_Bar two"];

        // When
        let (nodes, roles, codes) = define("py(code)", &body);

        // Then — no node, and the role is in the table with normalized classes
        assert!(nodes.is_empty());
        assert!(codes.is_empty(), "{codes:?}");
        assert_eq!(
            roles.lookup("py"),
            Some(CodeRole {
                language: python(),
                classes: vec!["foo-bar".to_string(), "two".to_string()],
            })
        );
    }

    #[test]
    fn test_parse_role_gives_a_role_without_class_its_own_name() {
        // Given / When
        let (_, roles, codes) = define("Snippet(code)", &[]);

        // Then — unhighlighted, classed by its normalized name
        assert!(codes.is_empty(), "{codes:?}");
        assert_eq!(
            roles.lookup("snippet"),
            Some(CodeRole {
                language: ResolvedLanguage::None,
                classes: vec!["snippet".to_string()],
            })
        );
    }

    #[test]
    fn test_parse_role_accepts_the_base_in_any_case() {
        // Given / When
        let (_, roles, codes) = define("py(Code)", &[]);

        // Then
        assert!(codes.is_empty(), "{codes:?}");
        assert!(roles.lookup("py").is_some());
    }

    #[test]
    fn test_parse_role_refuses_an_unreadable_argument() {
        // Given / When
        let (_, roles, codes) = define("a b(code)", &[]);

        // Then
        assert_eq!(codes, vec![DiagnosticCode::RoleInvalidArgument]);
        assert!(roles.lookup("a").is_none());
    }

    #[test]
    fn test_parse_role_refuses_a_name_no_role_can_be_written_with() {
        // Given a name the inline scan could never match
        let (_, roles, codes) = define("my.role(code)", &[]);

        // Then
        assert_eq!(codes, vec![DiagnosticCode::RoleInvalidArgument]);
        assert!(roles.lookup("my.role").is_none());
    }

    #[test]
    fn test_parse_role_refuses_another_base() {
        // Given / When
        let (_, roles, codes) = define("strongish(strong)", &[]);

        // Then
        assert_eq!(codes, vec![DiagnosticCode::RoleUnsupportedBase]);
        assert!(roles.lookup("strongish").is_none());
    }

    #[test]
    fn test_parse_role_refuses_a_role_with_no_base() {
        // Given / When
        let (_, roles, codes) = define("red", &[]);

        // Then
        assert_eq!(codes, vec![DiagnosticCode::RoleUnsupportedBase]);
        assert!(roles.lookup("red").is_none());
    }

    #[test]
    fn test_parse_role_refuses_a_built_in_role_name() {
        // Given / When — `ref` and `code` are both matched before any custom role
        let (_, roles, ref_codes) = define("ref(code)", &[]);
        let (_, _, code_codes) = define("Code(code)", &[]);

        // Then
        assert_eq!(ref_codes, vec![DiagnosticCode::RoleBuiltinName]);
        assert_eq!(code_codes, vec![DiagnosticCode::RoleBuiltinName]);
        assert!(roles.lookup("ref").is_none());
    }

    #[test]
    fn test_parse_role_refuses_an_entity_role_name() {
        // Given a schema declaring `:req:`, and the built-in `:entity:`
        let schema = load_schema(
            r#"
            [[entity_type]]
            name = "req"

            [[role]]
            name = "req"
            types = ["req"]
            "#,
            &NoReservedNames,
        )
        .unwrap();

        // When
        let (_, _, req) = define_with_schema("REQ(code)", &[], &schema);
        let (_, _, entity) = define_with_schema("entity(code)", &[], &schema);

        // Then
        assert_eq!(req, vec![DiagnosticCode::RoleBuiltinName]);
        assert_eq!(entity, vec![DiagnosticCode::RoleBuiltinName]);
    }

    #[test]
    fn test_parse_role_refuses_a_name_differing_from_an_entity_role_only_in_case() {
        // Given a schema declaring its role with a capital, `:Req:`
        let schema = load_schema(
            r#"
            [[entity_type]]
            name = "req"

            [[role]]
            name = "Req"
            types = ["req"]
            "#,
            &NoReservedNames,
        )
        .unwrap();

        // When — a spelling neither exact nor lowercase lookup would find
        let (_, roles, codes) = define_with_schema("rEQ(code)", &[], &schema);

        // Then — `:req:` would otherwise have been code, `:Req:` an entity
        assert_eq!(codes, vec![DiagnosticCode::RoleBuiltinName]);
        assert!(roles.lookup("req").is_none());
    }

    #[test]
    fn test_parse_role_reports_an_empty_language_and_defines_it_unhighlighted() {
        // Given / When
        let (_, roles, codes) = define("py(code)", &["   :language:"]);

        // Then
        assert_eq!(codes, vec![DiagnosticCode::RoleEmptyLanguage]);
        assert_eq!(
            roles.lookup("py").map(|role| role.language),
            Some(ResolvedLanguage::None)
        );
    }

    #[test]
    fn test_parse_role_reports_and_drops_a_class_that_normalizes_to_nothing() {
        // Given / When
        let (_, roles, codes) = define("py(code)", &["   :class: 123 keep"]);

        // Then
        assert_eq!(codes, vec![DiagnosticCode::RoleInvalidClass]);
        assert_eq!(
            roles.lookup("py").map(|role| role.classes),
            Some(vec!["keep".to_string()])
        );
    }

    #[test]
    fn test_parse_role_reports_an_unknown_option() {
        // Given / When
        let (_, roles, codes) = define("py(code)", &["   :format: html"]);

        // Then — still defined
        assert_eq!(codes, vec![DiagnosticCode::DirectiveUnknownOption]);
        assert!(roles.lookup("py").is_some());
    }

    #[test]
    fn test_read_classes_normalizes_each_name() {
        // Given
        let line = OptionLine {
            name: "class".to_string(),
            value: "A_b  C".to_string(),
            raw: ":class: A_b  C".to_string(),
            line_index: 0,
        };
        let mut diagnostics = Diagnostics::default();

        // When
        let classes = read_classes(&line, &mut diagnostics, &ParseCtx::with_domain(Domain::Py));

        // Then
        assert_eq!(classes, vec!["a-b", "c"]);
    }
}
