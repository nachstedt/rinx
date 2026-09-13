//! Reading sphinx-needs' `needs.json` interchange file.
//!
//! This is the format `.. needimport::` reads, and it is a *third party's*
//! file, exactly as `entities.toml` is a *user's* file — which is why the two
//! readers live side by side in this crate. Both turn text into checked data
//! and leave every question that needs a schema to the phase that holds one:
//! [`load`](crate::load) knows what a declaration means, this module knows
//! only what the file says.
//!
//! The split matters because nothing here may reject a field. A `needs.json`
//! carries a large amount of sphinx-needs' own bookkeeping — `docname`,
//! `lineno`, `type_color`, `parent_needs` and a few dozen more — alongside the
//! project's actual data, and only an [`EntitySchema`](crate::EntitySchema)
//! can tell which is which. So [`RawNeed`] keeps every unrecognised key rather
//! than using `deny_unknown_fields`, [`is_internal_field`] names the ones this
//! build ignores by design, and the parser reports the rest against the type
//! the need claims.
//!
//! [`field_text`] is the other half of that contract. JSON is typed and an RST
//! option is not, so an imported value could plausibly get its own type
//! ladder — and then an `enum` attribute would validate one way when written
//! and another way when imported. Instead a JSON value is rendered to the text
//! form [`parse_attribute_value`](crate::parse_attribute_value) already
//! understands, and there is exactly one funnel.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;

/// The fields a `needs.json` carries that belong to sphinx-needs itself.
///
/// Ignored by design rather than reported: they describe how *that* tool
/// stored, rendered and located a need, none of which survives the trip. The
/// list is explicit — and tested against the example file — because the
/// alternative is ignoring every unrecognised field, which would silently drop
/// the project data this directive exists to carry.
///
/// Paired with [`is_internal_field`] the way [`BUILTIN_FIELDS`] is paired with
/// [`is_builtin_field`], and for the same reason: two copies of a name list
/// drift.
///
/// [`BUILTIN_FIELDS`]: crate::BUILTIN_FIELDS
/// [`is_builtin_field`]: crate::is_builtin_field
pub const INTERNAL_FIELDS: [&str; 38] = [
    "arch",
    "collapse",
    "constraints",
    "constraints_error",
    "constraints_passed",
    "constraints_results",
    "content_id",
    "content_node",
    "doctype",
    "docname",
    "external_css",
    "external_url",
    "full_title",
    "has_dead_links",
    "has_forbidden_dead_links",
    "hide",
    "id_complete",
    "id_prefix",
    "is_external",
    "is_import",
    "is_modified",
    "is_need",
    "is_part",
    "jinja_content",
    "layout",
    "lineno",
    "modifications",
    "parent_need",
    "parent_needs",
    "parts",
    "post_template",
    "pre_template",
    "section_name",
    "sections",
    "signature",
    "style",
    "template",
    "type_name",
];

/// Whether `name` is one of sphinx-needs' own bookkeeping fields.
///
/// `type_prefix`, `type_color` and `type_style` are matched by prefix rather
/// than listed, since they are one family and sphinx-needs has added to it
/// before.
#[must_use]
pub fn is_internal_field(name: &str) -> bool {
    INTERNAL_FIELDS.contains(&name) || name.starts_with("type_")
}

/// One need as the file spells it.
///
/// `id`, `type` and `content` are named because this build gives each a
/// meaning of its own; everything else stays in [`fields`](RawNeed::fields)
/// for the parser to resolve against the schema.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawNeed {
    /// The need's own id. Optional here so a file missing one is a diagnostic
    /// rather than a parse failure that loses every other need with it.
    #[serde(default)]
    pub id: Option<String>,
    /// The entity type the need claims, which must name a declared type.
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
    /// The need's body, as reStructuredText.
    #[serde(default)]
    pub content: Option<String>,
    /// Every other key, unexamined.
    #[serde(flatten)]
    pub fields: BTreeMap<String, serde_json::Value>,
}

/// One version block of a `needs.json`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NeedsVersion {
    /// The needs it holds, keyed by id.
    ///
    /// A `BTreeMap` rather than the file's own order, because the entities
    /// built from it become nodes of a `.ast` that Bazel caches on its bytes:
    /// a deterministic order is a correctness requirement, not a preference.
    #[serde(default)]
    pub needs: BTreeMap<String, RawNeed>,
}

/// A whole `needs.json`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NeedsFile {
    /// The version a reader should take when none is asked for.
    #[serde(default)]
    pub current_version: Option<String>,
    /// Every version block, keyed by version string.
    #[serde(default)]
    pub versions: BTreeMap<String, NeedsVersion>,
}

