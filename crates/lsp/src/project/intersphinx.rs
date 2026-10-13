//! A Sphinx project's `intersphinx_mapping`: the other sites it links into,
//! and where each one's `objects.inv` is to be found (ADR-038 §6).
//!
//! This only says what the mapping means. Reading an inventory is
//! [`super::inventories`]'s, and downloading one is the editor's: the server
//! never touches the network.

use std::path::PathBuf;

use rinx_ast::Span;
use rinx_inventory::InventoryName;
use rinx_pyconf::{Value, ValueKind};

/// What a project's `intersphinx_mapping` and `intersphinx_cache_limit`
/// configure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntersphinxSettings {
    /// The declared inventories, in the order the mapping lists them.
    pub entries: Vec<IntersphinxEntry>,
    /// How many days a downloaded inventory stays fresh; negative for ever.
    pub cache_limit_days: i64,
}

/// Sphinx's default `intersphinx_cache_limit`, in days.
pub const DEFAULT_CACHE_LIMIT_DAYS: i64 = 5;

impl Default for IntersphinxSettings {
    /// A project linking into no other site.
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            cache_limit_days: DEFAULT_CACHE_LIMIT_DAYS,
        }
    }
}

/// One `intersphinx_mapping` entry: `name: (base_url, inventory)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntersphinxEntry {
    pub name: InventoryName,
    /// Where the site's pages are published, as written.
    pub base_url: String,
    /// Where its inventory may be read, tried in order until one loads.
    pub sources: Vec<InventorySource>,
    /// The entry in `conf.py`, where a problem with its inventory is reported.
    pub span: Span,
}

/// Where an inventory may be read.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum InventorySource {
    /// An address the editor downloads.
    Url(String),
    /// A file, relative to the directory holding `conf.py`, that the server
    /// reads itself.
    File(PathBuf),
}

/// The name Sphinx gives an inventory, under a site's base.
const INVENTORY_FILENAME: &str = "objects.inv";

impl InventorySource {
    /// The source `location` names: an address when it has a scheme, as
    /// Sphinx's `_fetch_inventory` decides, and a file otherwise.
    #[must_use]
    pub fn of(location: &str) -> Self {
        if location.contains("://") {
            Self::Url(location.to_string())
        } else {
            Self::File(PathBuf::from(location))
        }
    }

    /// The source a `None` target stands for: `objects.inv` under the base.
    #[must_use]
    pub fn default_for(base_url: &str) -> Self {
        Self::of(&format!(
            "{}/{INVENTORY_FILENAME}",
            base_url.trim_end_matches('/')
        ))
    }
}

/// The entries `mapping`, the value `intersphinx_mapping` is bound to,
/// declares — calling `invalid` with each part it must leave out and why, as
/// Sphinx's `validate_intersphinx_mapping` would refuse it. `span_of` turns a
/// value's position into a diagnostic's.
pub fn read_mapping(
    mapping: &Value,
    span_of: impl Fn(&Value) -> Span,
    mut invalid: impl FnMut(&Value, String),
) -> Vec<IntersphinxEntry> {
    let ValueKind::Dict(items) = &mapping.kind else {
        invalid(
            mapping,
            "`intersphinx_mapping` must be a dict from a name to `(base_url, inventory)`"
                .to_string(),
        );
        return Vec::new();
    };
    let mut entries = Vec::new();
    for (key, value) in items {
        let Some(key_text) = key.as_str() else {
            invalid(
                key,
                "an `intersphinx_mapping` key must be a string naming the inventory".to_string(),
            );
            continue;
        };
        let Some([base, target]) = value
            .as_sequence()
            .and_then(|pair| <&[Value; 2]>::try_from(pair).ok())
        else {
            let message = if key_text.contains("://") {
                format!(
                    "`{key_text}` is keyed by its address, a form Sphinx 8 removed; write `'name': ('{key_text}', None)`"
                )
            } else {
                format!("`{key_text}` must map to a pair `(base_url, inventory)`")
            };
            invalid(key, message);
            continue;
        };
        let name = match InventoryName::new(key_text) {
            Ok(name) => name,
            Err(error) => {
                invalid(key, error.to_string());
                continue;
            }
        };
        let Some(base_url) = base.as_str().filter(|text| !text.is_empty()) else {
            invalid(
                base,
                format!("the base URL of `{key_text}` must be a non-empty string"),
            );
            continue;
        };
        let Some(sources) = read_targets(target, base_url, key_text, &mut invalid) else {
            continue;
        };
        entries.push(IntersphinxEntry {
            name,
            base_url: base_url.to_string(),
            sources,
            span: span_of(key),
        });
    }
    entries
}

