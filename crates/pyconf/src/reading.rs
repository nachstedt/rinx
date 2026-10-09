//! What reading a module found: the names it binds, and what it binds them
//! to as far as that can be told without running it.

use std::collections::BTreeMap;

use crate::position::Span;
use crate::token::SyntaxError;
use crate::value::Value;

/// What [`read_module`](crate::read_module) found in one module.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModuleReading {
    /// Every name the module binds at its top level, by name.
    pub bindings: BTreeMap<String, Binding>,
    /// Every `from … import *`, which may bind any name at all.
    pub wildcard_imports: Vec<Span>,
    /// Why the module could not be read to its end, if it could not. What
    /// was read before stands.
    pub error: Option<SyntaxError>,
}

impl ModuleReading {
    /// What the module binds `name` to, if it binds it.
    #[must_use]
    pub fn binding(&self, name: &str) -> Option<&Binding> {
        self.bindings.get(name)
    }
}

/// One name's last top-level binding, and what the module does to it
/// afterwards.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    /// Where the name is bound: the target of its assignment.
    pub target: Span,
    pub value: BoundValue,
    /// Every statement after the binding that may change the value without
    /// rebinding the name at the top level — `+=`, `.append(…)`, an item
    /// assigned, an assignment inside an `if` — none of which the reader
    /// applies.
    pub modifications: Vec<Span>,
}

/// What a name is bound to.
#[derive(Debug, Clone, PartialEq)]
pub enum BoundValue {
    /// A literal, read.
    Literal(Value),
    /// Anything else — a computed value, an import, a function, a value
    /// bound only on some paths — over the source range that binds it.
    Unread(Span),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::Position;

    #[test]
    fn test_binding_finds_a_name_by_its_spelling() {
        // Given
        let span = Span {
            start: Position::START,
            end: Position::START,
        };
        let mut reading = ModuleReading::default();
        reading.bindings.insert(
            "project".to_string(),
            Binding {
                target: span,
                value: BoundValue::Unread(span),
                modifications: Vec::new(),
            },
        );

        // When / Then
        assert!(reading.binding("project").is_some());
        assert!(reading.binding("Project").is_none());
    }
}