/// Why a version could not be chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    /// The file holds no version blocks at all.
    NoVersions,
    /// A `:version:` was asked for that the file does not hold.
    Unknown {
        requested: String,
        available: Vec<String>,
    },
    /// No `:version:` was asked for, the file names no `current_version`, and
    /// it holds more than one — so there is no version this build may pick
    /// without guessing.
    Ambiguous { available: Vec<String> },
    /// The file names a `current_version` it does not hold.
    DanglingCurrent {
        named: String,
        available: Vec<String>,
    },
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoVersions => write!(f, "the file holds no versions"),
            Self::Unknown {
                requested,
                available,
            } => write!(
                f,
                "no version {requested:?}; the file holds {}",
                available.join(", ")
            ),
            Self::Ambiguous { available } => write!(
                f,
                "the file names no `current_version` and holds {}; \
                 write `:version:` to choose one",
                available.join(", ")
            ),
            Self::DanglingCurrent { named, available } => write!(
                f,
                "the file names {named:?} as its `current_version` but holds {}",
                available.join(", ")
            ),
        }
    }
}

impl std::error::Error for VersionError {}

impl NeedsFile {
    /// Chooses the version block to import from.
    ///
    /// `requested` is the `:version:` option, when one was written. With none,
    /// `current_version` decides; with neither, a file holding exactly one
    /// version is unambiguous and anything else is refused rather than
    /// guessed at.
    ///
    /// # Errors
    ///
    /// Returns [`VersionError`] when no single version block is determined.
    pub fn select_version(
        &self,
        requested: Option<&str>,
    ) -> Result<(&str, &NeedsVersion), VersionError> {
        let available = || self.versions.keys().cloned().collect::<Vec<_>>();

        if let Some(requested) = requested {
            return self.lookup(requested).ok_or_else(|| VersionError::Unknown {
                requested: requested.to_string(),
                available: available(),
            });
        }
        if let Some(named) = self.current_version.as_deref() {
            return self
                .lookup(named)
                .ok_or_else(|| VersionError::DanglingCurrent {
                    named: named.to_string(),
                    available: available(),
                });
        }
        match self.versions.len() {
            0 => Err(VersionError::NoVersions),
            1 => self
                .versions
                .iter()
                .next()
                .map(|(name, version)| (name.as_str(), version))
                .ok_or(VersionError::NoVersions),
            _ => Err(VersionError::Ambiguous {
                available: available(),
            }),
        }
    }

    /// The version block named `name`, with its key borrowed from the map.
    fn lookup(&self, name: &str) -> Option<(&str, &NeedsVersion)> {
        self.versions
            .get_key_value(name)
            .map(|(key, version)| (key.as_str(), version))
    }
}

/// Reads a `needs.json`'s text.
///
/// # Errors
///
/// Returns the deserializer's own message, which carries the line and column
/// of the fault — the only position anything in this file has.
pub fn read_needs_json(text: &str) -> Result<NeedsFile, String> {
    serde_json::from_str(text).map_err(|error| error.to_string())
}

/// Renders a JSON value into the text an option would have been written with.
///
/// Returns `None` for a value with no such spelling — an object, a null, or a
/// list holding either — which the caller reports rather than coercing. A
/// nested structure means the file says something this model has no way to
/// store, and inventing a flattening for it would lose data quietly.
#[must_use]
pub fn field_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(flag) => Some(flag.to_string()),
        // Joined with the separator `split_list` splits on, so a JSON array
        // and a written `a, b` reach `parse_attribute_value` identically.
        serde_json::Value::Array(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                parts.push(scalar_text(item)?);
            }
            Some(parts.join(", "))
        }
        serde_json::Value::Object(_) | serde_json::Value::Null => None,
    }
}

/// Whether the file spelled this value as an explicit `null`.
///
/// JSON's own way of saying a field is unset, and the corpus uses it: the
/// sphinx-needs demo marks several `[needs.fields]` `nullable`, and its export
/// then writes `null` for every need that left one blank. Distinct from the
/// values [`field_text`] also refuses — an object or a nested array — because
/// those say something this model has nowhere to put, where a `null` says
/// nothing at all.
#[must_use]
pub fn field_is_null(value: &serde_json::Value) -> bool {
    value.is_null()
}

/// Whether the file spelled this value as a list.
///
/// Asked by a caller that must decide between a list-valued and a text-valued
/// answer for a field whose type nothing declares — a relation, say. Kept here
/// beside [`field_text`] rather than answered by matching on the value at the
/// call site, so that a reader of a `needs.json` needs no dependency on the
/// JSON library of its own.
#[must_use]
pub fn field_is_list(value: &serde_json::Value) -> bool {
    value.is_array()
}

