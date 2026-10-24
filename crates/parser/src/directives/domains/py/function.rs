use super::super::body::{DescriptionOption, ObjectOptions, parse_object_body};
use super::dispatch::PyObjectOption;
use crate::diagnostics::Diagnostics;
use crate::directives::invocation::DirectiveInvocation;
use crate::directives::options::{DirectiveOptions, directive_options};
use crate::directives::spec::{Arguments, DirectiveSpec};
use crate::headings::Adornment;
use rinx_ast::{DescriptionFlags, DomainObjectBody, NonEmptyVector};

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
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    forced_decorator: bool,
) -> DomainObjectBody {
    let parsed = parse_object_body::<FunctionOptions>(
        "py:function",
        invocation,
        adornment_order,
        diagnostics,
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

directive_options! {
    /// The options Sphinx's `PyFunction` takes.
    enum FunctionOption {
        /// `:async:`.
        Async => (["async"], Flag, "Marks the function as a coroutine."),
        _ => (["canonical"], Text, "The object's name where it is defined, if documented elsewhere."),;
        Py(PyObjectOption),
        Description(DescriptionOption),
    }
}

impl ObjectOptions for FunctionOptions {
    type Option = FunctionOption;

    fn read(&mut self, option: FunctionOption, flags: &mut DescriptionFlags) {
        match option {
            FunctionOption::Async => self.is_async = true,
            FunctionOption::Py(PyObjectOption::Module(module)) => self.module = Some(module),
            FunctionOption::Description(option) => flags.set(option.flag()),
        }
    }
}

/// `.. py:function::`'s entry among the domain objects.
pub(in crate::directives) const FUNCTION_SPEC: DirectiveSpec = DirectiveSpec {
    names: &["function"],
    doc: "Describes a Python function.",
    arguments: Arguments::ONE_TEXT,
    takes_content: true,
    content_hint: None,
    options: <FunctionOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

/// `.. py:decorator::`'s entry among the domain objects.
pub(in crate::directives) const DECORATOR_SPEC: DirectiveSpec = DirectiveSpec {
    names: &["decorator"],
    doc: "Describes a Python function used as a decorator.",
    arguments: Arguments::ONE_TEXT,
    takes_content: true,
    content_hint: None,
    options: <FunctionOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

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
        if let Node::Directive(Directive::DomainObject {
            object:
                DomainObjectBody::PyFunction {
                    signatures,
                    is_decorator,
                    body,
                    ..
                },
            ..
        }) = &doc.nodes[0]
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
        if let Node::Directive(Directive::DomainObject {
            object: DomainObjectBody::PyFunction { is_decorator, .. },
            ..
        }) = &doc.nodes[0]
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
        if let Node::Directive(Directive::DomainObject {
            object: DomainObjectBody::PyFunction { is_decorator, .. },
            ..
        }) = &doc.nodes[0]
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
        let Some(Node::Directive(Directive::DomainObject {
            object: object @ DomainObjectBody::PyFunction { module, body, .. },
            ..
        })) = doc.nodes.first()
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
        if let Node::Directive(Directive::DomainObject {
            object:
                DomainObjectBody::PyFunction {
                    signatures,
                    is_decorator,
                    body,
                    ..
                },
            ..
        }) = &doc.nodes[0]
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
        if let Node::Directive(Directive::DomainObject {
            object:
                DomainObjectBody::PyFunction {
                    signatures,
                    is_decorator,
                    body,
                    ..
                },
            ..
        }) = &doc.nodes[0]
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
        if let Node::Directive(Directive::DomainObject {
            object: DomainObjectBody::PyFunction { body, .. },
            ..
        }) = &doc.nodes[0]
        {
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyFunction, got {:?}", doc.nodes[0]);
        }
    }
}
