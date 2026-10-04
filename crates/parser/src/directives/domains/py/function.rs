use super::super::body::{ObjectOptions, parse_object_body};
use super::dispatch::read_module_option;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::options::OptionLine;
use crate::headings::Adornment;
use rinx_ast::{DomainObjectBody, NonEmptyVector};

/// Parses a `.. py:function::` body: its option block, then its content.
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
    let parsed = parse_object_body::<FunctionOptions>(
        "py:function",
        body_lines,
        adornment_order,
        diagnostics,
        ctx,
    );
    DomainObjectBody::PyFunction {
        flags: parsed.flags,
        module: parsed.options.module,
        signatures,
        is_decorator: forced_decorator,
        is_async: parsed.options.is_async,
        body: parsed.content,
    }
}

/// The options Sphinx's `PyFunction` takes beyond the object-description
/// flags: `:async:`, and the `:module:` every `py` object takes.
#[derive(Debug, Default, PartialEq, Eq)]
struct FunctionOptions {
    module: Option<String>,
    is_async: bool,
}

impl ObjectOptions for FunctionOptions {
    fn read(&mut self, line: &OptionLine) -> bool {
        match line.name.as_str() {
            "async" => {
                self.is_async = true;
                true
            }
            _ => read_module_option(line, &mut self.module),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::body::test_support::read_options;
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
            body,
            ..
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
    fn test_function_options_read_module_and_async() {
        // Given — `asyncio-task.rst` writes `:async:` on a `.. function::`.
        let body = [":module: asyncio", ":async:", "", "Sleeps."];

        // When
        let (options, _, unrecognized) = read_options::<FunctionOptions>(&body);

        // Then
        assert_eq!(
            options,
            FunctionOptions {
                module: Some("asyncio".to_string()),
                is_async: true,
            }
        );
        assert!(unrecognized.is_empty());
    }
    #[test]
    fn test_function_options_leave_a_method_only_option() {
        // Given
        let body = [":classmethod:"];

        // When
        let (options, _, unrecognized) = read_options::<FunctionOptions>(&body);

        // Then
        assert_eq!(options, FunctionOptions::default());
        assert_eq!(unrecognized, ["classmethod"]);
    }
    #[test]
    fn test_parse_py_function_reads_no_index_and_module_in_any_order() {
        // Given — the shape `ctypes.rst` writes, where `:noindex:` used to stop
        // the option scan and both lines leaked into the body as text.
        let input =
            ".. function:: prototype(address)\n   :noindex:\n   :module:\n\n   Returns a function.";

        // When
        let doc = parse("test.rst", input);

        // Then
        let Some(Node::Directive(Directive::DomainObject(
            object @ DomainObjectBody::PyFunction { module, body, .. },
        ))) = doc.nodes.first()
        else {
            panic!("Expected PyFunction, got {:?}", doc.nodes);
        };
        assert!(object.no_index());
        assert_eq!(module.as_deref(), Some(""));
        assert_eq!(body.len(), 1);
        assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
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
            body,
            ..
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
            body,
            ..
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
