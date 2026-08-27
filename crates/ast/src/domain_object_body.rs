use serde::{Deserialize, Serialize};

use crate::c_object_type::CObjectType;
use crate::c_signature::CSignature;
use crate::node::Node;
use crate::non_empty_vector::NonEmptyVector;
use crate::object_type::ObjectType;
use crate::py_object_type::PyObjectType;
use crate::std_object_type::StdObjectType;
use crate::target_name::TargetName;

/// Extracts the referenceable name from a `py:*` domain object signature.
///
/// Takes the text before the first `(` (or the whole string if there is
/// none), then its last whitespace-separated token — e.g. `"foo(bar)"` ->
/// `"foo"`. This also strips an optional base-class list for free, e.g.
/// `"Greeter(Base)"` -> `"Greeter"`. Python-specific: identifiers never
/// start with a pointer sigil, so unlike the C extractor below, there is
/// nothing to strip from the extracted token.
#[must_use]
pub fn extract_python_object_name(signature: &str) -> String {
    let before_parens = signature.split('(').next().unwrap_or(signature).trim();
    before_parens
        .split_whitespace()
        .next_back()
        .unwrap_or(before_parens)
        .to_string()
}

/// Splits one raw `.. option::`/`.. cmdoption::` argument *line* into its
/// comma-separated specs, e.g. `"-c, --compress"` -> `["-c", "--compress"]`.
/// A line with no comma yields a single-element result (`"-m <module>"` ->
/// `["-m <module>"]`). Each piece is trimmed; empty pieces (a stray leading/
/// trailing/doubled comma) are dropped rather than kept as an empty spec.
#[must_use]
pub fn split_option_line_specs(line: &str) -> Vec<String> {
    line.split(',')
        .map(str::trim)
        .filter(|spec| !spec.is_empty())
        .map(str::to_string)
        .collect()
}

/// Extracts the referenceable flag name from one `.. option::` spec,
/// mirroring real Sphinx's `option_desc_re`
/// (`(?:/|--|-|\+)[^\s=]+`): a leading `-`, `--`, `/`, or `+` sigil followed
/// by the run of non-whitespace, non-`=` characters after it — e.g.
/// `"-m <module>"` -> `"-m"`, `"--check-hash-based-pycs default|always|never"`
/// -> `"--check-hash-based-pycs"`, `"--with-wheel-pkg-dir=PATH"` ->
/// `"--with-wheel-pkg-dir"`.
///
/// Falls back to the whole trimmed spec when no sigil matches at all, rather
/// than dropping it — the same "never lose content, degrade to a heuristic
/// instead" convention `extract_c_object_name` follows for a signature its
/// real parser can't handle.
#[must_use]
pub fn extract_option_name(spec: &str) -> String {
    let spec = spec.trim();
    let sigil_len = if spec.starts_with("--") {
        2
    } else if spec.starts_with(['-', '/', '+']) {
        1
    } else {
        return spec.to_string();
    };
    let (sigil, rest) = spec.split_at(sigil_len);
    let flag_body_len = rest
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(rest.len());
    if flag_body_len == 0 {
        // A bare sigil with nothing after it (e.g. just `"-"`) is not a
        // valid flag — fall back to the whole spec rather than returning an
        // empty/meaningless name.
        return spec.to_string();
    }
    format!("{sigil}{}", &rest[..flag_body_len])
}

/// Builds the qualified [`TargetName`] key shared by domain object
/// registration (analyzer) and cross-reference resolution (renderer), so
/// both always agree on the key for the same object.
#[must_use]
pub fn build_domain_object_key(object_type: ObjectType, name: &str) -> TargetName {
    TargetName::new(&format!(
        "{}:{}:{}",
        object_type.domain().as_str(),
        object_type.as_str(),
        name
    ))
}

/// The body of a domain object *definition* directive (e.g. `.. py:function::`,
/// `.. py:module::`, `.. c:function::`) — one variant per concrete object
/// type, each carrying exactly the fields/options meaningful to it.
///
/// This is deliberately separate from [`ObjectType`]: `ObjectType` is a
/// lightweight tag shared with cross-reference roles (`InlineNode::
/// DomainObjectReference`), which never carry options — only definitions do.
///
/// Every variant but `PyModule` holds `signatures`, not a single name: one
/// directive may declare several argument lines, each an independently
/// referenceable alias for the same documented object, all sharing one body.
/// `PyModule` is the exception because real Sphinx's `module` directive takes
/// exactly one argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DomainObjectBody {
    PyFunction {
        signatures: NonEmptyVector<String>,
        /// Set by the `.. decorator::` directive-name alias (real Sphinx's
        /// `PyDecoratorFunction`, which registers exactly as a `py:function`
        /// but additionally prefixes the rendered signature with a literal
        /// `@`). No dedicated `ObjectType`/directive option corresponds to
        /// this — it's parser-derived intent, not something an author can
        /// set via a body option line the way `py:method`'s flags are.
        is_decorator: bool,
        /// The `:module:` option: overrides the ambient `py:module`/
        /// `py:currentmodule` context for this definition and its nested
        /// body only, restored afterward. See [`Self::module_override`].
        module: Option<String>,
        body: Vec<Node>,
    },
    PyModule {
        name: String,
        /// Comma-separated platform identifiers (e.g. `"Unix, Windows"`).
        platform: Option<String>,
        /// One-sentence module summary.
        synopsis: Option<String>,
        /// Marks the module as deprecated.
        deprecated: bool,
        body: Vec<Node>,
    },
    PyData {
        signatures: NonEmptyVector<String>,
        /// The data item's type annotation (e.g. `"int"`).
        type_: Option<String>,
        /// The data item's value (e.g. `"30"`).
        value: Option<String>,
        /// The `:module:` option — see [`PyFunction::module`] and
        /// [`Self::module_override`].
        module: Option<String>,
        body: Vec<Node>,
    },
    PyAttribute {
        signatures: NonEmptyVector<String>,
        /// The attribute's type annotation (e.g. `"int"`).
        type_: Option<String>,
        /// The attribute's initial value (e.g. `"30"`).
        value: Option<String>,
        /// The fully qualified name (including module) of where the
        /// attribute is actually defined, when documented via a re-export.
        /// Rendered as metadata only — no alias/cross-reference-redirect
        /// semantics.
        canonical: Option<String>,
        /// The `:module:` option — see [`PyFunction::module`] and
        /// [`Self::module_override`].
        module: Option<String>,
        body: Vec<Node>,
    },
    CFunction {
        signatures: NonEmptyVector<CSignature>,
        body: Vec<Node>,
    },
    CMacro {
        signatures: NonEmptyVector<CSignature>,
        body: Vec<Node>,
    },
    CStruct {
        signatures: NonEmptyVector<CSignature>,
        /// Suppresses the cross-reference target entirely (and, per real
        /// Sphinx, implies `no_index_entry`).
        no_index: bool,
        /// Suppresses the general-index (`genindex.html`) entry only; the
        /// cross-reference target is still created.
        no_index_entry: bool,
        /// Excludes this object from a local contents/TOC listing. Parsed
        /// and stored for round-tripping, but rusty-sphinx has no such
        /// listing for domain objects yet, so it has no rendering effect.
        no_contents_entry: bool,
        body: Vec<Node>,
    },
    CUnion {
        signatures: NonEmptyVector<CSignature>,
        no_index: bool,
        no_index_entry: bool,
        no_contents_entry: bool,
        body: Vec<Node>,
    },
    CMember {
        signatures: NonEmptyVector<CSignature>,
        no_index: bool,
        no_index_entry: bool,
        no_contents_entry: bool,
        body: Vec<Node>,
    },
    CType {
        signatures: NonEmptyVector<CSignature>,
        no_index: bool,
        no_index_entry: bool,
        no_contents_entry: bool,
        body: Vec<Node>,
    },
    PyMethod {
        signatures: NonEmptyVector<String>,
        is_classmethod: bool,
        is_staticmethod: bool,
        is_abstractmethod: bool,
        is_async: bool,
        /// Set by the `.. decoratormethod::` directive-name alias — the
        /// `py:method` counterpart of `PyFunction::is_decorator`, see there.
        is_decorator: bool,
        /// The `:module:` option — see [`PyFunction::module`] and
        /// [`Self::module_override`].
        module: Option<String>,
        body: Vec<Node>,
    },
    PyClass {
        signatures: NonEmptyVector<String>,
        is_final: bool,
        /// The `:module:` option — see [`PyFunction::module`] and
        /// [`Self::module_override`].
        module: Option<String>,
        body: Vec<Node>,
    },
    PyException {
        signatures: NonEmptyVector<String>,
        is_final: bool,
        /// The `:module:` option — see [`PyFunction::module`] and
        /// [`Self::module_override`].
        module: Option<String>,
        body: Vec<Node>,
    },
    /// `.. option::`/`.. cmdoption::` (the `std` domain's only object type
    /// modeled today). One entry per raw argument *line*, stored verbatim —
    /// unlike every other variant, one line may itself name *several*
    /// independently-referenceable flags at once (comma-separated, e.g.
    /// `"-c, --compress"`), which real Sphinx renders as one shared `<dt>`
    /// with multiple anchor ids. That per-line/per-spec split (via
    /// [`split_option_line_specs`]/[`extract_option_name`]) happens in
    /// `index_domain_object`/`render_domain_object`, not here, and neither
    /// analyzer nor renderer route `StdCmdoption` through the generic
    /// `names()`-driven one-`<dt>`-per-name loop other variants share.
    StdCmdoption {
        signatures: NonEmptyVector<String>,
        body: Vec<Node>,
    },
}

