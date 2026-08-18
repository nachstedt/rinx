use super::declaration::{Declaration, Declarator};
use super::token::{Token, TokenKind, tokenize};

/// How deeply declarators may nest before the parser gives up.
///
/// Grouped declarators recurse, so without a bound a pathological input like
/// `int ((((((…x))))))` would exhaust the stack. Real declarations nest a
/// handful of levels at most; this is far above anything a documentation
/// signature contains, and turns a would-be crash into an ordinary
/// [`ParseError`].
const MAX_DECLARATOR_DEPTH: usize = 64;

/// Keywords that may appear among a declaration's specifiers.
///
/// Used to decide whether an identifier is definitely part of the type (a
/// keyword always is) or possibly the declared name — see
/// [`Parser::parse_specifiers`] for the rule applied to non-keyword
/// identifiers.
const SPECIFIER_KEYWORDS: &[&str] = &[
    "auto",
    "char",
    "const",
    "double",
    "enum",
    "extern",
    "float",
    "inline",
    "int",
    "long",
    "register",
    "restrict",
    "short",
    "signed",
    "static",
    "struct",
    "typedef",
    "union",
    "unsigned",
    "void",
    "volatile",
    "_Atomic",
    "_Bool",
    "_Complex",
    "_Noreturn",
    "_Thread_local",
];

/// Qualifiers that may follow a `*` and bind to the pointer itself, as in
/// `char * const * x`.
const POINTER_QUALIFIERS: &[&str] = &["const", "volatile", "restrict", "_Atomic"];

/// Identifiers that introduce a parenthesised attribute to be skipped whole.
const ATTRIBUTE_KEYWORDS: &[&str] = &["__attribute__", "__declspec"];

/// Why a declaration could not be parsed, and where in the input it went
/// wrong.
///
/// Callers are expected to treat this as "fall back to something cruder",
/// not as a fatal error: documentation signatures are frequently abbreviated
/// rather than compilable, and losing a cross-reference target is worse than
/// deriving its name imprecisely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    /// Byte offset into the input where parsing stopped.
    pub offset: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} at offset {}", self.message, self.offset)
    }
}

impl std::error::Error for ParseError {}

/// Parses a C declaration, as written in a Sphinx C-domain directive
/// argument.
///
/// Never panics and always terminates, whatever the input: recursion is
/// bounded by [`MAX_DECLARATOR_DEPTH`] and every loop either consumes a token
/// or breaks.
///
/// # Errors
///
/// Returns [`ParseError`] if the input is empty, if a construct is
/// unterminated (`int x[`), if declarators nest beyond the depth bound, or if
/// tokens remain once the declaration has been consumed (`int x ??`).
pub fn parse_declaration(text: &str) -> Result<Declaration, ParseError> {
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return Err(ParseError {
            message: "empty declaration".to_string(),
            offset: 0,
        });
    }

    let mut parser = Parser {
        input: text,
        tokens,
        position: 0,
        depth: 0,
    };
    let declaration = parser.parse_declaration()?;
    if let Some(token) = parser.peek() {
        return Err(ParseError {
            message: format!("unexpected '{}' after declaration", token.text),
            offset: token.offset,
        });
    }
    Ok(declaration)
}

/// Parses a declaration and returns just the declared name — the one thing
/// callers need in order to register a cross-reference target.
///
/// # Errors
///
/// Returns [`ParseError`] if the declaration does not parse, or if it parses
/// but declares no name (`int`, or a lone abstract declarator).
pub fn declared_name(text: &str) -> Result<String, ParseError> {
    let declaration = parse_declaration(text)?;
    declaration
        .name()
        .map(ToString::to_string)
        .ok_or_else(|| ParseError {
            message: "declaration declares no name".to_string(),
            offset: 0,
        })
}

/// Recursive-descent parser state: the token stream plus a cursor and the
/// current declarator nesting depth.
///
/// Holds the original `input` so opaque spans (array sizes, initialisers) can
/// be captured as exact source slices rather than reassembled from tokens.
struct Parser<'a> {
    input: &'a str,
    tokens: Vec<Token>,
    position: usize,
    depth: usize,
}

