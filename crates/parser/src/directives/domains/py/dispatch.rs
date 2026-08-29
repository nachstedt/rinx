//! The `py`-domain object-type dispatch, and the `:module:` option line
//! every `py:*` object-description directive shares.

use crate::context::ParseCtx;
use rusty_sphinx_ast::{DomainObjectBody, NonEmptyVector};

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
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    match object_type {
        DirectiveObjectType::PyFunction => parse_py_function(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
            false,
        ),
        DirectiveObjectType::PyDecorator => parse_py_function(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
            true,
        ),
        DirectiveObjectType::PyModule => parse_py_module(
            signatures.first().clone(),
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        ),
        DirectiveObjectType::PyData => {
            parse_py_data(signatures, body_lines, adornment_order, diagnostics, ctx)
        }
        DirectiveObjectType::PyMethod
        | DirectiveObjectType::PyClassmethod
        | DirectiveObjectType::PyStaticmethod
        | DirectiveObjectType::PyDecoratorMethod => parse_py_method(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
            ForcedMethodFlags::for_directive(object_type),
        ),
        DirectiveObjectType::PyClass => {
            parse_py_class(signatures, body_lines, adornment_order, diagnostics, ctx)
        }
        DirectiveObjectType::PyException => {
            parse_py_exception(signatures, body_lines, adornment_order, diagnostics, ctx)
        }
        DirectiveObjectType::PyAttribute => {
            parse_py_attribute(signatures, body_lines, adornment_order, diagnostics, ctx)
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

/// Parses a single leading option line as `:module:`, if it is one, e.g.
/// `":module: multiprocessing.managers"` -> `Some("multiprocessing.managers")`
/// (or `Some("")` for a bare `:module:` with no value). Shared by every
/// per-object-type extractor in this domain, since real Sphinx's `:module:`
/// option is common to every `py:*` object-description directive except
/// `py:module` itself (which is not a `PyObject` and has its own, disjoint
/// `platform`/`synopsis`/`deprecated` option set — see
/// [`super::module::extract_module_options`]).
pub(super) fn parse_module_option_line(trimmed: &str) -> Option<String> {
    trimmed
        .strip_prefix(":module:")
        .map(|rest| rest.trim().to_string())
}