/// The text of a value that may appear inside an array: never another array.
fn scalar_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_with(json: &str) -> NeedsFile {
        read_needs_json(json).expect("the fixture is valid JSON")
    }

    #[test]
    fn reads_a_needs_file_keeping_unrecognised_fields() {
        // Given a needs.json whose need carries a field this build knows
        // nothing about
        let json = r#"{
            "current_version": "1.0",
            "versions": {
                "1.0": {
                    "needs": {
                        "REQ_1": {
                            "id": "REQ_1",
                            "type": "req",
                            "content": "Prose.",
                            "owner": "alice",
                            "docname": "index"
                        }
                    }
                }
            }
        }"#;

        // When it is read
        let file = file_with(json);

        // Then the named fields are lifted out and every other key survives
        let need = &file.versions["1.0"].needs["REQ_1"];
        assert_eq!(need.id.as_deref(), Some("REQ_1"));
        assert_eq!(need.type_name.as_deref(), Some("req"));
        assert_eq!(need.content.as_deref(), Some("Prose."));
        assert!(need.fields.contains_key("owner"));
        assert!(need.fields.contains_key("docname"));
    }

    #[test]
    fn reads_a_need_missing_id_and_type_without_failing() {
        // Given a need with neither an id nor a type
        let json = r#"{"versions": {"1.0": {"needs": {"X": {}}}}}"#;

        // When it is read
        let file = file_with(json);

        // Then it deserializes, leaving both for the parser to diagnose
        let need = &file.versions["1.0"].needs["X"];
        assert!(need.id.is_none());
        assert!(need.type_name.is_none());
    }

    #[test]
    fn reports_malformed_json_with_its_position() {
        // Given text that is not JSON
        // When it is read
        let error = read_needs_json("{ oops }").expect_err("this is not JSON");

        // Then the deserializer's own message, carrying a position, is
        // returned
        assert!(error.contains("line"), "{error}");
    }

    #[test]
    fn select_version_prefers_the_requested_one() {
        // Given a file naming one current version and holding two
        let file = file_with(
            r#"{"current_version": "1.0",
                "versions": {"1.0": {"needs": {}}, "2.0": {"needs": {}}}}"#,
        );

        // When a different one is requested
        let (name, _) = file
            .select_version(Some("2.0"))
            .expect("the requested version exists");

        // Then the request wins over current_version
        assert_eq!(name, "2.0");
    }

    #[test]
    fn select_version_falls_back_to_current_version() {
        // Given a file naming a current version
        let file = file_with(
            r#"{"current_version": "2.0",
                "versions": {"1.0": {"needs": {}}, "2.0": {"needs": {}}}}"#,
        );

        // When no version is requested
        let (name, _) = file.select_version(None).expect("current_version exists");

        // Then current_version decides
        assert_eq!(name, "2.0");
    }

    #[test]
    fn select_version_takes_a_sole_version_when_nothing_names_one() {
        // Given a file with one version and no current_version
        let file = file_with(r#"{"versions": {"7": {"needs": {}}}}"#);

        // When no version is requested
        let (name, _) = file
            .select_version(None)
            .expect("one version is unambiguous");

        // Then the only version is taken
        assert_eq!(name, "7");
    }

    #[test]
    fn select_version_refuses_to_guess_between_several() {
        // Given two versions and no current_version
        let file = file_with(r#"{"versions": {"1.0": {"needs": {}}, "2.0": {"needs": {}}}}"#);

        // When no version is requested
        let error = file.select_version(None).expect_err("this is ambiguous");

        // Then it is refused, listing what could have been chosen
        assert_eq!(
            error,
            VersionError::Ambiguous {
                available: vec!["1.0".to_string(), "2.0".to_string()],
            }
        );
    }

    #[test]
    fn select_version_reports_an_unknown_request() {
        // Given a file holding one version
        let file = file_with(r#"{"versions": {"1.0": {"needs": {}}}}"#);

        // When another is requested
        let error = file.select_version(Some("9.9")).expect_err("9.9 is absent");

        // Then the request and the available versions are both named
        assert_eq!(
            error,
            VersionError::Unknown {
                requested: "9.9".to_string(),
                available: vec!["1.0".to_string()],
            }
        );
    }

    #[test]
    fn select_version_reports_a_dangling_current_version() {
        // Given a current_version the file does not hold
        let file = file_with(r#"{"current_version": "3.0", "versions": {"1.0": {"needs": {}}}}"#);

        // When no version is requested
        let error = file.select_version(None).expect_err("3.0 is absent");

        // Then the fault is named as the file's, not the author's
        assert_eq!(
            error,
            VersionError::DanglingCurrent {
                named: "3.0".to_string(),
                available: vec!["1.0".to_string()],
            }
        );
    }

    #[test]
    fn select_version_reports_a_file_with_no_versions() {
        // Given a file with no version blocks
        let file = file_with(r#"{"versions": {}}"#);

        // When a version is chosen
        let error = file.select_version(None).expect_err("there are none");

        // Then it says so
        assert_eq!(error, VersionError::NoVersions);
    }

    #[test]
    fn version_errors_display_what_was_available() {
        // Given each version error
        // When it is displayed
        // Then the message names the versions the reader could have written
        let unknown = VersionError::Unknown {
            requested: "9".to_string(),
            available: vec!["1".to_string(), "2".to_string()],
        }
        .to_string();
        assert!(
            unknown.contains("\"9\"") && unknown.contains("1, 2"),
            "{unknown}"
        );
        assert!(VersionError::NoVersions.to_string().contains("no versions"));
        assert!(
            VersionError::Ambiguous {
                available: vec!["1".to_string()],
            }
            .to_string()
            .contains(":version:")
        );
        assert!(
            VersionError::DanglingCurrent {
                named: "3".to_string(),
                available: vec!["1".to_string()],
            }
            .to_string()
            .contains("current_version")
        );
    }

    #[test]
    fn is_internal_field_matches_the_listed_names() {
        // Given sphinx-needs' own bookkeeping names
        // When each is tested
        // Then every listed name is internal
        for name in INTERNAL_FIELDS {
            assert!(is_internal_field(name), "{name} should be internal");
        }
    }

    #[test]
    fn is_internal_field_matches_the_type_family_by_prefix() {
        // Given the `type_*` presentation family, which sphinx-needs extends
        // When names from it are tested
        // Then they are internal without being listed individually
        assert!(is_internal_field("type_color"));
        assert!(is_internal_field("type_prefix"));
        assert!(is_internal_field("type_style"));
        assert!(is_internal_field("type_something_new"));
    }

    #[test]
    fn is_internal_field_leaves_project_data_alone() {
        // Given names a project would declare as attributes or relations
        // When each is tested
        // Then none is treated as bookkeeping
        for name in ["status", "owner", "tags", "links", "title", "id", "type"] {
            assert!(!is_internal_field(name), "{name} should not be internal");
        }
    }

    #[test]
    fn field_text_renders_each_scalar_as_an_option_would_spell_it() {
        // Given JSON scalars
        // When each is rendered
        // Then the text is what an author would have written in the option
        assert_eq!(
            field_text(&serde_json::json!("open")).as_deref(),
            Some("open")
        );
        assert_eq!(field_text(&serde_json::json!(42)).as_deref(), Some("42"));
        assert_eq!(
            field_text(&serde_json::json!(true)).as_deref(),
            Some("true")
        );
        assert_eq!(
            field_text(&serde_json::json!(false)).as_deref(),
            Some("false")
        );
    }

    #[test]
    fn field_text_joins_an_array_the_way_split_list_splits_it() {
        // Given a JSON array of scalars
        let value = serde_json::json!(["a", "b", 3]);

        // When it is rendered
        let text = field_text(&value).expect("scalars render");

        // Then splitting it again yields the original items
        assert_eq!(text, "a, b, 3");
        assert_eq!(crate::split_list(&text), vec!["a", "b", "3"]);
    }

    #[test]
    fn field_text_refuses_a_value_with_no_written_spelling() {
        // Given values this model has nowhere to store
        // When each is rendered
        // Then none is coerced into a string
        assert!(field_text(&serde_json::json!(null)).is_none());
        assert!(field_text(&serde_json::json!({"a": 1})).is_none());
        assert!(field_text(&serde_json::json!([["nested"]])).is_none());
        assert!(field_text(&serde_json::json!([{"a": 1}])).is_none());
    }

    #[test]
    fn field_text_renders_an_empty_array_as_empty_text() {
        // Given an empty JSON array
        // When it is rendered
        let text = field_text(&serde_json::json!([])).expect("an empty array is renderable");

        // Then it becomes the empty list `split_list` also produces
        assert_eq!(text, "");
        assert!(crate::split_list(&text).is_empty());
    }

    #[test]
    fn scalar_text_rejects_every_nested_value() {
        // Given values that may not appear inside an array
        // When each is rendered
        // Then none yields text
        assert!(scalar_text(&serde_json::json!([1])).is_none());
        assert!(scalar_text(&serde_json::json!({"a": 1})).is_none());
        assert!(scalar_text(&serde_json::json!(null)).is_none());
    }
}
