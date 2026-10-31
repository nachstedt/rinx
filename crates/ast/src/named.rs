//! A value together with a handle to where the author wrote it.

use serde::{Deserialize, Serialize};

/// Which written name of one parse a [`Named`] value is: the row of its
/// [`SourceMap`](crate::SourceMap) holding the name's characters.
///
/// Only [`SourceMap::record_name`](crate::SourceMap::record_name) makes one,
/// so an id always has a row in the map it was recorded in. It is part of the
/// `.ast` — a build phase reading one back holds the id but no map, and so
/// cannot ask where the name was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NameId(u32);

impl NameId {
    /// The id of the `index`th name a source map records.
    pub(crate) const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The row of the source map this id names.
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A name the author wrote — a label, a directive's `:name:`, an equation's
/// `:label:`, an entity's `:id:` — with the id under which the parse recorded
/// exactly its characters.
///
/// The characters are the editor's alone: a rename rewrites them and nothing
/// around them, and only the [`SourceMap`](crate::SourceMap) beside the parse
/// knows them. A value read from no source line, such as a generated row or
/// an imported need, has no id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Named<T> {
    value: T,
    id: Option<NameId>,
}

impl<T> Named<T> {
    /// `value`, written where `id` was recorded — `None` when it was read
    /// from no source line.
    #[must_use]
    pub const fn new(value: T, id: Option<NameId>) -> Self {
        Self { value, id }
    }

    /// `value`, written nowhere in particular: built by code, or read from no
    /// source line.
    #[must_use]
    pub const fn unplaced(value: T) -> Self {
        Self::new(value, None)
    }

    /// The value.
    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// The value, without where it was written.
    #[must_use]
    pub fn into_value(self) -> T {
        self.value
    }

    /// The id the parse recorded the value's characters under, when it was
    /// written in the source.
    #[must_use]
    pub const fn id(&self) -> Option<NameId> {
        self.id
    }
}

impl<T> From<T> for Named<T> {
    fn from(value: T) -> Self {
        Self::unplaced(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target_name::TargetName;

    #[test]
    fn test_a_named_value_reads_back_with_its_id() {
        // Given
        let named = Named::new(TargetName::new("install"), Some(NameId::new(3)));

        // When
        let json = serde_json::to_string(&named).expect("serializes");
        let read: Named<TargetName> = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(read, named);
        assert_eq!(read.id(), Some(NameId::new(3)));
    }

    #[test]
    fn test_an_unplaced_value_has_no_id() {
        // Given / When
        let named = Named::unplaced(TargetName::new("install"));

        // Then
        assert_eq!(named.id(), None);
        assert_eq!(named, Named::from(TargetName::new("install")));
    }

    #[test]
    fn test_values_differ_by_the_name_they_were_written_as() {
        // Given
        let first = Named::new(TargetName::new("install"), Some(NameId::new(0)));
        let second = Named::new(TargetName::new("install"), Some(NameId::new(1)));

        // When / Then
        assert_ne!(first, second);
    }

    #[test]
    fn test_into_value_drops_the_id() {
        // Given
        let named = Named::new(TargetName::new("install"), Some(NameId::new(3)));

        // When / Then
        assert_eq!(named.into_value(), TargetName::new("install"));
    }

    #[test]
    fn test_an_id_names_its_row() {
        // Given / When / Then
        assert_eq!(NameId::new(7).index(), 7);
    }
}