impl DomainObjectBody {
    /// The [`ObjectType`] this definition belongs to.
    #[must_use]
    pub const fn object_type(&self) -> ObjectType {
        match self {
            Self::PyFunction { .. } => ObjectType::Py(PyObjectType::Function),
            Self::PyModule { .. } => ObjectType::Py(PyObjectType::Module),
            Self::PyData { .. } => ObjectType::Py(PyObjectType::Data),
            Self::PyAttribute { .. } => ObjectType::Py(PyObjectType::Attribute),
            Self::CFunction { .. } => ObjectType::C(CObjectType::Function),
            Self::CMacro { .. } => ObjectType::C(CObjectType::Macro),
            Self::CStruct { .. } => ObjectType::C(CObjectType::Struct),
            Self::CUnion { .. } => ObjectType::C(CObjectType::Union),
            Self::CMember { .. } => ObjectType::C(CObjectType::Member),
            Self::CType { .. } => ObjectType::C(CObjectType::Type),
            Self::PyMethod { .. } => ObjectType::Py(PyObjectType::Method),
            Self::PyClass { .. } => ObjectType::Py(PyObjectType::Class),
            Self::PyException { .. } => ObjectType::Py(PyObjectType::Exception),
            Self::StdCmdoption { .. } => ObjectType::Std(StdObjectType::Cmdoption),
        }
    }

    /// The referenceable names used to build the cross-reference keys
    /// ([`build_domain_object_key`]) — one per declared signature, extracted
    /// from the signature for function-like objects, or taken directly for
    /// modules/data/attributes, whose signatures are already bare names.
    ///
    /// The first entry is the *primary* name: the only one qualified against
    /// the enclosing scope for the purpose of lending class context to the
    /// body (see [`Self::deduce_local_scope`]), mirroring real Sphinx, whose
    /// `before_content()` acts on the first parsed signature. Every entry,
    /// primary or not, is registered as an independently resolvable target.
    #[must_use]
    pub fn names(&self) -> NonEmptyVector<String> {
        match self {
            Self::PyFunction { signatures, .. }
            | Self::PyMethod { signatures, .. }
            | Self::PyClass { signatures, .. }
            | Self::PyException { signatures, .. } => {
                signatures.map(|signature| extract_python_object_name(signature))
            }
            Self::CFunction { signatures, .. }
            | Self::CMacro { signatures, .. }
            | Self::CStruct { signatures, .. }
            | Self::CUnion { signatures, .. }
            | Self::CMember { signatures, .. }
            | Self::CType { signatures, .. } => {
                signatures.map(|signature| signature.name().to_string())
            }
            Self::PyData { signatures, .. } | Self::PyAttribute { signatures, .. } => {
                signatures.map(String::clone)
            }
            Self::PyModule { name, .. } => NonEmptyVector::single(name.clone()),
            // Every flag across every raw line, flattened — e.g. a directive
            // written as `.. option:: -c, --compress` yields `["-c",
            // "--compress"]` here even though it is a *single* signature
            // line. This is the one variant where `names()` and
            // `signature_texts()` below are not index-parallel: `StdCmdoption`
            // is deliberately never routed through the generic
            // one-`<dt>`-per-name loop that relies on that parallelism (see
            // `index_domain_object`/`render_domain_object`), so nothing
            // depends on the lengths matching here.
            Self::StdCmdoption { signatures, .. } => {
                let mut names = signatures
                    .as_slice()
                    .iter()
                    .flat_map(|line| split_option_line_specs(line))
                    .map(|spec| extract_option_name(&spec));
                let first = names.next().unwrap_or_default();
                NonEmptyVector::new(first, names.collect())
            }
        }
    }

    /// The raw texts shown in the rendered `<dt>`s — one per declared
    /// signature: the full signature for function-like objects, or the bare
    /// dotted name for modules/data/attributes.
    ///
    /// Index-parallel to [`Self::names`] and equally non-empty; returns a
    /// `Vec` rather than a borrowed slice because the three storage shapes
    /// differ — `py` objects hold plain `String`s, `c` objects hold
    /// [`CSignature`]s, and `PyModule` holds a single name. **Except**
    /// `StdCmdoption`, whose entries are one per raw *line* (see [`Self::names`]).
    #[must_use]
    pub fn signature_texts(&self) -> Vec<&str> {
        match self {
            Self::PyFunction { signatures, .. }
            | Self::PyMethod { signatures, .. }
            | Self::PyClass { signatures, .. }
            | Self::PyException { signatures, .. }
            | Self::PyData { signatures, .. }
            | Self::PyAttribute { signatures, .. }
            | Self::StdCmdoption { signatures, .. } => {
                signatures.as_slice().iter().map(String::as_str).collect()
            }
            Self::CFunction { signatures, .. }
            | Self::CMacro { signatures, .. }
            | Self::CStruct { signatures, .. }
            | Self::CUnion { signatures, .. }
            | Self::CMember { signatures, .. }
            | Self::CType { signatures, .. } => {
                signatures.as_slice().iter().map(CSignature::text).collect()
            }
            Self::PyModule { name, .. } => vec![name.as_str()],
        }
    }

    /// The parsed docstring body shared by every object type.
    #[must_use]
    pub fn body(&self) -> &[Node] {
        match self {
            Self::PyFunction { body, .. }
            | Self::PyModule { body, .. }
            | Self::PyData { body, .. }
            | Self::PyAttribute { body, .. }
            | Self::CFunction { body, .. }
            | Self::CMacro { body, .. }
            | Self::CStruct { body, .. }
            | Self::CUnion { body, .. }
            | Self::CMember { body, .. }
            | Self::CType { body, .. }
            | Self::PyMethod { body, .. }
            | Self::PyClass { body, .. }
            | Self::PyException { body, .. }
            | Self::StdCmdoption { body, .. } => body,
        }
    }

    /// The class segments, if any, that this object's own nested body
    /// content should have pushed onto the enclosing `PythonScope` (in the
    /// `rusty_sphinx_scope` crate, which depends on this one, not the other
    /// way around, so it can't be linked from here) — the "class context" a
    /// bare cross-reference written inside that body resolves against
    /// first, and the qualifier a domain object *defined* inside it is
    /// indexed under. `new_segments` is this object's own contribution as
    /// returned by `PythonScope::qualify` — its (possibly dotted) name with
    /// any repeat of the *existing* class scope already absorbed.
    ///
    /// Two ways an object establishes one:
    /// - `py:class`/`py:exception` bodies introduce every one of their own
    ///   new segments (real lexical nesting: `.. method:: find_spec` written
    ///   inside `.. class:: zipimporter` is
    ///   `zipimport.zipimporter.find_spec`).
    /// - Any other `py` object *written with a dotted signature* lends all
    ///   but the last of its new segments (`.. method:: ZipFile.read` lends
    ///   `ZipFile`), mirroring real Sphinx's `PyObject.before_content()`/
    ///   `after_content()`, which sets the `py:class` `ref_context` from the
    ///   signature's name-prefix for the duration of that directive's body,
    ///   then restores it. This is what lets a bare ``:meth:`read` `` written
    ///   inside `.. method:: ZipFile.open`'s body resolve against
    ///   `ZipFile.read` with no global search: `open`'s own signature
    ///   already carries the scope. Real `CPython` docs (e.g. `zipfile.rst`)
    ///   document a class's methods flat like this rather than nested.
    ///
    /// `py:module` establishes no *class* scope: real Sphinx's `module`
    /// directive is not a `PyObject` and never sets `py:class` from its own
    /// name — it sets only the persistent, document-order module context
    /// `PythonScope::set_module` already models, so a dotted module
    /// name (`xml.etree.ElementTree`) must never be mistaken for a class
    /// prefix. `c` domain objects establish none either.
    ///
    /// Shared by the analyzer (`index_domain_object`) and the renderer
    /// (`render_domain_object`), so both always agree on the scope a given
    /// body introduces. Matched exhaustively rather than with a wildcard, so
    /// a new object type can't be added without deciding what it scopes.
    #[must_use]
    pub fn deduce_local_scope(&self, new_segments: &[String]) -> Vec<String> {
        match self {
            // `c:struct`/`c:union`/`c:type` nest exactly like
            // `py:class`/`py:exception` (real lexical nesting, just under
            // `CScope` rather than `PythonScope` — see the
            // analyzer/renderer's `uses_c_scope` branch). Real Sphinx's C
            // domain scopes nested declarations generically off whatever
            // declaration they're indented under, not specifically off
            // struct/union — confirmed by its own docs example nesting
            // `c:var` inside `c:union` inside `c:struct` — so `c:type` gets
            // the same treatment even though it has no members of its own
            // the way struct/union do.
            Self::PyClass { .. }
            | Self::PyException { .. }
            | Self::CStruct { .. }
            | Self::CUnion { .. }
            | Self::CType { .. } => new_segments.to_vec(),
            Self::PyFunction { .. }
            | Self::PyMethod { .. }
            | Self::PyData { .. }
            | Self::PyAttribute { .. } => new_segments
                .split_last()
                .map(|(_, rest)| rest.to_vec())
                .unwrap_or_default(),
            // Nothing nests under a `c:member` in real Sphinx, unlike
            // `py:data`/`py:attribute`, which lend a dotted prefix.
            // `StdCmdoption` lends nothing either: options never nest, and are
            // qualified against the ambient `.. program::` context, not
            // against a class/container stack.
            Self::PyModule { .. }
            | Self::CFunction { .. }
            | Self::CMacro { .. }
            | Self::CMember { .. }
            | Self::StdCmdoption { .. } => Vec::new(),
        }
    }

