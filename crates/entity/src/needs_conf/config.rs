//! A sphinx-needs configuration, as the conversion reads it.
//!
//! sphinx-needs is configured by `needs_*` settings, written in `conf.py` or
//! — with `needs_from_toml` — in a `[needs]` table of a `ubproject.toml`
//! whose keys drop the prefix. [`NeedsConfig`] is that table's shape, so
//! both sources deserialize into it: `needs_types` in `conf.py` is `types`
//! here. Only the settings the conversion reads are named; sphinx-needs has
//! dozens more about presentation, and every other key is ignored.
//!
//! Two spellings are accepted for a need's options and links: sphinx-needs
//! 6's `fields` and `links` tables, and the `extra_options`/`extra_links`
//! lists they replace, which a project configured in `conf.py` still writes.
//! [`NeedsConfig::fields`] and [`NeedsConfig::links`] give one view of each.

use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

use serde::Deserialize;
use serde::de::{DeserializeSeed, Deserializer, IgnoredAny, MapAccess, Visitor};
use serde_json::Value;

use super::ordered_table::OrderedTable;

/// The settings of a sphinx-needs project the conversion reads.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct NeedsConfig {
    /// `needs_types`: one directive per need type.
    pub types: Vec<NeedType>,
    /// `needs_fields`: the options every need may carry, beyond the built-in.
    pub fields: OrderedTable<NeedField>,
    /// `needs_extra_options`: the older list spelling of [`Self::fields`].
    pub extra_options: Vec<ExtraOption>,
    /// `needs_links`: the link options every need may carry.
    pub links: OrderedTable<NeedLink>,
    /// `needs_extra_links`: the older list spelling of [`Self::links`].
    pub extra_links: Vec<ExtraLink>,
    /// `needs_statuses`: the values `:status:` may take, when set.
    pub statuses: Vec<NeedStatus>,
    /// `needs_id_required`: every need must be given an explicit id.
    pub id_required: bool,
    /// `needs_import_keys`: names a `.. needimport::` may write for a file.
    pub import_keys: BTreeMap<String, String>,
    /// `needs_constraints`: checks evaluated while building.
    pub constraints: Option<Value>,
    /// `needs_variants`: option values chosen per build configuration.
    pub variants: Option<Value>,
    /// `needs_variant_data_file`: where further variants are read from.
    pub variant_data_file: Option<Value>,
    /// `needs_global_options`: defaults assigned by filter.
    pub global_options: Option<Value>,
    /// `needs_schema_definitions_from_json`: the `schemas.json` holding the
    /// project's validation rules, relative to the configuration file.
    pub schema_definitions_from_json: Option<String>,
}

/// One `needs_types` entry.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NeedType {
    /// The directive name, `req` for `.. req::`.
    pub directive: String,
    /// The type's human-readable name.
    #[serde(default)]
    pub title: Option<String>,
    /// What a generated id of the type starts with.
    #[serde(default)]
    pub prefix: Option<String>,
}

/// One `needs_fields` entry: an option every need may carry.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct NeedField {
    pub description: Option<String>,
    /// The JSON-Schema fragment typing the value — the only place
    /// sphinx-needs states one.
    pub schema: Option<FieldSchema>,
    /// Whether the value may be written in variant syntax.
    pub parse_variants: bool,
}

/// The part of a field's JSON-Schema fragment that types it.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct FieldSchema {
    #[serde(rename = "type")]
    pub json_type: Option<String>,
    #[serde(rename = "enum")]
    pub values: Option<Vec<Value>>,
}

/// One `needs_extra_options` entry: a name, or a field with its name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ExtraOption {
    Name(String),
    Field {
        name: String,
        #[serde(flatten)]
        field: NeedField,
    },
}

/// One `needs_links` entry: a link option every need may carry.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct NeedLink {
    /// The label of the link, read from the need carrying it.
    pub outgoing: Option<String>,
    /// The label of the back-link, read from the need it points at.
    pub incoming: Option<String>,
    /// Rules computing the link's targets with a dynamic function.
    pub predicates: Option<Value>,
    /// Whether the link is copied onto every need it points at.
    pub copy: bool,
}

/// One `needs_extra_links` entry: a link with its option name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExtraLink {
    pub option: String,
    #[serde(flatten)]
    pub link: NeedLink,
}

/// One `needs_statuses` entry.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NeedStatus {
    pub name: String,
}

/// Why a configuration could not be read as one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeedsConfigError {
    pub message: String,
}

impl fmt::Display for NeedsConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for NeedsConfigError {}

