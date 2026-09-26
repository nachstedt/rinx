use serde::{Deserialize, Serialize};

use crate::c_signature::CSignature;
use crate::node::Node;
use crate::non_empty_vector::NonEmptyVector;
use crate::object_naming::{
    extract_option_name, extract_python_object_name, split_option_line_specs,
};
use crate::object_type::CObjectType;
use crate::object_type::ObjectType;
use crate::object_type::PyObjectType;
use crate::object_type::StdObjectType;

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
        /// and stored for round-tripping, but rinx has no such
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
    /// `rinx_scope` crate, which depends on this one, not the other
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
    /// rinx has no such listing for domain objects yet, so this has
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
mod identity_tests;
#[cfg(test)]
mod scope_tests;
#[cfg(test)]
mod state_tests;
