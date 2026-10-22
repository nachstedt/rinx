//! The `py`-domain object-type dispatch, and the `:module:` option every
//! `py` object description but `py:module` takes.

use crate::diagnostics::Diagnostics;
use crate::directives::invocation::DirectiveInvocation;
use crate::directives::options::directive_options;
use rinx_ast::{DomainObjectBody, NonEmptyVector};

use crate::headings::Adornment;

use super::super::object_type::DirectiveObjectType;
use super::attribute::parse_py_attribute;
use super::class::{parse_py_class, parse_py_exception};
use super::data::parse_py_data;
use super::function::parse_py_function;
use super::method::{ForcedMethodFlags, parse_py_method};
use super::module::parse_py_module;

/// Dispatches the ten `py`-domain object types (including the
/// `classmethod`/`staticmethod`/`decorator`/`decoratormethod` directive-name
/// aliases, which carry no `ast::ObjectType`/[`DomainObjectBody`] variant of
/// their own) to their respective parsers. Only ever called with a non-`c`,
/// non-`std` variant (enforced by [`super::super::object::parse_domain_object`]'s
/// own match arm), so those variants are unreachable here.
pub(crate) fn parse_py_domain_object(
    object_type: DirectiveObjectType,
    signatures: NonEmptyVector<String>,
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> DomainObjectBody {
    match object_type {
        DirectiveObjectType::PyFunction => {
            parse_py_function(signatures, invocation, adornment_order, diagnostics, false)
        }
        DirectiveObjectType::PyDecorator => {
            parse_py_function(signatures, invocation, adornment_order, diagnostics, true)
        }
        DirectiveObjectType::PyModule => parse_py_module(
            signatures.first().clone(),
            invocation,
            adornment_order,
            diagnostics,
        ),
        DirectiveObjectType::PyData => {
            parse_py_data(signatures, invocation, adornment_order, diagnostics)
        }
        DirectiveObjectType::PyMethod
        | DirectiveObjectType::PyClassmethod
        | DirectiveObjectType::PyStaticmethod
        | DirectiveObjectType::PyDecoratorMethod => parse_py_method(
            signatures,
            invocation,
            adornment_order,
            diagnostics,
            ForcedMethodFlags::for_directive(object_type),
        ),
        DirectiveObjectType::PyClass => {
            parse_py_class(signatures, invocation, adornment_order, diagnostics)
        }
        DirectiveObjectType::PyException => {
            parse_py_exception(signatures, invocation, adornment_order, diagnostics)
        }
        DirectiveObjectType::PyAttribute => {
            parse_py_attribute(signatures, invocation, adornment_order, diagnostics)
        }
        DirectiveObjectType::CFunction
        | DirectiveObjectType::CMacro
        | DirectiveObjectType::CStruct
        | DirectiveObjectType::CUnion
        | DirectiveObjectType::CMember
        | DirectiveObjectType::CType
        | DirectiveObjectType::StdCmdoption => {
            unreachable!("parse_py_domain_object called with a non-py object type")
        }
    }
}

directive_options! {
    /// The options every `py` object description but `py:module` itself
    /// takes: Sphinx's `PyObject`'s.
    pub(super) enum PyObjectOption {
        /// `:module:`.
        Module => (["module"], Text, "The module the object belongs to, if not the current one."),
        _ => (["annotation"], Text, "Text shown after the signature."),
        _ => (["single-line-parameter-list"], Flag, "Writes the parameters on one line, however long."),
        _ => (["single-line-type-parameter-list"], Flag, "Writes the type parameters on one line, however long."),
    }
}
