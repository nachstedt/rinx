//! A table that keeps its entries in the order they were written.
//!
//! sphinx-needs reads its configuration into Python dicts, which keep
//! insertion order, and that order shows: a need's fields are listed in it.
//! `serde_json`'s and `toml`'s own maps sort their keys, so the conversion
//! reads every order-bearing table through this instead.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};

/// Entries in the order the input wrote them; a key written twice keeps its
/// first position and its last value, as a Python dict does.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderedTable<V>(Vec<(String, V)>);

impl<V> Default for OrderedTable<V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<V> OrderedTable<V> {
    /// Whether the table holds no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The entries, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }

    /// The value at `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&V> {
        self.0
            .iter()
            .find(|(written, _)| written == key)
            .map(|(_, value)| value)
    }

    /// Whether `key` has an entry.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Sets `key` to `value`: in place when it is already there, else last.
    pub fn insert(&mut self, key: String, value: V) {
        match self.0.iter_mut().find(|(written, _)| *written == key) {
            Some((_, slot)) => *slot = value,
            None => self.0.push((key, value)),
        }
    }
}

impl<V> FromIterator<(String, V)> for OrderedTable<V> {
    fn from_iter<I: IntoIterator<Item = (String, V)>>(entries: I) -> Self {
        let mut table = Self::default();
        for (key, value) in entries {
            table.insert(key, value);
        }
        table
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for OrderedTable<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(OrderedTableVisitor(PhantomData))
    }
}

struct OrderedTableVisitor<V>(PhantomData<V>);

impl<'de, V: Deserialize<'de>> Visitor<'de> for OrderedTableVisitor<V> {
    type Value = OrderedTable<V>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a table")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
        let mut table = OrderedTable::default();
        while let Some((key, value)) = access.next_entry::<String, V>()? {
            table.insert(key, value);
        }
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Wrapper {
        t: OrderedTable<i64>,
    }

    #[test]
    fn test_ordered_table_keeps_the_order_toml_wrote() {
        // Given keys written out of alphabetical order
        let text = "[t]\nzeta = 1\nalpha = 2\nmid = 3\n";

        // When
        let table = toml::from_str::<Wrapper>(text).expect("valid TOML").t;

        // Then
        let keys: Vec<&str> = table.iter().map(|(key, _)| key).collect();
        assert_eq!(keys, ["zeta", "alpha", "mid"]);
    }

    #[test]
    fn test_ordered_table_keeps_the_order_json_wrote() {
        // Given
        let text = r#"{"b": 1, "a": 2}"#;

        // When
        let table: OrderedTable<i64> = serde_json::from_str(text).expect("valid JSON");

        // Then
        let keys: Vec<&str> = table.iter().map(|(key, _)| key).collect();
        assert_eq!(keys, ["b", "a"]);
    }

    #[test]
    fn test_insert_replaces_a_key_in_place() {
        // Given
        let mut table: OrderedTable<i64> = [("a".to_string(), 1), ("b".to_string(), 2)]
            .into_iter()
            .collect();

        // When
        table.insert("a".to_string(), 3);
        table.insert("c".to_string(), 4);

        // Then
        let entries: Vec<(&str, i64)> = table.iter().map(|(key, value)| (key, *value)).collect();
        assert_eq!(entries, [("a", 3), ("b", 2), ("c", 4)]);
        assert!(table.contains_key("c"));
        assert_eq!(table.get("z"), None);
        assert!(!table.is_empty());
    }
}