/// The sources `target`, an entry's second element, names: `None`, a
/// location, or a sequence of them — or `None` when it is none of these.
fn read_targets(
    target: &Value,
    base_url: &str,
    name: &str,
    invalid: &mut impl FnMut(&Value, String),
) -> Option<Vec<InventorySource>> {
    let single = |value: &Value| match &value.kind {
        ValueKind::None => Some(InventorySource::default_for(base_url)),
        ValueKind::Str(location) if !location.is_empty() => Some(InventorySource::of(location)),
        _ => None,
    };
    if let Some(source) = single(target) {
        return Some(vec![source]);
    }
    let refuse = |invalid: &mut dyn FnMut(&Value, String), value: &Value| {
        invalid(
            value,
            format!("the inventory of `{name}` must be `None`, a location, or a tuple of them"),
        );
    };
    let Some(targets) = target.as_sequence() else {
        refuse(invalid, target);
        return None;
    };
    let mut sources: Vec<InventorySource> = Vec::new();
    for item in targets {
        match single(item) {
            Some(source) if !sources.contains(&source) => sources.push(source),
            Some(_) => {}
            None => refuse(invalid, item),
        }
    }
    if sources.is_empty() {
        refuse(invalid, target);
        return None;
    }
    Some(sources)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::Position;
    use rinx_pyconf::{BoundValue, read_module};

    /// What `text`, the right-hand side of `intersphinx_mapping`, declares,
    /// and the messages of what it left out.
    fn read(text: &str) -> (Vec<IntersphinxEntry>, Vec<String>) {
        let module = read_module(&format!("intersphinx_mapping = {text}\n"));
        let BoundValue::Literal(value) = &module.binding("intersphinx_mapping").unwrap().value
        else {
            panic!("not a literal: {text}");
        };
        let mut messages = Vec::new();
        let entries = read_mapping(
            value,
            |value| {
                let line = u32::try_from(value.span.start.line).unwrap();
                Span::new(Position::new(line, 1), Position::new(line, 1))
            },
            |_, message| messages.push(message),
        );
        (entries, messages)
    }

    fn sources(text: &str) -> Vec<InventorySource> {
        let (entries, messages) = read(text);
        assert_eq!(messages, Vec::<String>::new(), "{text}");
        entries.into_iter().next().unwrap().sources
    }

    fn url(text: &str) -> InventorySource {
        InventorySource::Url(text.to_string())
    }

    #[test]
    fn test_read_mapping_reads_a_none_target_as_the_default_inventory() {
        // When
        let (entries, messages) = read("{'python': ('https://docs.python.org/3/', None)}");

        // Then
        assert!(messages.is_empty());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name.as_str(), "python");
        assert_eq!(entries[0].base_url, "https://docs.python.org/3/");
        assert_eq!(
            entries[0].sources,
            [url("https://docs.python.org/3/objects.inv")]
        );
        assert_eq!(entries[0].span.start.line, 1);
    }

    #[test]
    fn test_read_mapping_reads_a_location_as_an_address_or_a_file() {
        // When / Then
        assert_eq!(
            sources("{'a': ('https://a.org', 'https://mirror.org/a.inv')}"),
            [url("https://mirror.org/a.inv")]
        );
        assert_eq!(
            sources("{'a': ('https://a.org', '_inv/a.inv')}"),
            [InventorySource::File(PathBuf::from("_inv/a.inv"))]
        );
        assert_eq!(
            sources("{'a': ('../a/', None)}"),
            [InventorySource::File(PathBuf::from("../a/objects.inv"))]
        );
    }

    #[test]
    fn test_read_mapping_keeps_every_target_of_a_tuple_in_order() {
        // When
        let found = sources("{'a': ('https://a.org', ('local.inv', None, None))}");

        // Then — the repeated `None` counts once
        assert_eq!(
            found,
            [
                InventorySource::File(PathBuf::from("local.inv")),
                url("https://a.org/objects.inv")
            ]
        );
    }

    #[test]
    fn test_read_mapping_keeps_entries_in_declaration_order() {
        // When
        let (entries, _) = read("{'b': ('https://b.org', None), 'a': ['https://a.org', None]}");

        // Then
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["b", "a"]);
    }

    #[test]
    fn test_read_mapping_refuses_the_address_keyed_form_by_name() {
        // When
        let (entries, messages) = read("{'https://docs.python.org/': None}");

        // Then
        assert!(entries.is_empty());
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("Sphinx 8 removed"), "{}", messages[0]);
    }

    #[test]
    fn test_read_mapping_refuses_each_malformed_entry_and_keeps_the_rest() {
        // Given
        let text = "{\n 'ok': ('https://ok.org', None),\n 1: ('https://x.org', None),\n 'one': ('https://x.org',),\n 'bad name': ('https://x.org', None),\n 'nobase': ('', None),\n 'target': ('https://x.org', 3),\n 'empty': ('https://x.org', ()),\n}";

        // When
        let (entries, messages) = read(text);

        // Then
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["ok"]);
        assert_eq!(messages.len(), 6, "{messages:?}");
    }

    #[test]
    fn test_read_mapping_leaves_out_a_bad_target_of_a_tuple() {
        // When
        let (entries, messages) = read("{'a': ('https://a.org', (None, 3))}");

        // Then
        assert_eq!(entries[0].sources, [url("https://a.org/objects.inv")]);
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_read_mapping_refuses_a_mapping_that_is_no_dict() {
        // When
        let (entries, messages) = read("[('https://a.org', None)]");

        // Then
        assert!(entries.is_empty());
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_settings_default_to_sphinxs() {
        // When
        let settings = IntersphinxSettings::default();

        // Then
        assert!(settings.entries.is_empty());
        assert_eq!(settings.cache_limit_days, 5);
    }
}