impl Parser<'_> {
    /// The token at the cursor, if any.
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    /// The token `ahead` positions past the cursor, if any.
    fn peek_ahead(&self, ahead: usize) -> Option<&Token> {
        self.tokens.get(self.position + ahead)
    }

    /// Advances the cursor past one token.
    fn advance(&mut self) {
        self.position += 1;
    }

    /// Whether the token at the cursor is the given punctuation.
    fn at_punctuation(&self, text: &str) -> bool {
        self.peek().is_some_and(|token| token.is_punctuation(text))
    }

    /// Consumes the given punctuation if present, reporting whether it was.
    fn consume_punctuation(&mut self, text: &str) -> bool {
        if self.at_punctuation(text) {
            self.advance();
            return true;
        }
        false
    }

    /// Consumes the given punctuation or fails with a pointed error.
    fn expect_punctuation(&mut self, text: &str) -> Result<(), ParseError> {
        if self.consume_punctuation(text) {
            return Ok(());
        }
        Err(self.error(&format!("expected '{text}'")))
    }

    /// Builds a [`ParseError`] pointing at the cursor, or at the end of the
    /// input if the cursor has run off the end.
    fn error(&self, message: &str) -> ParseError {
        let offset = self
            .peek()
            .map_or_else(|| self.input.len(), |token| token.offset);
        ParseError {
            message: message.to_string(),
            offset,
        }
    }

    /// Parses one declaration: specifiers, a declarator, and the trailing
    /// bitfield width or initialiser if present.
    ///
    /// Used both for the top-level declaration and, recursively, for each
    /// parameter — a parameter *is* a declaration, just usually an unnamed
    /// one.
    fn parse_declaration(&mut self) -> Result<Declaration, ParseError> {
        let specifiers = self.parse_specifiers();
        let declarator = self.parse_declarator()?;
        let bitfield_width = if self.consume_punctuation(":") {
            Some(self.capture_span_until(&[",", ")"]))
        } else {
            None
        };
        let initializer = if self.consume_punctuation("=") {
            Some(self.capture_span_until(&[",", ")"]))
        } else {
            None
        };
        Ok(Declaration {
            specifiers,
            declarator,
            initializer,
            bitfield_width,
        })
    }

    /// Consumes the declaration specifiers preceding the declarator.
    ///
    /// Keywords are unambiguously specifiers. A non-keyword identifier is the
    /// hard case — it is a type name in `Py_ssize_t ob_refcnt` but the
    /// declared name in `FILE` — resolved by looking at what follows it (see
    /// [`Self::identifier_continues_specifiers`]).
    fn parse_specifiers(&mut self) -> Vec<String> {
        let mut specifiers = Vec::new();
        while let Some(token) = self.peek() {
            if token.kind != TokenKind::Identifier {
                break;
            }
            if ATTRIBUTE_KEYWORDS.contains(&token.text.as_str()) {
                self.skip_attribute();
                continue;
            }
            let is_keyword = SPECIFIER_KEYWORDS.contains(&token.text.as_str());
            if !is_keyword && !self.identifier_continues_specifiers() {
                break;
            }
            specifiers.push(token.text.clone());
            self.advance();
        }
        specifiers
    }

    /// Whether the non-keyword identifier at the cursor is a type name rather
    /// than the declared name, judged by what follows it.
    ///
    /// A following identifier (`Py_ssize_t ob_refcnt`) or `*`
    /// (`PyObject *self`) means a declarator is still to come, so the current
    /// identifier belongs to the type. A following `(` is ambiguous: in
    /// `Py_ssize_t (*lenfunc)(…)` it opens a *grouped declarator* and the
    /// identifier is a type, while in `MAX(a, b)` it opens a *parameter list*
    /// and the identifier is the name. What is inside the parenthesis
    /// separates them — a group always begins with `*` or another group.
    fn identifier_continues_specifiers(&self) -> bool {
        match self.peek_ahead(1) {
            Some(next) if next.kind == TokenKind::Identifier => true,
            Some(next) if next.is_punctuation("*") => true,
            Some(next) if next.is_punctuation("(") => self
                .peek_ahead(2)
                .is_some_and(|after| after.is_punctuation("*") || after.is_punctuation("(")),
            _ => false,
        }
    }

    /// Skips an attribute and its balanced parenthesis group, e.g.
    /// `__attribute__((noreturn))`, which carries no information this crate
    /// models.
    fn skip_attribute(&mut self) {
        self.advance();
        if !self.at_punctuation("(") {
            return;
        }
        let mut depth = 0usize;
        while let Some(token) = self.peek() {
            if token.is_punctuation("(") {
                depth += 1;
            } else if token.is_punctuation(")") {
                depth -= 1;
                if depth == 0 {
                    self.advance();
                    return;
                }
            }
            self.advance();
        }
    }

    /// Parses a declarator: any number of leading `*`s wrapping a direct
    /// declarator.
    ///
    /// The pointers are applied *outermost-first* as the recursion unwinds,
    /// which is what puts the declared name at the innermost position.
    fn parse_declarator(&mut self) -> Result<Declarator, ParseError> {
        if !self.at_punctuation("*") {
            return self.parse_direct_declarator();
        }
        self.advance();
        let qualifiers = self.parse_pointer_qualifiers();
        let inner = self.recurse(Self::parse_declarator)?;
        Ok(Declarator::Pointer {
            qualifiers,
            inner: Box::new(inner),
        })
    }

    /// Consumes qualifiers binding to the pointer just parsed, as the `const`
    /// in `char * const * x`.
    fn parse_pointer_qualifiers(&mut self) -> Vec<String> {
        let mut qualifiers = Vec::new();
        while let Some(token) = self.peek() {
            if token.kind != TokenKind::Identifier
                || !POINTER_QUALIFIERS.contains(&token.text.as_str())
            {
                break;
            }
            qualifiers.push(token.text.clone());
            self.advance();
        }
        qualifiers
    }

    /// Parses a direct declarator: a name, a parenthesised group, or nothing
    /// at all (an abstract declarator), followed by any number of array and
    /// function suffixes.
    fn parse_direct_declarator(&mut self) -> Result<Declarator, ParseError> {
        let mut declarator = self.parse_declarator_base()?;
        loop {
            if self.at_punctuation("(") {
                declarator = self.parse_function_suffix(declarator)?;
            } else if self.at_punctuation("[") {
                declarator = self.parse_array_suffix(declarator)?;
            } else {
                break;
            }
        }
        Ok(declarator)
    }

    /// Parses what a direct declarator is built around, before any suffixes:
    /// a grouped declarator, an identifier, or nothing.
    fn parse_declarator_base(&mut self) -> Result<Declarator, ParseError> {
        if self.at_grouped_declarator() {
            self.advance();
            let inner = self.recurse(Self::parse_declarator)?;
            self.expect_punctuation(")")?;
            return Ok(inner);
        }
        if let Some(token) = self.peek()
            && token.kind == TokenKind::Identifier
        {
            let name = token.text.clone();
            self.advance();
            return Ok(Declarator::Name(name));
        }
        Ok(Declarator::Abstract)
    }

    /// Whether the cursor sits on a `(` that opens a grouped declarator
    /// rather than a parameter list.
    ///
    /// Same discriminator as in [`Self::identifier_continues_specifiers`]: a
    /// group starts with a pointer or a further group, so `(*f)` groups while
    /// `(int a)` is a parameter list.
    fn at_grouped_declarator(&self) -> bool {
        self.at_punctuation("(")
            && self
                .peek_ahead(1)
                .is_some_and(|next| next.is_punctuation("*") || next.is_punctuation("("))
    }

    /// Parses `(parameters)` applied to `inner`.
    fn parse_function_suffix(&mut self, inner: Declarator) -> Result<Declarator, ParseError> {
        self.advance();
        let (parameters, varargs) = self.parse_parameter_list()?;
        self.expect_punctuation(")")?;
        Ok(Declarator::Function {
            inner: Box::new(inner),
            parameters,
            varargs,
        })
    }

    /// Parses `[size]` applied to `inner`, capturing the size verbatim.
    fn parse_array_suffix(&mut self, inner: Declarator) -> Result<Declarator, ParseError> {
        self.advance();
        let size = self.capture_span_until(&["]"]);
        self.expect_punctuation("]")?;
        Ok(Declarator::Array {
            inner: Box::new(inner),
            size: (!size.is_empty()).then_some(size),
        })
    }

    /// Parses a comma-separated parameter list, up to but not including the
    /// closing `)`.
    ///
    /// `(void)` is normalised to an empty list, matching what it means in C,
    /// so `parameters.len()` is the arity a reader would expect.
    fn parse_parameter_list(&mut self) -> Result<(Vec<Declaration>, bool), ParseError> {
        let mut parameters = Vec::new();
        let mut varargs = false;
        if self.at_punctuation(")") {
            return Ok((parameters, varargs));
        }
        loop {
            if self.consume_punctuation("...") {
                varargs = true;
                break;
            }
            parameters.push(self.recurse(Self::parse_declaration)?);
            if !self.consume_punctuation(",") {
                break;
            }
        }
        if is_void_parameter_list(&parameters) {
            parameters.clear();
        }
        Ok((parameters, varargs))
    }

    /// Captures the source text from the cursor up to (not including) the
    /// first of `terminators` seen outside any nested brackets.
    ///
    /// Array sizes and initialisers are kept as opaque spans rather than
    /// parsed as expressions — nothing downstream evaluates them, and a real
    /// expression grammar would be a large amount of parser for no reader.
    fn capture_span_until(&mut self, terminators: &[&str]) -> String {
        let start = self.position;
        let mut depth = 0usize;
        while let Some(token) = self.peek() {
            if depth == 0 && terminators.iter().any(|text| token.is_punctuation(text)) {
                break;
            }
            if token.is_punctuation("(") || token.is_punctuation("[") {
                depth += 1;
            } else if token.is_punctuation(")") || token.is_punctuation("]") {
                depth -= 1;
            }
            self.advance();
        }
        self.source_between(start, self.position)
    }

    /// The original source text spanned by `tokens[start..end]`.
    fn source_between(&self, start: usize, end: usize) -> String {
        if start >= end {
            return String::new();
        }
        let first = &self.tokens[start];
        let last = &self.tokens[end - 1];
        self.input[first.offset..last.offset + last.text.len()]
            .trim()
            .to_string()
    }

    /// Runs `production` one level deeper, failing rather than recursing past
    /// [`MAX_DECLARATOR_DEPTH`].
    fn recurse<T>(
        &mut self,
        production: fn(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        if self.depth >= MAX_DECLARATOR_DEPTH {
            return Err(self.error("declarator nested too deeply"));
        }
        self.depth += 1;
        let result = production(self);
        self.depth -= 1;
        result
    }
}