/// The table of a `ubproject.toml` sphinx-needs reads its `needs` table
/// from by default — `needs_from_toml_table`'s default, the top level.
pub const DEFAULT_TOML_ROOT: &[&str] = &[];

/// The table under the root that holds the settings.
const NEEDS_TABLE: &str = "needs";

/// The settings [`NeedsConfig`] reads, as `ubproject.toml` keys them; a
/// `conf.py` writes each with a `needs_` prefix.
pub const NEEDS_SETTINGS: &[&str] = &[
    "types",
    "fields",
    "extra_options",
    "links",
    "extra_links",
    "statuses",
    "id_required",
    "import_keys",
    "constraints",
    "variants",
    "variant_data_file",
    "global_options",
    "schema_definitions_from_json",
];

impl NeedsConfig {
    /// The configuration in the `needs` table under the table at `root` of
    /// `text`, a `ubproject.toml` — `[needs]` for the top level, `[tool.needs]`
    /// for `["tool"]`, as `needs_from_toml_table` names it — and the settings
    /// it writes; empty when the table is not there.
    ///
    /// # Errors
    ///
    /// Returns a [`NeedsConfigError`] when `text` is no TOML, the path does
    /// not lead to a table, or a setting has the wrong shape.
    pub fn from_toml(text: &str, root: &[&str]) -> Result<(Self, Vec<String>), NeedsConfigError> {
        let error = |error: toml::de::Error| NeedsConfigError {
            message: error.message().trim().to_string(),
        };
        let table: Vec<&str> = root.iter().copied().chain([NEEDS_TABLE]).collect();
        let table = table.as_slice();
        let keys: Option<OrderedTable<IgnoredAny>> = AtPath::new(table)
            .deserialize(toml::Deserializer::parse(text).map_err(error)?)
            .map_err(error)?;
        let Some(keys) = keys else {
            return Ok((Self::default(), Vec::new()));
        };
        let config: Option<Self> = AtPath::new(table)
            .deserialize(toml::Deserializer::parse(text).map_err(error)?)
            .map_err(error)?;
        let written = NEEDS_SETTINGS
            .iter()
            .filter(|setting| keys.contains_key(setting))
            .map(|setting| (*setting).to_string())
            .collect();
        Ok((config.unwrap_or_default(), written))
    }

    /// This configuration, with each setting it leaves at its default taken
    /// from `fallback` — for two sources that write disjoint settings.
    #[must_use]
    pub fn or(self, fallback: Self) -> Self {
        fn pick<T: PartialEq + Default>(mine: T, theirs: T) -> T {
            if mine == T::default() { theirs } else { mine }
        }
        Self {
            types: pick(self.types, fallback.types),
            fields: pick(self.fields, fallback.fields),
            extra_options: pick(self.extra_options, fallback.extra_options),
            links: pick(self.links, fallback.links),
            extra_links: pick(self.extra_links, fallback.extra_links),
            statuses: pick(self.statuses, fallback.statuses),
            id_required: pick(self.id_required, fallback.id_required),
            import_keys: pick(self.import_keys, fallback.import_keys),
            constraints: pick(self.constraints, fallback.constraints),
            variants: pick(self.variants, fallback.variants),
            variant_data_file: pick(self.variant_data_file, fallback.variant_data_file),
            global_options: pick(self.global_options, fallback.global_options),
            schema_definitions_from_json: pick(
                self.schema_definitions_from_json,
                fallback.schema_definitions_from_json,
            ),
        }
    }

    /// Every option a need may carry beyond the built-in ones: the
    /// `extra_options` list, then the `fields` table, a later entry of a name
    /// replacing an earlier one where it stands.
    #[must_use]
    pub fn all_fields(&self) -> OrderedTable<NeedField> {
        let listed = self.extra_options.iter().map(|option| match option {
            ExtraOption::Name(name) => (name.clone(), NeedField::default()),
            ExtraOption::Field { name, field } => (name.clone(), field.clone()),
        });
        listed
            .chain(
                self.fields
                    .iter()
                    .map(|(name, field)| (name.to_string(), field.clone())),
            )
            .collect()
    }

    /// Every link option a need may carry: the `extra_links` list, then the
    /// `links` table, a later entry of a name replacing an earlier one.
    #[must_use]
    pub fn all_links(&self) -> OrderedTable<NeedLink> {
        let listed = self
            .extra_links
            .iter()
            .map(|link| (link.option.clone(), link.link.clone()));
        listed
            .chain(
                self.links
                    .iter()
                    .map(|(name, link)| (name.to_string(), link.clone())),
            )
            .collect()
    }
}

/// Deserializes the value at a path of nested tables as a `T`, skipping
/// every other entry; `None` when the path is not there.
struct AtPath<'a, T> {
    path: &'a [&'a str],
    leaf: PhantomData<T>,
}

