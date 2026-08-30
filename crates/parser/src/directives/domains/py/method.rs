use super::dispatch::parse_module_option_line;
use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::directives::domains::object_type::DirectiveObjectType;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rusty_sphinx_ast::{DomainObjectBody, NonEmptyVector};

/// The `py:method`-alias directive-name flags that [`parse_py_method`]
/// forces on regardless of the body's own option lines — one field per
/// legacy alias (`.. classmethod::`, `.. staticmethod::`,
/// `.. decoratormethod::`), bundled into one type purely so the function
/// accepting them stays under clippy's argument-count lint; a plain
/// `.. py:method::` passes [`Self::NONE`].
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ForcedMethodFlags {
    classmethod: bool,
    staticmethod: bool,
    decorator: bool,
}

impl ForcedMethodFlags {
    const NONE: Self = Self {
        classmethod: false,
        staticmethod: false,
        decorator: false,
    };

    /// Derives the flags a `py:method`-family [`DirectiveObjectType`] forces
    /// on. Only ever called with one of the four variants this covers
    /// (enforced by [`parse_py_domain_object`]'s own match arm), so any other
    /// variant is unreachable here — kept as a `const fn` on this type,
    /// rather than inlined per call site, purely to keep
    /// `parse_py_domain_object` under clippy's line-count lint.
    pub(crate) fn for_directive(object_type: DirectiveObjectType) -> Self {
        match object_type {
            DirectiveObjectType::PyMethod => Self::NONE,
            DirectiveObjectType::PyClassmethod => Self {
                classmethod: true,
                ..Self::NONE
            },
            DirectiveObjectType::PyStaticmethod => Self {
                staticmethod: true,
                ..Self::NONE
            },
            DirectiveObjectType::PyDecoratorMethod => Self {
                decorator: true,
                ..Self::NONE
            },
            _ => unreachable!("for_directive called with a non-py-method-family object type"),
        }
    }
}

/// Parses a `.. py:method::` body: strips `:classmethod:`/`:staticmethod:`/
/// `:abstractmethod:`/`:async:` flag lines off the front before parsing the
/// rest as the docstring body.
///
/// `forced.classmethod`/`forced.staticmethod` come from the legacy
/// `.. classmethod::`/`.. staticmethod::` directive-name aliases (which are
/// just `py:method` with the matching flag implied); they are OR-ed with any
/// flag the body's own `:classmethod:`/`:staticmethod:` option lines set, so
/// the alias spelling and the explicit option spelling compose rather than
/// conflict. `forced.decorator` comes from `.. decoratormethod::` the same
/// way, but — like `py:function`'s `forced_decorator` in
/// [`parse_py_function`] — has no explicit `:decorator:` option to OR
/// against, since real Sphinx doesn't define one.
pub(crate) fn parse_py_method(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    ctx: &ParseCtx<'_>,
    forced: ForcedMethodFlags,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (is_classmethod, is_staticmethod, is_abstractmethod, is_async, module, options_consumed) =
        extract_method_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, ctx);

    DomainObjectBody::PyMethod {
        module,
        signatures,
        is_classmethod: is_classmethod || forced.classmethod,
        is_staticmethod: is_staticmethod || forced.staticmethod,
        is_abstractmethod,
        is_async,
        is_decorator: forced.decorator,
        body,
    }
}