/// Whether a parsed parameter list is the single unnamed `void` that means
/// "no parameters".
fn is_void_parameter_list(parameters: &[Declaration]) -> bool {
    parameters.len() == 1
        && parameters[0].declarator == Declarator::Abstract
        && parameters[0].specifiers == ["void"]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The declared name, for the many tests that only care about that.
    fn name_of(signature: &str) -> String {
        declared_name(signature).unwrap_or_else(|error| panic!("{signature:?}: {error}"))
    }

    // ── Specifiers and plain declarations ────────────────────────────────

    #[test]
    fn test_parses_a_bare_identifier_as_the_name() {
        // Given — `.. c:type:: FILE`, a name with no type at all.
        let signature = "FILE";

        // When / Then
        assert_eq!(name_of(signature), "FILE");
    }

    #[test]
    fn test_parses_a_typedef_alias_pair() {
        // Given — `.. c:type:: unsigned long ulong`
        let signature = "unsigned long ulong";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then — the leading words are the type, the last is the name.
        assert_eq!(declaration.specifiers, vec!["unsigned", "long"]);
        assert_eq!(declaration.name(), Some("ulong"));
    }

    #[test]
    fn test_parses_a_plain_declaration() {
        // Given
        let signature = "int x";

        // When / Then
        assert_eq!(name_of(signature), "x");
    }

    #[test]
    fn test_parses_a_non_keyword_type_name() {
        // Given — `Py_ssize_t` is a typedef, not a keyword, so the "is this
        // the type or the name?" rule has to resolve it.
        let signature = "Py_ssize_t ob_refcnt";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        assert_eq!(declaration.specifiers, vec!["Py_ssize_t"]);
        assert_eq!(declaration.name(), Some("ob_refcnt"));
    }

    #[test]
    fn test_parses_storage_class_and_qualifiers() {
        // Given
        let signature = "static const unsigned long long x";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        assert_eq!(
            declaration.specifiers,
            vec!["static", "const", "unsigned", "long", "long"]
        );
        assert_eq!(declaration.name(), Some("x"));
    }

    #[test]
    fn test_parses_the_typedef_keyword() {
        // Given
        let signature = "typedef int myint";

        // When / Then
        assert_eq!(name_of(signature), "myint");
    }

    #[test]
    fn test_parses_a_struct_tag_type() {
        // Given
        let signature = "struct Foo *x";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        assert_eq!(declaration.specifiers, vec!["struct", "Foo"]);
        assert_eq!(declaration.name(), Some("x"));
    }

    #[test]
    fn test_parses_an_enum_tag_type() {
        // Given
        let signature = "enum E e";

        // When / Then
        assert_eq!(name_of(signature), "e");
    }

    // ── Pointers ──────────────────────────────────────────────────────────

    #[test]
    fn test_parses_a_pointer_declarator() {
        // Given
        let signature = "int *x";

        // When / Then
        assert_eq!(name_of(signature), "x");
    }

    #[test]
    fn test_parses_a_pointer_with_whitespace_on_both_sides() {
        // Given
        let signature = "int * x";

        // When / Then
        assert_eq!(name_of(signature), "x");
    }

    #[test]
    fn test_parses_a_double_pointer() {
        // Given
        let signature = "int **x";

        // When / Then
        assert_eq!(name_of(signature), "x");
    }

    #[test]
    fn test_parses_a_qualified_pointer() {
        // Given — the `const` binds to the pointer, not the pointee.
        let signature = "char * const * x";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        let Declarator::Pointer { qualifiers, .. } = &declaration.declarator else {
            panic!("expected a pointer declarator");
        };
        assert_eq!(qualifiers, &["const"]);
        assert_eq!(declaration.name(), Some("x"));
    }

    // ── Arrays ────────────────────────────────────────────────────────────

    #[test]
    fn test_parses_an_unsized_array() {
        // Given
        let signature = "int x[]";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        let Declarator::Array { size, .. } = &declaration.declarator else {
            panic!("expected an array declarator");
        };
        assert_eq!(size, &None);
        assert_eq!(declaration.name(), Some("x"));
    }

    #[test]
    fn test_parses_a_sized_array_capturing_the_size_verbatim() {
        // Given
        let signature = "int x[10]";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        let Declarator::Array { size, .. } = &declaration.declarator else {
            panic!("expected an array declarator");
        };
        assert_eq!(size.as_deref(), Some("10"));
    }

    #[test]
    fn test_parses_an_array_sized_by_an_identifier() {
        // Given
        let signature = "int x[N]";

        // When / Then
        assert_eq!(name_of(signature), "x");
    }

    #[test]
    fn test_parses_a_static_sized_array_parameter() {
        // Given — C99's `[static N]` form.
        let signature = "int x[static 4]";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        let Declarator::Array { size, .. } = &declaration.declarator else {
            panic!("expected an array declarator");
        };
        assert_eq!(size.as_deref(), Some("static 4"));
    }

    #[test]
    fn test_parses_a_two_dimensional_array() {
        // Given
        let signature = "int x[2][3]";

        // When / Then
        assert_eq!(name_of(signature), "x");
    }

    // ── Functions ─────────────────────────────────────────────────────────

    #[test]
    fn test_parses_a_function_with_no_parameters() {
        // Given
        let signature = "int f()";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        let Declarator::Function { parameters, .. } = &declaration.declarator else {
            panic!("expected a function declarator");
        };
        assert!(parameters.is_empty());
        assert_eq!(declaration.name(), Some("f"));
    }

    #[test]
    fn test_normalises_a_void_parameter_list_to_no_parameters() {
        // Given — `(void)` means "takes nothing", not "takes one void".
        let signature = "int f(void)";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        let Declarator::Function { parameters, .. } = &declaration.declarator else {
            panic!("expected a function declarator");
        };
        assert!(parameters.is_empty());
    }

    #[test]
    fn test_parses_a_function_with_named_parameters() {
        // Given
        let signature = "int f(int a, int b)";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        let Declarator::Function { parameters, .. } = &declaration.declarator else {
            panic!("expected a function declarator");
        };
        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters[0].name(), Some("a"));
        assert_eq!(parameters[1].name(), Some("b"));
    }

    #[test]
    fn test_parses_a_varargs_function() {
        // Given
        let signature = "int f(const char *fmt, ...)";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        let Declarator::Function {
            parameters,
            varargs,
            ..
        } = &declaration.declarator
        else {
            panic!("expected a function declarator");
        };
        assert!(varargs);
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name(), Some("fmt"));
    }

    #[test]
    fn test_parses_an_unnamed_parameter_as_abstract() {
        // Given — parameter types with no names are legal and common.
        let signature = "int f(PyObject *)";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        let Declarator::Function { parameters, .. } = &declaration.declarator else {
            panic!("expected a function declarator");
        };
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name(), None);
    }

    #[test]
    fn test_parses_a_pointer_return_type() {
        // Given — the `*` binds to the return type, and the heuristic this
        // crate replaces already handled this one.
        let signature = "PyObject *PyUnicode_FromString(const char *str)";

        // When / Then
        assert_eq!(name_of(signature), "PyUnicode_FromString");
    }

    // ── Grouped declarators ───────────────────────────────────────────────

    #[test]
    fn test_parses_a_function_pointer_typedef() {
        // Given — the shape that motivated this crate: the name is inside the
        // `(*…)` group, not before the first parenthesis.
        let signature = "int (*type_name)(int arg1, int arg2)";

        // When / Then
        assert_eq!(name_of(signature), "type_name");
    }

    #[test]
    fn test_parses_a_function_pointer_with_a_pointer_return_type() {
        // Given — `c-api/typeobj.rst`'s slot typedefs take this shape.
        let signature = "PyObject *(*unaryfunc)(PyObject *)";

        // When / Then
        assert_eq!(name_of(signature), "unaryfunc");
    }

    #[test]
    fn test_parses_a_pointer_to_function_pointer() {
        // Given
        let signature = "int (**fp)(void)";

        // When / Then
        assert_eq!(name_of(signature), "fp");
    }

    #[test]
    fn test_parses_a_pointer_to_array() {
        // Given
        let signature = "int (*p)[10]";

        // When / Then
        assert_eq!(name_of(signature), "p");
    }

    #[test]
    fn test_parses_a_function_returning_a_function_pointer() {
        // Given — the textbook worst case: `signal` takes a function pointer
        // and returns one.
        let signature = "void (*signal(int, void (*)(int)))(int)";

        // When / Then
        assert_eq!(name_of(signature), "signal");
    }

    #[test]
    fn test_parses_a_pointer_to_array_of_function_pointers() {
        // Given
        let signature = "int (*(*x)[5])(void)";

        // When / Then
        assert_eq!(name_of(signature), "x");
    }

    #[test]
    fn test_builds_a_function_pointer_tree_outermost_first() {
        // Given — structure, not just the name: `int (*f)(int)` is a pointer
        // to a function, so the function suffix must wrap the pointer.
        let signature = "int (*f)(int)";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        let Declarator::Function { inner, .. } = &declaration.declarator else {
            panic!("expected the outermost declarator to be a function");
        };
        let Declarator::Pointer { inner, .. } = inner.as_ref() else {
            panic!("expected a pointer inside the function");
        };
        assert_eq!(inner.as_ref(), &Declarator::Name("f".to_string()));
    }

    // ── Documentation-specific trailing forms ─────────────────────────────

    #[test]
    fn test_parses_a_member_bitfield_width() {
        // Given
        let signature = "unsigned int flags : 3";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        assert_eq!(declaration.bitfield_width.as_deref(), Some("3"));
        assert_eq!(declaration.name(), Some("flags"));
    }

    #[test]
    fn test_parses_a_numeric_initialiser() {
        // Given
        let signature = "int x = 5";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        assert_eq!(declaration.initializer.as_deref(), Some("5"));
        assert_eq!(declaration.name(), Some("x"));
    }

    #[test]
    fn test_parses_a_pointer_initialiser() {
        // Given
        let signature = "PyObject *o = NULL";

        // When
        let declaration = parse_declaration(signature).expect("should parse");

        // Then
        assert_eq!(declaration.initializer.as_deref(), Some("NULL"));
        assert_eq!(declaration.name(), Some("o"));
    }

    #[test]
    fn test_skips_a_leading_attribute() {
        // Given
        let signature = "__attribute__((noreturn)) void f(void)";

        // When / Then
        assert_eq!(name_of(signature), "f");
    }

    #[test]
    fn test_parses_a_function_like_macro_signature() {
        // Given — `.. c:macro:: MAX(a, b)`: no return type, so `MAX` is the
        // name and the parenthesis opens its parameter list.
        let signature = "MAX(a, b)";

        // When / Then
        assert_eq!(name_of(signature), "MAX");
    }

    #[test]
    fn test_parses_an_object_like_macro_signature() {
        // Given
        let signature = "PY_SSIZE_T_MAX";

        // When / Then
        assert_eq!(name_of(signature), "PY_SSIZE_T_MAX");
    }

    // ── Failure and robustness ────────────────────────────────────────────

    #[test]
    fn test_rejects_empty_input() {
        // Given
        let signature = "";

        // When
        let result = parse_declaration(signature);

        // Then
        assert_eq!(
            result.unwrap_err().message,
            "empty declaration".to_string(),
            "empty input has nothing to fall back on"
        );
    }

    #[test]
    fn test_rejects_whitespace_only_input() {
        // Given
        let signature = "   \t  ";

        // When / Then
        assert!(parse_declaration(signature).is_err());
    }

    #[test]
    fn test_rejects_a_declaration_with_no_declared_name() {
        // Given — parses fine as a type, but declares nothing.
        let signature = "int";

        // When
        let result = declared_name(signature);

        // Then
        assert_eq!(result.unwrap_err().message, "declaration declares no name");
    }

    #[test]
    fn test_rejects_an_unbalanced_parenthesis() {
        // Given
        let signature = "int (*x(";

        // When / Then
        assert!(parse_declaration(signature).is_err());
    }

    #[test]
    fn test_rejects_an_unterminated_array() {
        // Given
        let signature = "int x[";

        // When / Then
        assert!(parse_declaration(signature).is_err());
    }

    #[test]
    fn test_rejects_a_stray_closing_parenthesis() {
        // Given
        let signature = ")";

        // When
        let error = parse_declaration(signature).unwrap_err();

        // Then
        assert_eq!(error.offset, 0);
    }

    #[test]
    fn test_rejects_trailing_junk_after_a_complete_declaration() {
        // Given
        let signature = "int x ??";

        // When
        let error = parse_declaration(signature).unwrap_err();

        // Then
        assert!(error.message.starts_with("unexpected"));
        assert_eq!(error.offset, 6);
    }

    #[test]
    fn test_rejects_prose_that_is_not_a_declaration() {
        // Given
        let signature = ">>> not a declaration <<<";

        // When / Then
        assert!(parse_declaration(signature).is_err());
    }

    #[test]
    fn test_rejects_declarators_nested_beyond_the_depth_bound() {
        // Given — deeper than `MAX_DECLARATOR_DEPTH` groups.
        let signature = format!("int {}x{}", "(*".repeat(200), ")".repeat(200));

        // When
        let error = parse_declaration(&signature).unwrap_err();

        // Then — a bounded error rather than a blown stack.
        assert_eq!(error.message, "declarator nested too deeply");
    }

    #[test]
    fn test_never_panics_on_truncations_or_deletions_of_real_signatures() {
        // Given — every prefix and every single-character deletion of a set
        // of real signatures, which is where a hand-rolled parser is most
        // likely to walk off the end of its token stream.
        let signatures = [
            "int (*Py_tracefunc)(PyObject *obj, PyFrameObject *frame, int what, PyObject *arg)",
            "PyObject *(*unaryfunc)(PyObject *)",
            "void (*signal(int, void (*)(int)))(int)",
            "unsigned int flags : 3",
            "int x[static 4] = {0}",
            "__attribute__((noreturn)) void f(void)",
        ];

        // When / Then — the assertion is simply that each call returns.
        for signature in signatures {
            for end in 0..=signature.len() {
                if signature.is_char_boundary(end) {
                    let _ = parse_declaration(&signature[..end]);
                }
            }
            for skip in 0..signature.len() {
                let mutated: String = signature
                    .chars()
                    .enumerate()
                    .filter(|&(index, _)| index != skip)
                    .map(|(_, character)| character)
                    .collect();
                let _ = parse_declaration(&mutated);
            }
        }
    }

    // ── Real CPython signatures ───────────────────────────────────────────

    #[test]
    fn test_parses_cpython_py_tracefunc_from_c_api_init() {
        // Given — `Doc/c-api/init.rst`; referenced 8 times from
        // `Doc/c-api/profiling.rst` and unresolvable until now.
        let signature =
            "int (*Py_tracefunc)(PyObject *obj, PyFrameObject *frame, int what, PyObject *arg)";

        // When / Then
        assert_eq!(name_of(signature), "Py_tracefunc");
    }

    #[test]
    fn test_parses_cpython_slot_typedefs_from_c_api_typeobj() {
        // Given — `Doc/c-api/typeobj.rst` declares dozens of these.
        let cases = [
            ("PyObject *(*unaryfunc)(PyObject *)", "unaryfunc"),
            (
                "PyObject *(*binaryfunc)(PyObject *, PyObject *)",
                "binaryfunc",
            ),
            (
                "PyObject *(*ternaryfunc)(PyObject *, PyObject *, PyObject *)",
                "ternaryfunc",
            ),
            ("int (*inquiry)(PyObject *)", "inquiry"),
            ("Py_ssize_t (*lenfunc)(PyObject *)", "lenfunc"),
            ("int (*visitproc)(PyObject *object, void *arg)", "visitproc"),
            (
                "int (*traverseproc)(PyObject *self, visitproc visit, void *arg)",
                "traverseproc",
            ),
            ("Py_hash_t (*hashfunc)(PyObject *)", "hashfunc"),
            (
                "PyObject *(*richcmpfunc)(PyObject *, PyObject *, int)",
                "richcmpfunc",
            ),
            ("PyObject *(*getiterfunc)(PyObject *)", "getiterfunc"),
            (
                "PyObject *(*descrgetfunc)(PyObject *, PyObject *, PyObject *)",
                "descrgetfunc",
            ),
            (
                "int (*initproc)(PyObject *, PyObject *, PyObject *)",
                "initproc",
            ),
            (
                "PyObject *(*newfunc)(PyTypeObject *, PyObject *, PyObject *)",
                "newfunc",
            ),
            ("void (*destructor)(PyObject *)", "destructor"),
        ];

        // When / Then
        for (signature, expected) in cases {
            assert_eq!(name_of(signature), expected, "for {signature:?}");
        }
    }

    #[test]
    fn test_parses_cpython_member_and_macro_signatures() {
        // Given — the non-function C object types.
        let cases = [
            ("Py_ssize_t ob_refcnt", "ob_refcnt"),
            ("PyTypeObject *ob_type", "ob_type"),
            ("PY_SSIZE_T_MAX", "PY_SSIZE_T_MAX"),
            ("Py_RETURN_NONE", "Py_RETURN_NONE"),
        ];

        // When / Then
        for (signature, expected) in cases {
            assert_eq!(name_of(signature), expected, "for {signature:?}");
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────

    #[test]
    fn test_is_void_parameter_list_recognises_only_a_lone_unnamed_void() {
        // Given
        let void_parameter = Declaration {
            specifiers: vec!["void".to_string()],
            declarator: Declarator::Abstract,
            initializer: None,
            bitfield_width: None,
        };
        let named_void_parameter = Declaration {
            declarator: Declarator::Name("v".to_string()),
            ..void_parameter.clone()
        };
        let int_parameter = Declaration {
            specifiers: vec!["int".to_string()],
            ..void_parameter.clone()
        };

        // When / Then
        assert!(is_void_parameter_list(std::slice::from_ref(
            &void_parameter
        )));
        assert!(!is_void_parameter_list(&[]));
        assert!(!is_void_parameter_list(&[named_void_parameter]));
        assert!(!is_void_parameter_list(&[int_parameter]));
        assert!(!is_void_parameter_list(&[
            void_parameter.clone(),
            void_parameter
        ]));
    }

    #[test]
    fn test_parse_error_displays_message_and_offset() {
        // Given
        let error = ParseError {
            message: "expected ')'".to_string(),
            offset: 7,
        };

        // When / Then
        assert_eq!(error.to_string(), "expected ')' at offset 7");
    }
}