impl<'a, T> AtPath<'a, T> {
    fn new(path: &'a [&'a str]) -> Self {
        Self {
            path,
            leaf: PhantomData,
        }
    }
}

impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for AtPath<'_, T> {
    type Value = Option<T>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        if self.path.is_empty() {
            return T::deserialize(deserializer).map(Some);
        }
        deserializer.deserialize_map(self)
    }
}

impl<'de, T: Deserialize<'de>> Visitor<'de> for AtPath<'_, T> {
    type Value = Option<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a table holding `{}`", self.path.join("."))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
        let mut found = None;
        while let Some(key) = access.next_key::<String>()? {
            if key == self.path[0] {
                found = access.next_value_seed(AtPath::<T>::new(&self.path[1..]))?;
            } else {
                access.next_value::<IgnoredAny>()?;
            }
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_toml_reads_the_needs_table_and_the_settings_it_writes() {
        // Given a `ubproject.toml` with other tables beside `[needs]`
        let text = "[project]\nname = \"x\"\n\n[needs]\nid_required = true\nbuild_json = true\n\n[[needs.types]]\ndirective = \"req\"\n";

        // When
        let (config, written) = NeedsConfig::from_toml(text, DEFAULT_TOML_ROOT).expect("valid");

        // Then unknown settings are ignored, and only the read ones are listed
        assert!(config.id_required);
        assert_eq!(config.types[0].directive, "req");
        assert_eq!(written, ["types", "id_required"]);
    }

    #[test]
    fn test_from_toml_reads_a_nested_table() {
        // Given `needs_from_toml_table = ["tool", "needs"]`
        let text = "[tool.needs]\nid_required = true\n";

        // When
        let (config, _) = NeedsConfig::from_toml(text, &["tool"]).expect("valid");

        // Then
        assert!(config.id_required);
    }

    #[test]
    fn test_from_toml_without_the_table_is_empty() {
        // Given
        let text = "[project]\nname = \"x\"\n";

        // When
        let (config, written) = NeedsConfig::from_toml(text, DEFAULT_TOML_ROOT).expect("valid");

        // Then
        assert_eq!(config, NeedsConfig::default());
        assert!(written.is_empty());
    }

    #[test]
    fn test_from_toml_reports_a_setting_of_the_wrong_shape() {
        // Given
        let text = "[needs]\ntypes = \"req\"\n";

        // When
        let error = NeedsConfig::from_toml(text, DEFAULT_TOML_ROOT).expect_err("invalid");

        // Then
        assert!(!error.message.is_empty());
    }

    #[test]
    fn test_from_toml_reports_text_that_is_no_toml() {
        // Given / When
        let error = NeedsConfig::from_toml("[needs", DEFAULT_TOML_ROOT).expect_err("invalid");

        // Then
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn test_from_toml_keeps_the_order_fields_are_written_in() {
        // Given fields out of alphabetical order
        let text = "[needs.fields.zeta]\n[needs.fields.alpha]\n";

        // When
        let (config, _) = NeedsConfig::from_toml(text, DEFAULT_TOML_ROOT).expect("valid");

        // Then
        let names: Vec<&str> = config.fields.iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["zeta", "alpha"]);
    }

    #[test]
    fn test_or_takes_each_unset_setting_from_the_fallback() {
        // Given one source writing the types and another the id rule
        let types = NeedsConfig {
            types: vec![NeedType {
                directive: "req".into(),
                title: None,
                prefix: None,
            }],
            ..NeedsConfig::default()
        };
        let rule = NeedsConfig {
            id_required: true,
            schema_definitions_from_json: Some("schemas.json".into()),
            ..NeedsConfig::default()
        };

        // When
        let merged = types.or(rule);

        // Then
        assert_eq!(merged.types.len(), 1);
        assert!(merged.id_required);
        assert_eq!(
            merged.schema_definitions_from_json.as_deref(),
            Some("schemas.json")
        );
    }

    #[test]
    fn test_all_fields_lists_extra_options_before_fields() {
        // Given both spellings, one name written in each
        let config: NeedsConfig = serde_json::from_value(serde_json::json!({
            "extra_options": ["owner", "effort"],
            "fields": {"effort": {"description": "Points"}, "asil": {}},
        }))
        .expect("a configuration");

        // When
        let fields = config.all_fields();

        // Then the later entry replaces the earlier one where it stands
        let names: Vec<&str> = fields.iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["owner", "effort", "asil"]);
        assert_eq!(
            fields.get("effort").and_then(|f| f.description.as_deref()),
            Some("Points")
        );
    }
}