/// Extracts `.. py:method::`-specific options: the flags `:classmethod:`,
/// `:staticmethod:`, `:abstractmethod:`, `:async:`, plus `:module:` (shared
/// with every other `py:*` object-description directive) from the leading
/// lines of a domain object's body.
///
/// Scans from the start and stops at the first line that isn't one of these
/// recognized options (e.g. a blank line or the start of the docstring body),
/// returning how many leading lines were consumed as options so the caller
/// can slice them off before parsing the remaining body content.
fn extract_method_options(lines: &[String]) -> (bool, bool, bool, bool, Option<String>, usize) {
    let mut is_classmethod = false;
    let mut is_staticmethod = false;
    let mut is_abstractmethod = false;
    let mut is_async = false;
    let mut module = None;
    let mut consumed = 0;

    for line in lines {
        let trimmed = line.trim();
        if let Some(module_value) = parse_module_option_line(trimmed) {
            module = Some(module_value);
        } else {
            match trimmed {
                ":classmethod:" => is_classmethod = true,
                ":staticmethod:" => is_staticmethod = true,
                ":abstractmethod:" => is_abstractmethod = true,
                ":async:" => is_async = true,
                _ => break,
            }
        }
        consumed += 1;
    }

    (
        is_classmethod,
        is_staticmethod,
        is_abstractmethod,
        is_async,
        module,
        consumed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::{Directive, Node};

    #[test]
    fn test_extract_method_options_parses_all_four_flags() {
        // Given
        let lines = vec![
            ":classmethod:".to_string(),
            ":staticmethod:".to_string(),
            ":abstractmethod:".to_string(),
            ":async:".to_string(),
            String::new(),
            "Does the thing.".to_string(),
        ];

        // When
        let (is_classmethod, is_staticmethod, is_abstractmethod, is_async, module, consumed) =
            extract_method_options(&lines);

        // Then
        assert!(is_classmethod);
        assert!(is_staticmethod);
        assert!(is_abstractmethod);
        assert!(is_async);
        assert_eq!(module, None);
        assert_eq!(consumed, 4);
    }
    #[test]
    fn test_extract_method_options_stops_at_first_non_option_line() {
        // Given
        let lines = vec![":classmethod:".to_string(), "Does the thing.".to_string()];

        // When
        let (is_classmethod, is_staticmethod, is_abstractmethod, is_async, module, consumed) =
            extract_method_options(&lines);

        // Then
        assert!(is_classmethod);
        assert!(!is_staticmethod);
        assert!(!is_abstractmethod);
        assert!(!is_async);
        assert_eq!(module, None);
        assert_eq!(consumed, 1);
    }
    #[test]
    fn test_extract_method_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["Does the thing.".to_string()];

        // When
        let (is_classmethod, is_staticmethod, is_abstractmethod, is_async, module, consumed) =
            extract_method_options(&lines);

        // Then
        assert!(!is_classmethod);
        assert!(!is_staticmethod);
        assert!(!is_abstractmethod);
        assert!(!is_async);
        assert_eq!(module, None);
        assert_eq!(consumed, 0);
    }
    #[test]
    fn test_extract_method_options_parses_module_option_alongside_flags() {
        // Given — `:module:` interleaved with the flag options, proving both
        // recognition and order-independence.
        let lines = vec![
            ":classmethod:".to_string(),
            ":module: multiprocessing.managers".to_string(),
            ":async:".to_string(),
            "Does the thing.".to_string(),
        ];

        // When
        let (is_classmethod, is_staticmethod, is_abstractmethod, is_async, module, consumed) =
            extract_method_options(&lines);

        // Then
        assert!(is_classmethod);
        assert!(!is_staticmethod);
        assert!(!is_abstractmethod);
        assert!(is_async);
        assert_eq!(module.as_deref(), Some("multiprocessing.managers"));
        assert_eq!(consumed, 3);
    }
    #[test]
    fn test_parse_creates_py_method_domain_object() {
        // Given
        let input = ".. py:method:: greet(self, name)\n\n   Greets the given name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod {
            signatures,
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            is_decorator,
            module: _,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["greet(self, name)"]);
            assert!(!is_classmethod);
            assert!(!is_staticmethod);
            assert!(!is_abstractmethod);
            assert!(!is_async);
            assert!(!is_decorator);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyMethod, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_method_with_classmethod_and_abstractmethod_options() {
        // Given
        let input = ".. py:method:: create(cls)\n   :classmethod:\n   :abstractmethod:\n\n   Creates an instance.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod {
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert!(*is_classmethod);
            assert!(!is_staticmethod);
            assert!(*is_abstractmethod);
            assert!(!is_async);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyMethod, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_method_options_in_any_order_with_no_body() {
        // Given — async before staticmethod, and no docstring body
        let input = ".. py:method:: run()\n   :async:\n   :staticmethod:";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod {
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert!(!is_classmethod);
            assert!(*is_staticmethod);
            assert!(!is_abstractmethod);
            assert!(*is_async);
            assert!(body.is_empty());
        } else {
            panic!("Expected PyMethod, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_classmethod_alias_directive_forces_classmethod_flag() {
        // Given — the legacy `.. classmethod::` directive spelling (the shape
        // CPython's `zoneinfo` docs use for `ZoneInfo.clear_cache`), a bare
        // name under the default `py` domain
        let input =
            ".. classmethod:: ZoneInfo.clear_cache(*, only_keys=None)\n\n   Clear the cache.";

        // When
        let doc = parse("test.rst", input);

        // Then — it parses as a `py:method` with `is_classmethod` forced on
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod {
            signatures,
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            is_decorator,
            module: _,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(
                signatures.as_slice(),
                ["ZoneInfo.clear_cache(*, only_keys=None)"]
            );
            assert!(*is_classmethod);
            assert!(!is_staticmethod);
            assert!(!is_abstractmethod);
            assert!(!is_async);
            assert!(!is_decorator);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyMethod, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_staticmethod_alias_directive_forces_staticmethod_flag() {
        // Given — the legacy `.. staticmethod::` directive spelling
        let input = ".. staticmethod:: Greeter.default_name()\n\n   The default name.";

        // When
        let doc = parse("test.rst", input);

        // Then — it parses as a `py:method` with `is_staticmethod` forced on
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod {
            signatures,
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            is_decorator,
            module: _,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["Greeter.default_name()"]);
            assert!(!is_classmethod);
            assert!(*is_staticmethod);
            assert!(!is_abstractmethod);
            assert!(!is_async);
            assert!(!is_decorator);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyMethod, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_classmethod_alias_composes_with_explicit_abstractmethod_option() {
        // Given — the alias directive name forces `classmethod`, and an
        // explicit `:abstractmethod:` option line in the body is still parsed
        // and OR-ed in on top of it
        let input = ".. classmethod:: validate(cls, name)\n   :abstractmethod:\n\n   Validate.";

        // When
        let doc = parse("test.rst", input);

        // Then — both flags are set
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod {
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            ..
        })) = &doc.nodes[0]
        {
            assert!(*is_classmethod);
            assert!(!is_staticmethod);
            assert!(*is_abstractmethod);
            assert!(!is_async);
        } else {
            panic!("Expected PyMethod, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_decoratormethod_alias_directive_forces_is_decorator_flag() {
        // Given — the `py:method` counterpart, nested inside a class the way
        // real decorator-producing methods are documented
        let input =
            ".. class:: Traits\n\n   .. decoratormethod:: register(cls)\n\n      Registers cls.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass {
            body, ..
        })) = &doc.nodes[0]
        {
            let method = body.iter().find_map(|node| match node {
                Node::Directive(Directive::DomainObject(
                    method @ DomainObjectBody::PyMethod { .. },
                )) => Some(method),
                _ => None,
            });
            if let Some(DomainObjectBody::PyMethod {
                signatures,
                is_classmethod,
                is_staticmethod,
                is_decorator,
                ..
            }) = method
            {
                assert_eq!(signatures.as_slice(), ["register(cls)"]);
                assert!(!is_classmethod);
                assert!(!is_staticmethod);
                assert!(*is_decorator);
            } else {
                panic!("Expected a nested PyMethod, got {method:?}");
            }
        } else {
            panic!("Expected PyClass, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_class_with_nested_py_method() {
        // Given — a `py:method` nested inside a `py:class` body, indented
        // like any other nested directive (e.g. `py:data` inside a table).
        let input = ".. py:class:: Greeter\n\n   A greeter.\n\n   .. py:method:: greet(self, name)\n\n      Greets the given name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass {
            body, ..
        })) = &doc.nodes[0]
        {
            assert!(body.iter().any(|node| matches!(
                node,
                Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod { .. }))
            )));
        } else {
            panic!("Expected PyClass, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_exception_with_nested_py_method() {
        // Given — a `py:method` nested inside a `py:exception` body, indented
        // like any other nested directive.
        let input = ".. py:exception:: GreeterError\n\n   Raised when greeting fails.\n\n   .. py:method:: reason(self)\n\n      Returns the failure reason.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyException {
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert!(body.iter().any(|node| matches!(
                node,
                Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod { .. }))
            )));
        } else {
            panic!("Expected PyException, got {:?}", doc.nodes[0]);
        }
    }
}
