//! Deserializing a literal read from `conf.py` as any serde type.
//!
//! A setting written as a Python literal — `needs_types = [dict(…)]` aside,
//! a list of dicts — then reads into the same type a TOML file deserializes
//! into, so a setting means one thing whichever file it is written in. Dicts
//! are visited in written order, as Python keeps them.

use std::fmt;

use rinx_pyconf::{Value, ValueKind};
use serde::de::value::{MapDeserializer, SeqDeserializer};
use serde::de::{self, Deserializer, IntoDeserializer, Visitor};

/// A literal from `conf.py`, as a serde [`Deserializer`].
#[derive(Clone, Copy)]
pub struct PythonValue<'a>(pub &'a Value);

/// Why a literal is not of the shape a type asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonValueError(pub String);

impl fmt::Display for PythonValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PythonValueError {}

impl de::Error for PythonValueError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

impl<'de> IntoDeserializer<'de, PythonValueError> for PythonValue<'de> {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self {
        self
    }
}

impl<'de> Deserializer<'de> for PythonValue<'de> {
    type Error = PythonValueError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match &self.0.kind {
            ValueKind::Str(text) => visitor.visit_borrowed_str(text),
            ValueKind::Int(number) => visitor.visit_i64(*number),
            ValueKind::Float(number) => visitor.visit_f64(*number),
            ValueKind::Bool(value) => visitor.visit_bool(*value),
            ValueKind::None => visitor.visit_unit(),
            ValueKind::List(items) | ValueKind::Tuple(items) | ValueKind::Set(items) => {
                let mut sequence = SeqDeserializer::new(items.iter().map(PythonValue));
                let value = visitor.visit_seq(&mut sequence)?;
                sequence.end()?;
                Ok(value)
            }
            ValueKind::Dict(entries) => {
                let mut map = MapDeserializer::new(
                    entries
                        .iter()
                        .map(|(key, value)| (PythonValue(key), PythonValue(value))),
                );
                let value = visitor.visit_map(&mut map)?;
                map.end()?;
                Ok(value)
            }
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.0.kind {
            ValueKind::None => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::Deserialize;

    use super::*;

    fn literal(text: &str) -> Value {
        let module = rinx_pyconf::read_module(&format!("x = {text}\n"));
        match &module.binding("x").expect("bound").value {
            rinx_pyconf::BoundValue::Literal(value) => value.clone(),
            rinx_pyconf::BoundValue::Dict { .. } | rinx_pyconf::BoundValue::Unread(_) => {
                panic!("{text} is no literal")
            }
        }
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Shape {
        name: String,
        count: i64,
        ratio: f64,
        on: bool,
        tags: Vec<String>,
        pair: (String, String),
        missing: Option<String>,
        nothing: Option<String>,
    }

    #[test]
    fn test_a_dict_deserializes_as_a_struct() {
        // Given
        let value = literal(
            "{'name': 'a', 'count': 2, 'ratio': 0.5, 'on': True, 'tags': ['x', 'y'], 'pair': ('p', 'q'), 'nothing': None}",
        );

        // When
        let shape = Shape::deserialize(PythonValue(&value)).expect("deserializes");

        // Then
        assert_eq!(
            shape,
            Shape {
                name: "a".into(),
                count: 2,
                ratio: 0.5,
                on: true,
                tags: vec!["x".into(), "y".into()],
                pair: ("p".into(), "q".into()),
                missing: None,
                nothing: None,
            }
        );
    }

    #[test]
    fn test_a_value_of_the_wrong_kind_is_an_error() {
        // Given
        let value = literal("['a']");

        // When
        let error = BTreeMap::<String, String>::deserialize(PythonValue(&value))
            .expect_err("a list is no dict");

        // Then
        assert!(error.to_string().contains("sequence"), "{error}");
    }

    #[test]
    fn test_a_dict_is_visited_in_written_order() {
        // Given
        let value = literal("{'b': 1, 'a': 2}");

        // When
        let table: rinx_entity::needs_conf::OrderedTable<i64> =
            Deserialize::deserialize(PythonValue(&value)).expect("deserializes");

        // Then
        let keys: Vec<&str> = table.iter().map(|(key, _)| key).collect();
        assert_eq!(keys, ["b", "a"]);
    }
}
