//! What the reader's tests ask of a reading.

use crate::reading::{BoundValue, ModuleReading};
use crate::value::ValueKind;

pub(super) use super::read_module;

/// The literal `name` is bound to, if it is bound to one.
pub(super) fn literal(reading: &ModuleReading, name: &str) -> Option<ValueKind> {
    match &reading.binding(name)?.value {
        BoundValue::Literal(value) => Some(value.kind.clone()),
        BoundValue::Dict { .. } | BoundValue::Unread(_) => None,
    }
}

/// The keys of the dict display `name` is bound to, when its keys are
/// literals and its values are not all.
pub(super) fn dict_keys(reading: &ModuleReading, name: &str) -> Option<Vec<ValueKind>> {
    match &reading.binding(name)?.value {
        BoundValue::Dict { keys, .. } => Some(keys.iter().map(|key| key.kind.clone()).collect()),
        BoundValue::Literal(_) | BoundValue::Unread(_) => None,
    }
}

/// Whether `name` is bound to something the reader did not read.
pub(super) fn is_unread(reading: &ModuleReading, name: &str) -> bool {
    matches!(
        reading.binding(name).map(|binding| &binding.value),
        Some(BoundValue::Unread(_))
    )
}

/// The lines of the statements modifying `name` after its binding.
pub(super) fn modification_lines(reading: &ModuleReading, name: &str) -> Vec<usize> {
    reading.binding(name).map_or_else(Vec::new, |binding| {
        binding
            .modifications
            .iter()
            .map(|span| span.start.line)
            .collect()
    })
}

/// The string `text` as a literal's kind.
pub(super) fn string(text: &str) -> ValueKind {
    ValueKind::Str(text.to_string())
}