    /// Mutable access to the parsed docstring body, for passes that rewrite
    /// nested nodes in place (e.g. assigning `.. index::` anchor ids).
    #[must_use]
    pub fn body_mut(&mut self) -> &mut Vec<Node> {
        match self {
            Self::PyFunction { body, .. }
            | Self::PyModule { body, .. }
            | Self::PyData { body, .. }
            | Self::PyAttribute { body, .. }
            | Self::CFunction { body, .. }
            | Self::CMacro { body, .. }
            | Self::CStruct { body, .. }
            | Self::CUnion { body, .. }
            | Self::CMember { body, .. }
            | Self::CType { body, .. }
            | Self::PyMethod { body, .. }
            | Self::PyClass { body, .. }
            | Self::PyException { body, .. }
            | Self::StdCmdoption { body, .. } => body,
        }
    }

    /// Whether this object's cross-reference target (and, per real Sphinx,
    /// its general-index entry too) is suppressed. `false` for every object
    /// type that doesn't model the option yet.
    #[must_use]
    pub const fn no_index(&self) -> bool {
        match self {
            Self::CStruct { no_index, .. }
            | Self::CUnion { no_index, .. }
            | Self::CMember { no_index, .. }
            | Self::CType { no_index, .. } => *no_index,
            Self::PyFunction { .. }
            | Self::PyModule { .. }
            | Self::PyData { .. }
            | Self::PyAttribute { .. }
            | Self::CFunction { .. }
            | Self::CMacro { .. }
            | Self::PyMethod { .. }
            | Self::PyClass { .. }
            | Self::PyException { .. }
            | Self::StdCmdoption { .. } => false,
        }
    }

    /// Whether this object's general-index (`genindex.html`) entry is
    /// suppressed — true either because `no_index_entry` was set directly,
    /// or because `no_index` implies it. `false` for every object type that
    /// doesn't model either option yet.
    #[must_use]
    pub const fn no_index_entry(&self) -> bool {
        match self {
            Self::CStruct {
                no_index,
                no_index_entry,
                ..
            }
            | Self::CUnion {
                no_index,
                no_index_entry,
                ..
            }
            | Self::CMember {
                no_index,
                no_index_entry,
                ..
            }
            | Self::CType {
                no_index,
                no_index_entry,
                ..
            } => *no_index || *no_index_entry,
            Self::PyFunction { .. }
            | Self::PyModule { .. }
            | Self::PyData { .. }
            | Self::PyAttribute { .. }
            | Self::CFunction { .. }
            | Self::CMacro { .. }
            | Self::PyMethod { .. }
            | Self::PyClass { .. }
            | Self::PyException { .. }
            | Self::StdCmdoption { .. } => false,
        }
    }

    /// The `:module:` option's override value: real Sphinx's `PyObject`
    /// directives (every `py:*` object-description directive except
    /// `py:module` itself, which is not a `PyObject` and has its own,
    /// disjoint `platform`/`synopsis`/`deprecated` option set) accept a
    /// `:module:` option that overrides the ambient `py:module`/
    /// `py:currentmodule` context for this one definition and its nested
    /// body, restored once the directive (including its nested content) is
    /// fully processed — see `PythonScope::push_module_override`/
    /// `restore_module`, which implement that scoped override, and
    /// `deduce_local_scope`, whose class-nesting concept this is
    /// independent of. `None` for object types that don't carry the option
    /// or didn't set it.
    #[must_use]
    pub fn module_override(&self) -> Option<&str> {
        match self {
            Self::PyFunction { module, .. }
            | Self::PyMethod { module, .. }
            | Self::PyClass { module, .. }
            | Self::PyException { module, .. }
            | Self::PyData { module, .. }
            | Self::PyAttribute { module, .. } => module.as_deref(),
            Self::PyModule { .. }
            | Self::CFunction { .. }
            | Self::CMacro { .. }
            | Self::CStruct { .. }
            | Self::CUnion { .. }
            | Self::CMember { .. }
            | Self::CType { .. }
            | Self::StdCmdoption { .. } => None,
        }
    }

