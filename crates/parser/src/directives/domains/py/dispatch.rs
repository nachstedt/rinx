//! The `py`-domain object-type dispatch, and the `:module:` option every
//! `py` object description but `py:module` takes.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::options::OptionLine;
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
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
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

/// Reads `line` into `module` if it is the `:module:` option every `py`
/// object description but `py:module` itself takes, and says whether it was.
///
/// A bare `:module:` reads as `Some("")`, which is meaningful: Sphinx's
/// falsy-`modname` check makes it un-qualify the object.
pub(super) fn read_module_option(line: &OptionLine, module: &mut Option<String>) -> bool {
    if line.name != "module" {
        return false;
    }
    *module = Some(line.value.clone());
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directives::options::scan_option_lines;

    #[test]
    fn test_read_module_option_reads_a_value_and_a_bare_option() {
        // Given
        let lines: Vec<String> = [":module: ctypes", ":module:", ":final:"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let (option_lines, _) = scan_option_lines(&lines);
        let mut module = None;

        // When
        let read: Vec<(bool, Option<String>)> = option_lines
            .iter()
            .map(|line| (read_module_option(line, &mut module), module.clone()))
            .collect();

        // Then
        assert_eq!(
            read,
            [
                (true, Some("ctypes".to_string())),
                (true, Some(String::new())),
                (false, Some(String::new())),
            ]
        );
    }
}
