//! Canonical prefix-label ordering for a domain object's `<dt>`
//! (`abstractmethod`/`async`/`classmethod`/`staticmethod`, `final`, the
//! decorator `@` prefix).

/// Canonical, deterministic prefix-label order for a domain object's `<dt>`
/// (e.g. `abstractmethod`/`async`/`classmethod`/`staticmethod` for
/// `py:method`, `final`/`class` for `py:class`) — independent of how the
/// author wrote the option flags.
pub(super) fn domain_object_prefix_labels(
    obj: &rusty_sphinx_ast::DomainObjectBody,
) -> Vec<&'static str> {
    match obj {
        rusty_sphinx_ast::DomainObjectBody::PyMethod {
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            ..
        } => [
            (*is_abstractmethod, "abstractmethod"),
            (*is_async, "async"),
            (*is_classmethod, "classmethod"),
            (*is_staticmethod, "staticmethod"),
        ]
        .into_iter()
        .filter_map(|(active, label)| active.then_some(label))
        .collect(),
        rusty_sphinx_ast::DomainObjectBody::PyClass { is_final, .. } => {
            class_like_prefix_labels(*is_final, "class")
        }
        rusty_sphinx_ast::DomainObjectBody::PyException { is_final, .. } => {
            class_like_prefix_labels(*is_final, "exception")
        }
        _ => Vec::new(),
    }
}

/// Whether this domain object is a `.. decorator::`/`.. decoratormethod::`
/// definition — real Sphinx prefixes such a signature with a literal `@`
/// (`desc_addname('@', '@')` in `PyDecoratorFunction`/`PyDecoratorMethod`),
/// distinct from the `<em class="property">` badges
/// [`domain_object_prefix_labels`] renders for flag options like
/// `classmethod`/`staticmethod`.
pub(super) fn is_decorator_signature(obj: &rusty_sphinx_ast::DomainObjectBody) -> bool {
    match obj {
        rusty_sphinx_ast::DomainObjectBody::PyFunction { is_decorator, .. }
        | rusty_sphinx_ast::DomainObjectBody::PyMethod { is_decorator, .. } => *is_decorator,
        _ => false,
    }
}

/// Shared prefix-label construction for `py:class`/`py:exception` — both
/// objects have the same `is_final` option and only differ in the trailing
/// label naming the object type (`"class"` vs `"exception"`).
pub(super) fn class_like_prefix_labels(
    is_final: bool,
    kind_label: &'static str,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if is_final {
        labels.push("final");
    }
    labels.push(kind_label);
    labels
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::NonEmptyVector;

    #[test]
    fn test_domain_object_prefix_labels_orders_method_flags_independent_of_input_order() {
        // Given — flags set in a different order than the canonical output order
        let obj = rusty_sphinx_ast::DomainObjectBody::PyMethod {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("run()".to_string()),
            is_classmethod: true,
            is_staticmethod: true,
            is_abstractmethod: true,
            is_async: true,
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert_eq!(
            labels,
            vec!["abstractmethod", "async", "classmethod", "staticmethod"]
        );
    }
    #[test]
    fn test_domain_object_prefix_labels_omits_inactive_method_flags() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyMethod {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("run()".to_string()),
            is_classmethod: false,
            is_staticmethod: true,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert_eq!(labels, vec!["staticmethod"]);
    }
    #[test]
    fn test_domain_object_prefix_labels_includes_final_before_class_label() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyClass {
            module: None,
            signatures: NonEmptyVector::single("Greeter".to_string()),
            is_final: true,
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert_eq!(labels, vec!["final", "class"]);
    }
    #[test]
    fn test_domain_object_prefix_labels_includes_final_before_exception_label() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyException {
            module: None,
            signatures: NonEmptyVector::single("GreeterError".to_string()),
            is_final: true,
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert_eq!(labels, vec!["final", "exception"]);
    }
    #[test]
    fn test_class_like_prefix_labels_omits_final_when_not_set() {
        // Given / When
        let labels = class_like_prefix_labels(false, "exception");

        // Then
        assert_eq!(labels, vec!["exception"]);
    }
    #[test]
    fn test_domain_object_prefix_labels_is_empty_for_object_types_without_flags() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyFunction {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("greet(name)".to_string()),
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert!(labels.is_empty());
    }
    #[test]
    fn test_is_decorator_signature_true_for_decorator_function() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyFunction {
            module: None,
            signatures: NonEmptyVector::single("classmethod".to_string()),
            is_decorator: true,
            body: vec![],
        };

        // When / Then
        assert!(is_decorator_signature(&obj));
    }
    #[test]
    fn test_is_decorator_signature_true_for_decoratormethod() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyMethod {
            module: None,
            signatures: NonEmptyVector::single("register(cls)".to_string()),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            is_decorator: true,
            body: vec![],
        };

        // When / Then
        assert!(is_decorator_signature(&obj));
    }
    #[test]
    fn test_is_decorator_signature_false_for_plain_function() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyFunction {
            module: None,
            signatures: NonEmptyVector::single("greet(name)".to_string()),
            is_decorator: false,
            body: vec![],
        };

        // When / Then
        assert!(!is_decorator_signature(&obj));
    }
    #[test]
    fn test_is_decorator_signature_false_for_object_types_without_the_flag() {
        // Given — e.g. `py:class`, which has no `is_decorator` field at all.
        let obj = rusty_sphinx_ast::DomainObjectBody::PyClass {
            module: None,
            signatures: NonEmptyVector::single("Greeter".to_string()),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert!(!is_decorator_signature(&obj));
    }
}