    /// Whether this object is excluded from a local contents/TOC listing.
    /// Parsed and stored for the three object types that model it, but
    /// rusty-sphinx has no such listing for domain objects yet, so this has
    /// no rendering effect today.
    #[must_use]
    pub const fn no_contents_entry(&self) -> bool {
        match self {
            Self::CStruct {
                no_contents_entry, ..
            }
            | Self::CUnion {
                no_contents_entry, ..
            }
            | Self::CMember {
                no_contents_entry, ..
            }
            | Self::CType {
                no_contents_entry, ..
            } => *no_contents_entry,
            Self::PyFunction { .. }
            | Self::PyModule { .. }
            | Self::PyData { .. }
            | Self::PyAttribute { .. }
            | Self::CFunction { .. }
            | Self::CMacro { .. }
            | Self::PyMethod { .. }
            | Self::PyClass { .. }
            | Self::PyException { .. }
            | Self::StdCmdoption { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_node::InlineNode;

    #[test]
    fn test_extract_python_object_name_simple_call() {
        // Given
        let signature = "foo(bar)";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_python_object_name_no_parens() {
        // Given
        let signature = "foo";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_python_object_name_no_args() {
        // Given
        let signature = "foo()";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_python_object_name_extra_whitespace() {
        // Given
        let signature = "  foo   (bar)";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_python_object_name_empty_string() {
        // Given
        let signature = "";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "");
    }

    #[test]
    fn test_split_option_line_specs_splits_comma_separated_flags() {
        // Given
        let line = "-c, --compress";

        // When
        let specs = split_option_line_specs(line);

        // Then
        assert_eq!(specs, vec!["-c".to_string(), "--compress".to_string()]);
    }

    #[test]
    fn test_split_option_line_specs_single_flag_with_no_comma() {
        // Given
        let line = "-m <module>";

        // When
        let specs = split_option_line_specs(line);

        // Then
        assert_eq!(specs, vec!["-m <module>".to_string()]);
    }

    #[test]
    fn test_split_option_line_specs_drops_empty_pieces() {
        // Given — a stray trailing comma.
        let line = "-h, --help,";

        // When
        let specs = split_option_line_specs(line);

        // Then
        assert_eq!(specs, vec!["-h".to_string(), "--help".to_string()]);
    }

    #[test]
    fn test_extract_option_name_short_flag() {
        // Given / When / Then
        assert_eq!(extract_option_name("-m <module>"), "-m");
    }

    #[test]
    fn test_extract_option_name_long_flag() {
        // Given / When / Then
        assert_eq!(extract_option_name("--module <module>"), "--module");
    }

    #[test]
    fn test_extract_option_name_long_flag_with_choices_argument() {
        // Given / When / Then
        assert_eq!(
            extract_option_name("--check-hash-based-pycs default|always|never"),
            "--check-hash-based-pycs"
        );
    }

    #[test]
    fn test_extract_option_name_equals_joined_argument() {
        // Given / When / Then
        assert_eq!(
            extract_option_name("--with-wheel-pkg-dir=PATH"),
            "--with-wheel-pkg-dir"
        );
    }

    #[test]
    fn test_extract_option_name_bare_flag_with_no_argument() {
        // Given / When / Then
        assert_eq!(extract_option_name("-h"), "-h");
    }

    #[test]
    fn test_extract_option_name_slash_and_plus_sigils() {
        // Given / When / Then — Windows-style `/` and the rare `+` sigil,
        // both accepted by real Sphinx's `option_desc_re`.
        assert_eq!(extract_option_name("/Wall"), "/Wall");
        assert_eq!(extract_option_name("+x"), "+x");
    }

    #[test]
    fn test_extract_option_name_falls_back_to_whole_spec_when_no_sigil_matches() {
        // Given — malformed: no leading `-`/`--`/`/`/`+`.
        let spec = "not-a-flag";

        // When
        let name = extract_option_name(spec);

        // Then — never lose content, matching `extract_c_object_name`'s
        // fallback philosophy.
        assert_eq!(name, "not-a-flag");
    }

    #[test]
    fn test_extract_option_name_falls_back_for_bare_sigil() {
        // Given — a lone `-` with nothing after it.
        let spec = "-";

        // When
        let name = extract_option_name(spec);

        // Then
        assert_eq!(name, "-");
    }

    #[test]
    fn test_cmdoption_object_type_is_std_cmdoption() {
        // Given
        let obj = DomainObjectBody::StdCmdoption {
            signatures: NonEmptyVector::single("-h".to_string()),
            body: vec![],
        };

        // When / Then
        assert_eq!(
            obj.object_type(),
            ObjectType::Std(crate::std_object_type::StdObjectType::Cmdoption)
        );
    }

    #[test]
    fn test_cmdoption_names_flattens_comma_separated_specs_across_lines() {
        // Given — one line with two comma-separated specs, and a second,
        // single-spec continuation line.
        let obj = DomainObjectBody::StdCmdoption {
            signatures: NonEmptyVector::new(
                "-c, --compress".to_string(),
                vec!["--level <n>".to_string()],
            ),
            body: vec![],
        };

        // When
        let names = obj.names();

        // Then
        assert_eq!(
            names.as_slice(),
            &[
                "-c".to_string(),
                "--compress".to_string(),
                "--level".to_string()
            ]
        );
    }

    #[test]
    fn test_cmdoption_signature_texts_is_one_per_raw_line_not_per_spec() {
        // Given — deliberately asymmetric with `names()` (see its doc
        // comment): one raw line yields two names but one display text.
        let obj = DomainObjectBody::StdCmdoption {
            signatures: NonEmptyVector::single("-c, --compress".to_string()),
            body: vec![],
        };

        // When
        let texts = obj.signature_texts();

        // Then
        assert_eq!(texts, vec!["-c, --compress"]);
    }

    #[test]
    fn test_cmdoption_deduce_local_scope_lends_nothing() {
        // Given
        let obj = DomainObjectBody::StdCmdoption {
            signatures: NonEmptyVector::single("-h".to_string()),
            body: vec![],
        };

        // When
        let lend = obj.deduce_local_scope(&["irrelevant".to_string()]);

        // Then — options never nest, so nothing is lent to the body.
        assert!(lend.is_empty());
    }

    #[test]
    fn test_cmdoption_has_no_index_options_modeled() {
        // Given
        let obj = DomainObjectBody::StdCmdoption {
            signatures: NonEmptyVector::single("-h".to_string()),
            body: vec![],
        };

        // When / Then — real Sphinx's `Cmdoption` has none of these options.
        assert!(!obj.no_index());
        assert!(!obj.no_index_entry());
        assert!(!obj.no_contents_entry());
        assert_eq!(obj.module_override(), None);
    }

    #[test]
    fn test_build_domain_object_key_produces_expected_format() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Function);
        let name = "foo";

        // When
        let key = build_domain_object_key(object_type, name);

        // Then
        assert_eq!(key.as_str(), "py:function:foo");
    }

    #[test]
    fn test_build_domain_object_key_distinguishes_domains() {
        // Given
        let py_type = ObjectType::Py(PyObjectType::Function);
        let c_type = ObjectType::C(CObjectType::Function);
        let name = "foo";

        // When
        let py_key = build_domain_object_key(py_type, name);
        let c_key = build_domain_object_key(c_type, name);

        // Then
        assert_ne!(py_key, c_key);
    }

    #[test]
    fn test_build_domain_object_key_for_module() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Module);
        let name = "mypackage.mymodule";

        // When
        let key = build_domain_object_key(object_type, name);

        // Then
        assert_eq!(key.as_str(), "py:module:mypackage.mymodule");
    }

    #[test]
    fn test_build_domain_object_key_for_data_is_shared_by_data_and_const_roles() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Data);
        let name = "DEFAULT_TIMEOUT";

        // When — both `:py:data:` and `:py:const:` resolve to the same
        // `ObjectType`, so both must build this same key.
        let key = build_domain_object_key(object_type, name);

        // Then — `TargetName` normalizes to lowercase.
        assert_eq!(key.as_str(), "py:data:default_timeout");
    }

    #[test]
    fn test_deduce_local_scope_lends_all_new_segments_for_classes() {
        // Given
        let class = DomainObjectBody::PyClass {
            module: None,
            signatures: NonEmptyVector::single("zipimporter(archivepath)".to_string()),
            is_final: false,
            body: vec![],
        };

        // When
        let scope = class.deduce_local_scope(&["zipimporter".to_string()]);

        // Then
        assert_eq!(scope, vec!["zipimporter".to_string()]);
    }

    #[test]
    fn test_deduce_local_scope_lends_all_new_segments_for_exceptions() {
        // Given — exceptions are classes in Python, so they scope their body
        // the same way.
        let exception = DomainObjectBody::PyException {
            module: None,
            signatures: NonEmptyVector::single("ZipImportError".to_string()),
            is_final: false,
            body: vec![],
        };

        // When
        let scope = exception.deduce_local_scope(&["ZipImportError".to_string()]);

        // Then
        assert_eq!(scope, vec!["ZipImportError".to_string()]);
    }

    #[test]
    fn test_deduce_local_scope_lends_all_but_last_new_segment_for_dotted_method() {
        // Given — the real-world CPython shape that surfaced the
        // "broken domain object 'read'" warning: `zipfile.rst` documents
        // `ZipFile`'s methods flat, with dotted signatures, so the method's
        // own name carries the class scope its body should resolve against.
        let method = DomainObjectBody::PyMethod {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("ZipFile.open(name, mode='r')".to_string()),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When
        let scope = method.deduce_local_scope(&["ZipFile".to_string(), "open".to_string()]);

        // Then — the last component is dropped, not the whole dotted path.
        assert_eq!(scope, vec!["ZipFile".to_string()]);
    }

    #[test]
    fn test_deduce_local_scope_lends_all_but_last_new_segment_for_dotted_function() {
        // Given
        let function = DomainObjectBody::PyFunction {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("path.join(a, *p)".to_string()),
            body: vec![],
        };

        // When
        let scope = function.deduce_local_scope(&["path".to_string(), "join".to_string()]);

        // Then
        assert_eq!(scope, vec!["path".to_string()]);
    }

    #[test]
    fn test_deduce_local_scope_lends_all_but_last_new_segment_for_dotted_attribute() {
        // Given
        let attribute = DomainObjectBody::PyAttribute {
            module: None,
            signatures: NonEmptyVector::single("ZipInfo.filename".to_string()),
            type_: None,
            value: None,
            canonical: None,
            body: vec![],
        };

        // When
        let attribute_scope =
            attribute.deduce_local_scope(&["ZipInfo".to_string(), "filename".to_string()]);

        // Then
        assert_eq!(attribute_scope, vec!["ZipInfo".to_string()]);
    }

    #[test]
    fn test_deduce_local_scope_lends_all_but_last_new_segment_for_dotted_data() {
        // Given
        let data = DomainObjectBody::PyData {
            module: None,
            signatures: NonEmptyVector::single("ZipFile.DEFAULT_TIMEOUT".to_string()),
            type_: None,
            value: None,
            body: vec![],
        };

        // When
        let scope =
            data.deduce_local_scope(&["ZipFile".to_string(), "DEFAULT_TIMEOUT".to_string()]);

        // Then
        assert_eq!(scope, vec!["ZipFile".to_string()]);
    }

    #[test]
    fn test_deduce_local_scope_returns_empty_for_undotted_function() {
        // Given — an unqualified, module-less function has no prefix to lend.
        let function = DomainObjectBody::PyFunction {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("greet(name)".to_string()),
            body: vec![],
        };

        // When
        let scope = function.deduce_local_scope(&["greet".to_string()]);

        // Then
        assert!(scope.is_empty());
    }

    #[test]
    fn test_deduce_local_scope_returns_empty_for_undotted_method() {
        // Given
        let method = DomainObjectBody::PyMethod {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("find_spec(fullname)".to_string()),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When
        let scope = method.deduce_local_scope(&["find_spec".to_string()]);

        // Then
        assert!(scope.is_empty());
    }

    #[test]
    fn test_deduce_local_scope_returns_empty_for_modules() {
        // Given — real Sphinx's `module` directive is not a `PyObject` and
        // never sets `py:class` from its own name — it only sets the
        // persistent, document-order module context
        // (`PythonScope::set_module`), so a dotted module name
        // (`xml.etree.ElementTree`) must never be mistaken for a class
        // prefix.
        let module = DomainObjectBody::PyModule {
            name: "xml.etree.ElementTree".to_string(),
            platform: None,
            synopsis: None,
            deprecated: false,
            body: vec![],
        };

        // When
        let scope = module.deduce_local_scope(&[
            "xml".to_string(),
            "etree".to_string(),
            "ElementTree".to_string(),
        ]);

        // Then — must not lend "xml.etree" to everything in its body.
        assert!(scope.is_empty());
    }

    #[test]
    fn test_deduce_local_scope_returns_empty_for_c_domain_objects() {
        // Given — the `py:class` context is a py-domain-only concept.
        let function = DomainObjectBody::CFunction {
            signatures: NonEmptyVector::single(
                "int PyList_Append(PyObject *list, PyObject *item)".into(),
            ),
            body: vec![],
        };
        let macro_ = DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single("PY_SSIZE_T_MAX".into()),
            body: vec![],
        };

        // When / Then
        assert!(
            function
                .deduce_local_scope(&["PyList_Append".to_string()])
                .is_empty()
        );
        assert!(
            macro_
                .deduce_local_scope(&["PY_SSIZE_T_MAX".to_string()])
                .is_empty()
        );
    }

    #[test]
    fn test_build_domain_object_key_for_attribute() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Attribute);
        let name = "Greeter.name";

