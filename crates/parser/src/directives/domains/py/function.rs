use super::dispatch::parse_module_option_line;
use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rinx_ast::{DomainObjectBody, NonEmptyVector};

/// Parses a `.. py:function::` body: strips a leading `:module:` option line
/// off the front before parsing the rest as the docstring body — the only
/// option real Sphinx's `py:function` directive has (unlike `py:method`,
/// it has no body-option flags of its own).
///
/// `forced_decorator` comes from the legacy `.. decorator::` directive-name
/// alias (which is just `py:function` with `is_decorator` implied) — see
/// [`DomainObjectBody::PyFunction::is_decorator`]. Unlike
/// `forced_classmethod`/`forced_staticmethod` on [`parse_py_method`], there's
/// no explicit body-option spelling to OR it against: real Sphinx has no
/// `:decorator:` option, only the directive-name alias.
pub(crate) fn parse_py_function(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
    forced_decorator: bool,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (module, options_consumed) = extract_function_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, ctx);

    DomainObjectBody::PyFunction {
        module,
        signatures,
        is_decorator: forced_decorator,
        body,
    }
}

/// Extracts `.. py:function::`-specific options (`:module:`, the only one
/// real Sphinx's `py:function` directive has) from the leading lines of a
/// domain object's body.
///
/// Scans from the start and stops at the first line that isn't `:module:`
/// (e.g. a blank line or the start of the docstring body), returning how
/// many leading lines were consumed as options so the caller can slice them
/// off before parsing the remaining body content.
fn extract_function_options(lines: &[String]) -> (Option<String>, usize) {
    let mut module = None;
    let mut consumed = 0;

    for line in lines {
        let trimmed = line.trim();
        if let Some(value) = parse_module_option_line(trimmed) {
            module = Some(value);
        } else {
            break;
        }
        consumed += 1;
    }

    (module, consumed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{Directive, Node};

    #[test]
    fn test_parse_decorator_alias_directive_forces_is_decorator_flag() {
        // Given — the `docs/dev/known_bugs.md` #1 repro: `.. decorator::` under the
        // default `py` domain, a bare name (as CPython's
        // `Doc/reference/datamodel` writes `classmethod`/`staticmethod`)
        let input = ".. decorator:: classmethod\n\n   Transform a method into a class method.";

        // When
        let doc = parse("test.rst", input);

        // Then — it parses as a `py:function` with `is_decorator` forced on
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyFunction {
            signatures,
            is_decorator,
            module: _,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["classmethod"]);
            assert!(*is_decorator);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyFunction, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_explicit_py_decorator_directive_forces_is_decorator_flag() {
        // Given — the explicit `py:decorator` domain-prefixed spelling
        let input = ".. py:decorator:: coroutine\n\n   Mark a function as a coroutine.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyFunction {
            is_decorator,
            ..
        })) = &doc.nodes[0]
        {
            assert!(*is_decorator);
        } else {
            panic!("Expected PyFunction, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_plain_py_function_leaves_is_decorator_unset() {
        // Given — a negative case: an ordinary `.. py:function::` must not
        // pick up `is_decorator` just because the variant now has the field.
        let input = ".. py:function:: greet(name)\n\n   Greets the given name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyFunction {
            is_decorator,
            ..
        })) = &doc.nodes[0]
        {
            assert!(!is_decorator);
        } else {
            panic!("Expected PyFunction, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_extract_function_options_parses_module_option() {
        // Given — real Sphinx's `py:function` directive has no other options.
        let lines = vec![
            ":module: ctypes.util".to_string(),
            String::new(),
            "Finds a library.".to_string(),
        ];

        // When
        let (module, consumed) = extract_function_options(&lines);

        // Then
        assert_eq!(module.as_deref(), Some("ctypes.util"));
        assert_eq!(consumed, 1);
    }
    #[test]
    fn test_extract_function_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["Finds a library.".to_string()];

        // When
        let (module, consumed) = extract_function_options(&lines);

        // Then
        assert_eq!(module, None);
        assert_eq!(consumed, 0);
    }
    #[test]
    fn test_parse_creates_py_function_domain_object() {
        // Given
        let input = ".. py:function:: greet(name)\n\n   Greets the given name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyFunction {
            signatures,
            is_decorator,
            module: _,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["greet(name)"]);
            assert!(!is_decorator);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyFunction, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_function_collects_every_argument_line_as_a_signature() {
        // Given — multi-signature declarations are not limited to bare names;
        // each continuation line may be a full signature.
        let input = ".. py:function:: spawnl(mode, file, *args)\n                 spawnle(mode, file, *args, env)\n\n   Spawn a process.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyFunction {
            signatures,
            is_decorator,
            module: _,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(
                signatures.as_slice(),
                [
                    "spawnl(mode, file, *args)",
                    "spawnle(mode, file, *args, env)"
                ]
            );
            assert!(!is_decorator);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyFunction, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_function_ignores_module_only_options() {
        // Given — platform/synopsis/deprecated are py:module-only per Sphinx's
        // spec (and, since `PyFunction` has no such fields at all, it's a
        // compile error for a function to carry them) — so on a py:function
        // this text must remain part of the docstring body instead.
        let input = ".. py:function:: greet(name)\n\n   :platform: Unix";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyFunction {
            body, ..
        })) = &doc.nodes[0]
        {
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyFunction, got {:?}", doc.nodes[0]);
        }
    }
}
