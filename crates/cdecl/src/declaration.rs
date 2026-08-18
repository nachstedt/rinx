/// One parsed C declaration: the declaration specifiers, the declarator that
/// binds the declared name, and the two trailing forms that appear in
/// documentation signatures (a default value and a member bitfield width).
///
/// Only ever one declarator: Sphinx's C domain accepts a single declaration
/// per directive argument, so the `int a, b;` multi-declarator form is out of
/// scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// Storage class, qualifiers and type specifiers, in source order — e.g.
    /// `["static", "const", "unsigned", "long"]`. Kept as plain strings
    /// because nothing downstream interprets them yet.
    pub specifiers: Vec<String>,
    /// The declarator, which is what actually carries the declared name.
    pub declarator: Declarator,
    /// The text after `=`, captured verbatim rather than parsed as an
    /// expression (`int x = 5` -> `Some("5")`).
    pub initializer: Option<String>,
    /// The width after `:` in a struct/union member (`int x : 3` ->
    /// `Some("3")`), likewise captured verbatim.
    pub bitfield_width: Option<String>,
}

impl Declaration {
    /// The declared name, or `None` for an abstract declarator (an unnamed
    /// parameter such as the `PyObject *` in `PyObject *(*unaryfunc)(PyObject *)`).
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.declarator.name()
    }
}

/// A C declarator, modelling the "declaration reflects use" structure: the
/// tree is built outside-in, so the declared name sits at the *innermost*
/// position and [`Declarator::name`] walks down to find it.
///
/// Grouping parentheses need no variant of their own — `(*f)` and `*f` differ
/// in the *shape* they produce (whether the pointer wraps the function or the
/// function wraps the pointer), which the tree already expresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declarator {
    /// The declared identifier itself.
    Name(String),
    /// No name at all — legal wherever only a type is needed, i.e. in
    /// parameter lists.
    Abstract,
    /// `*inner`, with any qualifiers that applied to the pointer itself
    /// (`char * const * x` -> the inner pointer carries `["const"]`).
    Pointer {
        qualifiers: Vec<String>,
        inner: Box<Declarator>,
    },
    /// `inner[size]`, with the size captured verbatim (`None` for `[]`).
    Array {
        inner: Box<Declarator>,
        size: Option<String>,
    },
    /// `inner(parameters)`, with `varargs` set when the list ended in `...`.
    Function {
        inner: Box<Declarator>,
        parameters: Vec<Declaration>,
        varargs: bool,
    },
}

impl Declarator {
    /// Walks to the innermost [`Declarator::Name`] — the declared name.
    ///
    /// This is the whole point of building the tree: for
    /// `int (*Py_tracefunc)(PyObject *obj)` the outermost node is a
    /// `Function`, inside it a `Pointer`, and only inside *that* the name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Name(name) => Some(name),
            Self::Abstract => None,
            Self::Pointer { inner, .. }
            | Self::Array { inner, .. }
            | Self::Function { inner, .. } => inner.name(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A declaration with no specifiers and no trailing forms, wrapping the
    /// given declarator — keeps the name-walking tests focused.
    fn declaration_of(declarator: Declarator) -> Declaration {
        Declaration {
            specifiers: Vec::new(),
            declarator,
            initializer: None,
            bitfield_width: None,
        }
    }

    fn name_of(text: &str) -> Declarator {
        Declarator::Name(text.to_string())
    }

    fn pointer_to(inner: Declarator) -> Declarator {
        Declarator::Pointer {
            qualifiers: Vec::new(),
            inner: Box::new(inner),
        }
    }

    #[test]
    fn test_name_returns_the_identifier_for_a_bare_name() {
        // Given
        let declarator = name_of("FILE");

        // When / Then
        assert_eq!(declarator.name(), Some("FILE"));
    }

    #[test]
    fn test_name_returns_none_for_an_abstract_declarator() {
        // Given — an unnamed parameter, e.g. the `PyObject *` in
        // `PyObject *(*unaryfunc)(PyObject *)`.
        let declarator = Declarator::Abstract;

        // When / Then
        assert_eq!(declarator.name(), None);
    }

    #[test]
    fn test_name_walks_through_a_pointer() {
        // Given — `int *x`
        let declarator = pointer_to(name_of("x"));

        // When / Then
        assert_eq!(declarator.name(), Some("x"));
    }

    #[test]
    fn test_name_walks_through_nested_pointers() {
        // Given — `int **x`
        let declarator = pointer_to(pointer_to(name_of("x")));

        // When / Then
        assert_eq!(declarator.name(), Some("x"));
    }

    #[test]
    fn test_name_walks_through_an_array() {
        // Given — `int x[10]`
        let declarator = Declarator::Array {
            inner: Box::new(name_of("x")),
            size: Some("10".to_string()),
        };

        // When / Then
        assert_eq!(declarator.name(), Some("x"));
    }

    #[test]
    fn test_name_walks_through_a_function() {
        // Given — `int f(void)`
        let declarator = Declarator::Function {
            inner: Box::new(name_of("f")),
            parameters: Vec::new(),
            varargs: false,
        };

        // When / Then
        assert_eq!(declarator.name(), Some("f"));
    }

    #[test]
    fn test_name_walks_through_a_function_pointer() {
        // Given — `int (*Py_tracefunc)(PyObject *obj)`: a function whose
        // inner declarator is a pointer to the name. This is exactly the
        // shape a "text before the first parenthesis" heuristic gets wrong.
        let declarator = Declarator::Function {
            inner: Box::new(pointer_to(name_of("Py_tracefunc"))),
            parameters: Vec::new(),
            varargs: false,
        };

        // When / Then
        assert_eq!(declarator.name(), Some("Py_tracefunc"));
    }

    #[test]
    fn test_name_returns_none_when_an_abstract_declarator_is_nested() {
        // Given — `(PyObject *)`, a pointer with no name inside it.
        let declarator = pointer_to(Declarator::Abstract);

        // When / Then
        assert_eq!(declarator.name(), None);
    }

    #[test]
    fn test_name_walks_through_a_deeply_nested_declarator() {
        // Given — `int (*(*x)[5])(void)`: function -> pointer -> array ->
        // pointer -> name.
        let declarator = Declarator::Function {
            inner: Box::new(pointer_to(Declarator::Array {
                inner: Box::new(pointer_to(name_of("x"))),
                size: Some("5".to_string()),
            })),
            parameters: Vec::new(),
            varargs: false,
        };

        // When / Then
        assert_eq!(declarator.name(), Some("x"));
    }

    #[test]
    fn test_declaration_name_delegates_to_its_declarator() {
        // Given
        let declaration = declaration_of(pointer_to(name_of("obj")));

        // When / Then
        assert_eq!(declaration.name(), Some("obj"));
    }

    #[test]
    fn test_declaration_name_is_none_for_an_abstract_declarator() {
        // Given
        let declaration = declaration_of(Declarator::Abstract);

        // When / Then
        assert_eq!(declaration.name(), None);
    }
}