        // When
        let key = build_domain_object_key(object_type, name);

        // Then
        assert_eq!(key.as_str(), "py:attribute:greeter.name");
    }

    #[test]
    fn test_domain_object_body_object_type_matches_variant() {
        // Given / When / Then
        assert_eq!(
            DomainObjectBody::PyFunction {
                module: None,
                is_decorator: false,
                signatures: NonEmptyVector::single("greet(name)".to_string()),
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Function)
        );
        assert_eq!(
            DomainObjectBody::PyModule {
                name: "greetings".to_string(),
                platform: None,
                synopsis: None,
                deprecated: false,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Module)
        );
        assert_eq!(
            DomainObjectBody::PyData {
                module: None,
                signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
                type_: None,
                value: None,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Data)
        );
        assert_eq!(
            DomainObjectBody::PyAttribute {
                module: None,
                signatures: NonEmptyVector::single("Greeter.name".to_string()),
                type_: None,
                value: None,
                canonical: None,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Attribute)
        );
        assert_eq!(
            DomainObjectBody::CFunction {
                signatures: NonEmptyVector::single("int add(int a, int b)".into()),
                body: vec![],
            }
            .object_type(),
            ObjectType::C(CObjectType::Function)
        );
        assert_eq!(
            DomainObjectBody::CMacro {
                signatures: NonEmptyVector::single("MAX(a, b)".into()),
                body: vec![],
            }
            .object_type(),
            ObjectType::C(CObjectType::Macro)
        );
        assert_eq!(
            DomainObjectBody::PyMethod {
                module: None,
                is_decorator: false,
                signatures: NonEmptyVector::single("greet(self, name)".to_string()),
                is_classmethod: false,
                is_staticmethod: false,
                is_abstractmethod: false,
                is_async: false,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Method)
        );
        assert_eq!(
            DomainObjectBody::PyClass {
                module: None,
                signatures: NonEmptyVector::single("Greeter".to_string()),
                is_final: false,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Class)
        );
        assert_eq!(
            DomainObjectBody::PyException {
                module: None,
                signatures: NonEmptyVector::single("GreeterError".to_string()),
                is_final: false,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Exception)
        );
    }

    #[test]
    fn test_domain_object_body_object_type_is_function_for_decorator() {
        // Given — a `.. decorator::`-derived `PyFunction`: real Sphinx
        // registers it under the exact same object type as a plain
        // `py:function` (`PyDecoratorFunction.run()` forces
        // `self.name = 'py:function'`), so `is_decorator` must not change
        // `object_type()`.
        let decorator = DomainObjectBody::PyFunction {
            module: None,
            signatures: NonEmptyVector::single("classmethod".to_string()),
            is_decorator: true,
            body: vec![],
        };

        // When / Then
        assert_eq!(
            decorator.object_type(),
            ObjectType::Py(PyObjectType::Function)
        );
    }

    #[test]
    fn test_domain_object_body_object_type_is_method_for_decoratormethod() {
        // Given
        let decorator_method = DomainObjectBody::PyMethod {
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
        assert_eq!(
            decorator_method.object_type(),
            ObjectType::Py(PyObjectType::Method)
        );
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_functions() {
        // Given
        let function = DomainObjectBody::PyFunction {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("greet(name)".to_string()),
            body: vec![],
        };

        // When / Then
        assert_eq!(function.names().as_slice(), ["greet"]);
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_macros() {
        // Given
        let macro_ = DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single("MAX(a, b)".into()),
            body: vec![],
        };

        // When / Then
        assert_eq!(macro_.names().as_slice(), ["MAX"]);
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_signature_for_object_like_macros() {
        // Given
        let macro_ = DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single("PY_SSIZE_T_MAX".into()),
            body: vec![],
        };

        // When / Then
        assert_eq!(macro_.names().as_slice(), ["PY_SSIZE_T_MAX"]);
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_methods() {
        // Given
        let method = DomainObjectBody::PyMethod {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("Greeter.greet(self, name)".to_string()),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(method.names().as_slice(), ["Greeter.greet"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_methods() {
        // Given
        let method = DomainObjectBody::PyMethod {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("greet(self, name)".to_string()),
            is_classmethod: true,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(method.signature_texts(), ["greet(self, name)"]);
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_classes() {
        // Given
        let class = DomainObjectBody::PyClass {
            module: None,
            signatures: NonEmptyVector::single("Greeter".to_string()),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(class.names().as_slice(), ["Greeter"]);
    }

    #[test]
    fn test_domain_object_body_name_ignores_base_class_list() {
        // Given — base classes shouldn't leak into the referenceable name
        let class = DomainObjectBody::PyClass {
            module: None,
            signatures: NonEmptyVector::single("Greeter(Base)".to_string()),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(class.names().as_slice(), ["Greeter"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_classes() {
        // Given
        let class = DomainObjectBody::PyClass {
            module: None,
            signatures: NonEmptyVector::single("Greeter(Base)".to_string()),
            is_final: true,
            body: vec![],
        };

        // When / Then
        assert_eq!(class.signature_texts(), ["Greeter(Base)"]);
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_exceptions() {
        // Given
        let exception = DomainObjectBody::PyException {
            module: None,
            signatures: NonEmptyVector::single("GreeterError".to_string()),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(exception.names().as_slice(), ["GreeterError"]);
    }

    #[test]
    fn test_domain_object_body_name_ignores_base_class_list_for_exceptions() {
        // Given — base classes shouldn't leak into the referenceable name
        let exception = DomainObjectBody::PyException {
            module: None,
            signatures: NonEmptyVector::single("InvalidNameError(GreeterError)".to_string()),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(exception.names().as_slice(), ["InvalidNameError"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_exceptions() {
        // Given
        let exception = DomainObjectBody::PyException {
            module: None,
            signatures: NonEmptyVector::single("InvalidNameError(GreeterError)".to_string()),
            is_final: true,
            body: vec![],
        };

        // When / Then
        assert_eq!(
            exception.signature_texts(),
            ["InvalidNameError(GreeterError)"]
        );
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_name_for_modules() {
        // Given
        let module = DomainObjectBody::PyModule {
            name: "mypackage.mymodule".to_string(),
            platform: None,
            synopsis: None,
            deprecated: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(module.names().as_slice(), ["mypackage.mymodule"]);
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_name_for_data() {
        // Given
        let data = DomainObjectBody::PyData {
            module: None,
            signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
            type_: None,
            value: None,
            body: vec![],
        };

        // When / Then
        assert_eq!(data.names().as_slice(), ["DEFAULT_TIMEOUT"]);
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_name_for_attributes() {
        // Given
        let attribute = DomainObjectBody::PyAttribute {
            module: None,
            signatures: NonEmptyVector::single("Greeter.name".to_string()),
            type_: None,
            value: None,
            canonical: None,
            body: vec![],
        };

        // When / Then
        assert_eq!(attribute.names().as_slice(), ["Greeter.name"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_functions() {
        // Given
        let function = DomainObjectBody::CFunction {
            signatures: NonEmptyVector::single("int add(int a, int b)".into()),
            body: vec![],
        };

        // When / Then
        assert_eq!(function.signature_texts(), ["int add(int a, int b)"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_macros() {
        // Given
        let macro_ = DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single("MAX(a, b)".into()),
            body: vec![],
        };

        // When / Then
        assert_eq!(macro_.signature_texts(), ["MAX(a, b)"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_bare_name_for_modules() {
        // Given
        let module = DomainObjectBody::PyModule {
            name: "greetings".to_string(),
            platform: None,
            synopsis: None,
            deprecated: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(module.signature_texts(), ["greetings"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_bare_name_for_data() {
        // Given
        let data = DomainObjectBody::PyData {
            module: None,
            signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
            type_: None,
            value: None,
            body: vec![],
        };

        // When / Then
        assert_eq!(data.signature_texts(), ["DEFAULT_TIMEOUT"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_bare_name_for_attributes() {
        // Given
        let attribute = DomainObjectBody::PyAttribute {
            module: None,
            signatures: NonEmptyVector::single("Greeter.name".to_string()),
            type_: None,
            value: None,
            canonical: None,
            body: vec![],
        };

        // When / Then
        assert_eq!(attribute.signature_texts(), ["Greeter.name"]);
    }

    #[test]
    fn test_names_extracts_from_every_signature_not_just_the_first() {
        // Given — a multi-signature function: every entry needs the same
        // name extraction applied, not only the primary.
        let function = DomainObjectBody::PyFunction {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::new(
                "spawnl(mode, file, *args)".to_string(),
                vec!["spawnle(mode, file, *args, env)".to_string()],
            ),
            body: vec![],
        };

        // When
        let names = function.names();

        // Then
        assert_eq!(names.as_slice(), ["spawnl", "spawnle"]);
    }

    #[test]
    fn test_names_strips_base_class_lists_from_every_signature() {
        // Given
        let class = DomainObjectBody::PyClass {
            module: None,
            signatures: NonEmptyVector::new(
                "Greeter(Base)".to_string(),
                vec!["PoliteGreeter(Greeter)".to_string()],
            ),
            is_final: false,
            body: vec![],
        };

        // When
        let names = class.names();

        // Then
        assert_eq!(names.as_slice(), ["Greeter", "PoliteGreeter"]);
    }

    #[test]
    fn test_names_strips_c_return_types_from_every_signature() {
        // Given
        let function = DomainObjectBody::CFunction {
            signatures: NonEmptyVector::new(
                "int add(int a, int b)".into(),
                vec!["PyObject *PyUnicode_FromString(const char *str)".into()],
            ),
            body: vec![],
        };

        // When
        let names = function.names();

        // Then
        assert_eq!(names.as_slice(), ["add", "PyUnicode_FromString"]);
    }

    #[test]
    fn test_names_reads_the_name_parsed_from_a_function_pointer_typedef() {
        // Given — `Doc/c-api/init.rst`'s `Py_tracefunc`, whose name sits
        // inside the `(*…)` group. Reaching it needs the declaration parser;
        // the older "text before the first parenthesis" heuristic reads the
        // return type instead.
        let type_ = DomainObjectBody::CType {
            signatures: NonEmptyVector::single(
                "int (*Py_tracefunc)(PyObject *obj, PyFrameObject *frame, int what, PyObject *arg)"
                    .into(),
            ),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        };

        // When
        let names = type_.names();

        // Then
        assert_eq!(names.as_slice(), ["Py_tracefunc"]);
    }

    #[test]
    fn test_names_reads_the_parsed_name_for_every_c_object_type() {
        // Given — one of each `c` variant, all carrying a signature whose
        // name only a real declarator parse recovers.
        let function = DomainObjectBody::CFunction {
            signatures: NonEmptyVector::single("PyObject *(*getattrofunc)(PyObject *)".into()),
            body: vec![],
        };
        let macro_ = DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single("void (*freefunc)(void *)".into()),
            body: vec![],
        };
        let struct_ = DomainObjectBody::CStruct {
            signatures: NonEmptyVector::single("int (*inquiry)(PyObject *)".into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        };
        let union_ = DomainObjectBody::CUnion {
            signatures: NonEmptyVector::single("Py_ssize_t (*lenfunc)(PyObject *)".into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        };
        let member = DomainObjectBody::CMember {
            signatures: NonEmptyVector::single("int (*visitproc)(PyObject *o, void *arg)".into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        };
        let type_ = DomainObjectBody::CType {
            signatures: NonEmptyVector::single("PyObject *(*unaryfunc)(PyObject *)".into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(function.names().as_slice(), ["getattrofunc"]);
        assert_eq!(macro_.names().as_slice(), ["freefunc"]);
        assert_eq!(struct_.names().as_slice(), ["inquiry"]);
        assert_eq!(union_.names().as_slice(), ["lenfunc"]);
        assert_eq!(member.names().as_slice(), ["visitproc"]);
        assert_eq!(type_.names().as_slice(), ["unaryfunc"]);
    }

    #[test]
    fn test_signature_texts_returns_c_signatures_as_written() {
        // Given — the rendered `<dt>` shows the whole declaration, so the
        // stored text must survive the name extraction untouched.
        let type_ = DomainObjectBody::CType {
            signatures: NonEmptyVector::new(
                "int (*Py_tracefunc)(PyObject *obj, int what)".into(),
                vec!["unsigned long ulong".into()],
            ),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        };

        // When
        let texts = type_.signature_texts();

        // Then
        assert_eq!(
            texts,
            [
                "int (*Py_tracefunc)(PyObject *obj, int what)",
                "unsigned long ulong"
            ]
        );
    }

    #[test]
    fn test_names_and_signature_texts_stay_index_parallel_for_c_objects() {
        // Given — a multi-signature `c:type`, where each name is derived
        // independently of its neighbours.
        let type_ = DomainObjectBody::CType {
            signatures: NonEmptyVector::new(
                "int (*Py_tracefunc)(PyObject *obj)".into(),
                vec!["unsigned long ulong".into(), "FILE".into()],
            ),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        };

        // When
        let names = type_.names();
        let texts = type_.signature_texts();

        // Then
        assert_eq!(names.as_slice().len(), texts.len());
        assert_eq!(names.as_slice(), ["Py_tracefunc", "ulong", "FILE"]);
    }

    #[test]
    fn test_names_uses_bare_signatures_verbatim_for_data() {
        // Given — the confirmed `library/socket.rst` shape: `py:data`
        // signatures are already bare names, so nothing is extracted.
        let data = DomainObjectBody::PyData {
            module: None,
            signatures: NonEmptyVector::new(
                "AF_UNIX".to_string(),
                vec!["AF_INET".to_string(), "AF_INET6".to_string()],
            ),
            type_: None,
            value: None,
            body: vec![],
        };

        // When
        let names = data.names();

        // Then
        assert_eq!(names.as_slice(), ["AF_UNIX", "AF_INET", "AF_INET6"]);
    }

    #[test]
    fn test_signature_texts_returns_every_signature_unextracted() {
        // Given — the `<dt>` display text keeps the full signature, unlike
        // `names()`.
        let function = DomainObjectBody::PyFunction {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::new(
                "spawnl(mode, file, *args)".to_string(),
                vec!["spawnle(mode, file, *args, env)".to_string()],
            ),
            body: vec![],
        };

        // When
        let texts = function.signature_texts();

        // Then
        assert_eq!(
            texts,
            [
                "spawnl(mode, file, *args)",
                "spawnle(mode, file, *args, env)"
            ]
        );
    }

    #[test]
    fn test_modules_always_have_exactly_one_name_and_signature_text() {
        // Given — real Sphinx's `module` directive takes exactly one
        // argument, so `PyModule` can never be multi-signature.
        let module = DomainObjectBody::PyModule {
            name: "xml.etree.ElementTree".to_string(),
            platform: None,
            synopsis: None,
            deprecated: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(module.names().as_slice(), ["xml.etree.ElementTree"]);
        assert_eq!(module.signature_texts(), ["xml.etree.ElementTree"]);
    }

    #[test]
    fn test_names_and_signature_texts_stay_index_parallel() {
        // Given — the renderer zips the two to pair each anchor with its
        // display text, so they must have matching lengths and order.
        let method = DomainObjectBody::PyMethod {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::new(
                "ZipFile.open(name)".to_string(),
                vec!["ZipFile.read(name)".to_string()],
            ),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When
        let names = method.names();
        let texts = method.signature_texts();

        // Then
        assert_eq!(names.as_slice().len(), texts.len());
        assert_eq!(names.as_slice(), ["ZipFile.open", "ZipFile.read"]);
        assert_eq!(texts, ["ZipFile.open(name)", "ZipFile.read(name)"]);
    }

    #[test]
    fn test_domain_object_body_body_returns_shared_body_for_every_variant() {
        // Given
        let paragraph = Node::Paragraph(vec![InlineNode::Text("hello".to_string())]);
        let function = DomainObjectBody::PyFunction {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("greet(name)".to_string()),
            body: vec![paragraph.clone()],
        };
        let module = DomainObjectBody::PyModule {
            name: "greetings".to_string(),
            platform: None,
            synopsis: None,
            deprecated: false,
            body: vec![paragraph.clone()],
        };
        let data = DomainObjectBody::PyData {
            module: None,
            signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
            type_: None,
            value: None,
            body: vec![paragraph.clone()],
        };
        let attribute = DomainObjectBody::PyAttribute {
            module: None,
            signatures: NonEmptyVector::single("Greeter.name".to_string()),
            type_: None,
            value: None,
            canonical: None,
            body: vec![paragraph.clone()],
        };
        let c_function = DomainObjectBody::CFunction {
            signatures: NonEmptyVector::single("int add(int a, int b)".into()),
            body: vec![paragraph.clone()],
        };
        let c_macro = DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single("MAX(a, b)".into()),
            body: vec![paragraph.clone()],
        };
        let method = DomainObjectBody::PyMethod {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("greet(self, name)".to_string()),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![paragraph.clone()],
        };
        let class = DomainObjectBody::PyClass {
            module: None,
            signatures: NonEmptyVector::single("Greeter".to_string()),
            is_final: false,
            body: vec![paragraph.clone()],
        };
        let exception = DomainObjectBody::PyException {
            module: None,
            signatures: NonEmptyVector::single("GreeterError".to_string()),
            is_final: false,
            body: vec![paragraph.clone()],
        };

        // When / Then
        assert_eq!(function.body(), std::slice::from_ref(&paragraph));
        assert_eq!(module.body(), std::slice::from_ref(&paragraph));
        assert_eq!(data.body(), std::slice::from_ref(&paragraph));
        assert_eq!(attribute.body(), std::slice::from_ref(&paragraph));
        assert_eq!(c_function.body(), std::slice::from_ref(&paragraph));
        assert_eq!(c_macro.body(), std::slice::from_ref(&paragraph));
        assert_eq!(method.body(), std::slice::from_ref(&paragraph));
        assert_eq!(class.body(), std::slice::from_ref(&paragraph));
        assert_eq!(exception.body(), std::slice::from_ref(&paragraph));
    }

    #[test]
    fn test_body_mut_allows_in_place_rewrite() {
        // Given
        let mut function = DomainObjectBody::PyFunction {
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("foo()".to_string()),
            body: vec![Node::Comment],
        };

        // When
        function.body_mut().push(Node::Comment);

        // Then
        assert_eq!(function.body(), &[Node::Comment, Node::Comment]);
    }

    fn plain_c_member(signature: &str) -> DomainObjectBody {
        DomainObjectBody::CMember {
            signatures: NonEmptyVector::single(signature.into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        }
    }

    fn plain_c_struct(signature: &str) -> DomainObjectBody {
        DomainObjectBody::CStruct {
            signatures: NonEmptyVector::single(signature.into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        }
    }

    fn plain_c_union(signature: &str) -> DomainObjectBody {
        DomainObjectBody::CUnion {
            signatures: NonEmptyVector::single(signature.into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        }
    }

    fn plain_c_type(signature: &str) -> DomainObjectBody {
        DomainObjectBody::CType {
            signatures: NonEmptyVector::single(signature.into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        }
    }

    #[test]
    fn test_domain_object_body_object_type_for_c_struct_union_member() {
        // Given / When / Then
        assert_eq!(
            plain_c_struct("Data").object_type(),
            ObjectType::C(CObjectType::Struct)
        );
        assert_eq!(
            plain_c_union("Number").object_type(),
            ObjectType::C(CObjectType::Union)
        );
        assert_eq!(
            plain_c_member("count").object_type(),
            ObjectType::C(CObjectType::Member)
        );
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_signature_for_c_struct() {
        // Given — struct/union tags have no parens, like object-like macros.
        let struct_ = plain_c_struct("Data");

        // When / Then
        assert_eq!(struct_.names().as_slice(), ["Data"]);
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_signature_for_c_union() {
        // Given
        let union_ = plain_c_union("Number");

        // When / Then
        assert_eq!(union_.names().as_slice(), ["Number"]);
    }

    #[test]
    fn test_domain_object_body_name_extracts_dotted_flat_name_for_c_member() {
        // Given — the real CPython-docs shape: a member declared with its
        // enclosing struct's name baked into the signature, no `.. c:struct::`
        // wrapper at all.
        let member = plain_c_member("PyObject *PyTypeObject.tp_bases");

        // When / Then
        assert_eq!(member.names().as_slice(), ["PyTypeObject.tp_bases"]);
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_name_for_undotted_c_member() {
        // Given — a member written bare, e.g. nested inside `.. c:struct::`.
        let member = plain_c_member("int count");

        // When / Then
        assert_eq!(member.names().as_slice(), ["count"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_c_struct_union_member() {
        // Given
        let struct_ = plain_c_struct("Data");
        let union_ = plain_c_union("Number");
        let member = plain_c_member("PyObject *PyTypeObject.tp_bases");

        // When / Then
        assert_eq!(struct_.signature_texts(), ["Data"]);
        assert_eq!(union_.signature_texts(), ["Number"]);
        assert_eq!(
            member.signature_texts(),
            ["PyObject *PyTypeObject.tp_bases"]
        );
    }

    #[test]
    fn test_deduce_local_scope_lends_all_new_segments_for_c_struct_and_union() {
        // Given — real lexical nesting, like `py:class`.
        let struct_ = plain_c_struct("Data");
        let union_ = plain_c_union("Number");

        // When / Then
        assert_eq!(
            struct_.deduce_local_scope(&["Data".to_string()]),
            vec!["Data".to_string()]
        );
        assert_eq!(
            union_.deduce_local_scope(&["Number".to_string()]),
            vec!["Number".to_string()]
        );
    }

    #[test]
    fn test_deduce_local_scope_returns_empty_for_c_member() {
        // Given — nothing nests under a `c:member` in real Sphinx.
        let member = plain_c_member("count");

        // When
        let scope = member.deduce_local_scope(&["count".to_string()]);

        // Then
        assert!(scope.is_empty());
    }

    #[test]
    fn test_domain_object_body_body_returns_shared_body_for_c_struct_union_member() {
        // Given
        let paragraph = Node::Paragraph(vec![InlineNode::Text("hello".to_string())]);
        let struct_ = DomainObjectBody::CStruct {
            signatures: NonEmptyVector::single("Data".into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![paragraph.clone()],
        };
        let union_ = DomainObjectBody::CUnion {
            signatures: NonEmptyVector::single("Number".into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![paragraph.clone()],
        };
        let member = DomainObjectBody::CMember {
            signatures: NonEmptyVector::single("count".into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![paragraph.clone()],
        };

        // When / Then
        assert_eq!(struct_.body(), std::slice::from_ref(&paragraph));
        assert_eq!(union_.body(), std::slice::from_ref(&paragraph));
        assert_eq!(member.body(), std::slice::from_ref(&paragraph));
    }

    #[test]
    fn test_no_index_is_false_by_default_for_pre_existing_variants() {
        // Given / When / Then
        assert!(
            !DomainObjectBody::CFunction {
                signatures: NonEmptyVector::single("int add(int a, int b)".into()),
                body: vec![],
            }
            .no_index()
        );
    }

    #[test]
    fn test_no_index_reflects_flag_for_c_member() {
        // Given
        let mut member = plain_c_member("count");

        // When / Then
        assert!(!member.no_index());
        if let DomainObjectBody::CMember { no_index, .. } = &mut member {
            *no_index = true;
        }
        assert!(member.no_index());
    }

    #[test]
    fn test_no_index_entry_is_true_when_no_index_entry_flag_set() {
        // Given
        let member = DomainObjectBody::CMember {
            signatures: NonEmptyVector::single("count".into()),
            no_index: false,
            no_index_entry: true,
            no_contents_entry: false,
            body: vec![],
        };

        // When / Then
        assert!(member.no_index_entry());
    }

    #[test]
    fn test_no_index_entry_is_implied_by_no_index() {
        // Given — real Sphinx's `no-index` implies `no-index-entry`.
        let member = DomainObjectBody::CMember {
            signatures: NonEmptyVector::single("count".into()),
            no_index: true,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        };

        // When / Then
        assert!(member.no_index_entry());
    }

    #[test]
    fn test_domain_object_body_object_type_for_c_type() {
        // Given / When / Then
        assert_eq!(
            plain_c_type("PyMemAllocatorDomain").object_type(),
            ObjectType::C(CObjectType::Type)
        );
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_signature_for_c_type() {
        // Given — bare typedef alias, no parens.
        let type_ = plain_c_type("PyMemAllocatorDomain");

        // When / Then
        assert_eq!(type_.names().as_slice(), ["PyMemAllocatorDomain"]);
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_two_token_typedef_signature() {
        // Given — real Sphinx's `type name` typedef-alias form.
        let type_ = plain_c_type("unsigned long ulong");

        // When / Then
        assert_eq!(type_.names().as_slice(), ["ulong"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_c_type() {
        // Given
        let type_ = plain_c_type("unsigned long ulong");

        // When / Then
        assert_eq!(type_.signature_texts(), ["unsigned long ulong"]);
    }

    #[test]
    fn test_deduce_local_scope_lends_all_new_segments_for_c_type() {
        // Given — nesting under `c:type` scope-qualifies exactly like
        // `c:struct`/`c:union`, confirmed against real Sphinx's C-domain
        // docs (nesting is generic to any declaration, not struct/union
        // specific).
        let type_ = plain_c_type("PyMemAllocatorDomain");

        // When
        let scope = type_.deduce_local_scope(&["PyMemAllocatorDomain".to_string()]);

        // Then
        assert_eq!(scope, vec!["PyMemAllocatorDomain".to_string()]);
    }

    #[test]
    fn test_domain_object_body_body_returns_shared_body_for_c_type() {
        // Given
        let paragraph = Node::Paragraph(vec![InlineNode::Text("hello".to_string())]);
        let type_ = DomainObjectBody::CType {
            signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![paragraph.clone()],
        };

        // When / Then
        assert_eq!(type_.body(), std::slice::from_ref(&paragraph));
    }

    #[test]
    fn test_no_index_reflects_flag_for_c_type() {
        // Given
        let mut type_ = plain_c_type("PyMemAllocatorDomain");

        // When / Then
        assert!(!type_.no_index());
        if let DomainObjectBody::CType { no_index, .. } = &mut type_ {
            *no_index = true;
        }
        assert!(type_.no_index());
    }

    #[test]
    fn test_no_index_entry_is_true_when_no_index_entry_flag_set_for_c_type() {
        // Given
        let type_ = DomainObjectBody::CType {
            signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
            no_index: false,
            no_index_entry: true,
            no_contents_entry: false,
            body: vec![],
        };

        // When / Then
        assert!(type_.no_index_entry());
    }

    #[test]
    fn test_no_index_entry_is_implied_by_no_index_for_c_type() {
        // Given
        let type_ = DomainObjectBody::CType {
            signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
            no_index: true,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        };

        // When / Then
        assert!(type_.no_index_entry());
    }

    #[test]
    fn test_no_contents_entry_reflects_flag_for_c_type() {
        // Given
        let type_with_flag = DomainObjectBody::CType {
            signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: true,
            body: vec![],
        };

        // When / Then
        assert!(type_with_flag.no_contents_entry());
        assert!(!plain_c_type("PyMemAllocatorDomain").no_contents_entry());
    }

    #[test]
    fn test_no_contents_entry_reflects_flag_for_c_struct() {
        // Given
        let struct_with_flag = DomainObjectBody::CStruct {
            signatures: NonEmptyVector::single("Data".into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: true,
            body: vec![],
        };

        // When / Then
        assert!(struct_with_flag.no_contents_entry());
        assert!(!plain_c_struct("Data").no_contents_entry());
    }

    #[test]
    fn test_module_override_is_none_by_default_for_every_py_variant_that_carries_it() {
        // Given / When / Then
        assert_eq!(
            DomainObjectBody::PyFunction {
                module: None,
                is_decorator: false,
                signatures: NonEmptyVector::single("greet(name)".to_string()),
                body: vec![],
            }
            .module_override(),
            None
        );
        assert_eq!(
            DomainObjectBody::PyMethod {
                module: None,
                is_decorator: false,
                signatures: NonEmptyVector::single("greet(self, name)".to_string()),
                is_classmethod: false,
                is_staticmethod: false,
                is_abstractmethod: false,
                is_async: false,
                body: vec![],
            }
            .module_override(),
            None
        );
        assert_eq!(
            DomainObjectBody::PyClass {
                module: None,
                signatures: NonEmptyVector::single("Greeter".to_string()),
                is_final: false,
                body: vec![],
            }
            .module_override(),
            None
        );
        assert_eq!(
            DomainObjectBody::PyException {
                module: None,
                signatures: NonEmptyVector::single("GreeterError".to_string()),
                is_final: false,
                body: vec![],
            }
            .module_override(),
            None
        );
        assert_eq!(
            DomainObjectBody::PyData {
                module: None,
                signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
                type_: None,
                value: None,
                body: vec![],
            }
            .module_override(),
            None
        );
        assert_eq!(
            DomainObjectBody::PyAttribute {
                module: None,
                signatures: NonEmptyVector::single("Greeter.name".to_string()),
                type_: None,
                value: None,
                canonical: None,
                body: vec![],
            }
            .module_override(),
            None
        );
    }

    #[test]
    fn test_module_override_reflects_the_field_for_every_py_variant_that_carries_it() {
        // Given / When / Then
        assert_eq!(
            DomainObjectBody::PyFunction {
                module: Some("ctypes.util".to_string()),
                is_decorator: false,
                signatures: NonEmptyVector::single("find_library(name)".to_string()),
                body: vec![],
            }
            .module_override(),
            Some("ctypes.util")
        );
        assert_eq!(
            DomainObjectBody::PyMethod {
                module: Some("multiprocessing.managers".to_string()),
                is_decorator: false,
                signatures: NonEmptyVector::single("get_server()".to_string()),
                is_classmethod: false,
                is_staticmethod: false,
                is_abstractmethod: false,
                is_async: false,
                body: vec![],
            }
            .module_override(),
            Some("multiprocessing.managers")
        );
        assert_eq!(
            DomainObjectBody::PyClass {
                module: Some("multiprocessing.managers".to_string()),
                signatures: NonEmptyVector::single("SharedMemoryManager".to_string()),
                is_final: false,
                body: vec![],
            }
            .module_override(),
            Some("multiprocessing.managers")
        );
        assert_eq!(
            DomainObjectBody::PyException {
                module: Some("mymodule.other".to_string()),
                signatures: NonEmptyVector::single("GreeterError".to_string()),
                is_final: false,
                body: vec![],
            }
            .module_override(),
            Some("mymodule.other")
        );
        assert_eq!(
            DomainObjectBody::PyData {
                module: Some("ctypes.util".to_string()),
                signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
                type_: None,
                value: None,
                body: vec![],
            }
            .module_override(),
            Some("ctypes.util")
        );
        assert_eq!(
            DomainObjectBody::PyAttribute {
                module: Some("mymodule.other".to_string()),
                signatures: NonEmptyVector::single("Greeter.name".to_string()),
                type_: None,
                value: None,
                canonical: None,
                body: vec![],
            }
            .module_override(),
            Some("mymodule.other")
        );
    }

    #[test]
    fn test_module_override_is_always_none_for_py_module_and_every_c_variant() {
        // Given — `py:module` is not a `PyObject` and has no `:module:`
        // option (it *is* the module declaration); no `c`-domain object
        // carries the option either.
        let module = DomainObjectBody::PyModule {
            name: "greetings".to_string(),
            platform: None,
            synopsis: None,
            deprecated: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(module.module_override(), None);
        assert_eq!(
            DomainObjectBody::CFunction {
                signatures: NonEmptyVector::single("int add(int a, int b)".into()),
                body: vec![],
            }
            .module_override(),
            None
        );
        assert_eq!(
            DomainObjectBody::CMacro {
                signatures: NonEmptyVector::single("MAX(a, b)".into()),
                body: vec![],
            }
            .module_override(),
            None
        );
        assert_eq!(plain_c_struct("Data").module_override(), None);
        assert_eq!(plain_c_union("Number").module_override(), None);
        assert_eq!(plain_c_member("count").module_override(), None);
        assert_eq!(plain_c_type("PyMemAllocatorDomain").module_override(), None);
    }
}
