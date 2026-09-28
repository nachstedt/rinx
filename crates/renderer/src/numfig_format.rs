//! `rinx.toml`'s `[numfig_format]` table: the text each kind of number is
//! shown in — Sphinx's `numfig_format`.
//!
//! Its own module rather than a field block in `config.rs` because it is a
//! type with invariants of its own. Every format is a [`NumberFormat`], and
//! none may show `{name}`: the same format is written in front of the caption
//! it would name, and Sphinx itself cannot apply one there — it fails the
//! build — so it is refused on load rather than half-supported.

use rinx_ast::{EnumerableKind, NumberFormat};
use serde::{Deserialize, Deserializer};

/// The format each kind of number is shown in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumfigFormat {
    #[serde(default = "default_figure", deserialize_with = "caption_format")]
    figure: NumberFormat,
    #[serde(default = "default_table", deserialize_with = "caption_format")]
    table: NumberFormat,
    #[serde(
        rename = "code-block",
        default = "default_code_block",
        deserialize_with = "caption_format"
    )]
    code_block: NumberFormat,
    #[serde(default = "default_section", deserialize_with = "caption_format")]
    section: NumberFormat,
}

impl NumfigFormat {
    /// The format a number of `kind` is shown in.
    #[must_use]
    pub const fn for_kind(&self, kind: EnumerableKind) -> &NumberFormat {
        match kind {
            EnumerableKind::Figure => &self.figure,
            EnumerableKind::Table => &self.table,
            EnumerableKind::CodeBlock => &self.code_block,
            EnumerableKind::Section => &self.section,
        }
    }
}

impl Default for NumfigFormat {
    /// Sphinx's defaults.
    fn default() -> Self {
        Self {
            figure: default_figure(),
            table: default_table(),
            code_block: default_code_block(),
            section: default_section(),
        }
    }
}

/// A built-in format, which is valid by construction.
fn builtin(format: &str) -> NumberFormat {
    NumberFormat::parse(format).expect("a built-in format is valid")
}

fn default_figure() -> NumberFormat {
    builtin("Fig. %s")
}

fn default_table() -> NumberFormat {
    builtin("Table %s")
}

fn default_code_block() -> NumberFormat {
    builtin("Listing %s")
}

fn default_section() -> NumberFormat {
    builtin("Section %s")
}

/// Reads one format, refusing a `{name}` — which a caption's own number
/// cannot show.
fn caption_format<'de, D: Deserializer<'de>>(deserializer: D) -> Result<NumberFormat, D::Error> {
    use serde::de::Error as _;
    let format = NumberFormat::deserialize(deserializer)?;
    if format.requires_name() {
        return Err(D::Error::custom(format!(
            "'{}' shows {{name}}, but it is also written in front of the caption it would name; \
             use {{name}} in a :numref: title instead",
            format.as_str()
        )));
    }
    Ok(format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Wrapper {
        #[serde(default)]
        numfig_format: NumfigFormat,
    }

    fn read(toml_str: &str) -> Result<NumfigFormat, toml::de::Error> {
        toml::from_str::<Wrapper>(toml_str).map(|wrapper| wrapper.numfig_format)
    }

    #[test]
    fn test_defaults_are_sphinx_defaults() {
        // Given / When
        let formats = NumfigFormat::default();

        // Then
        assert_eq!(formats.for_kind(EnumerableKind::Figure).as_str(), "Fig. %s");
        assert_eq!(formats.for_kind(EnumerableKind::Table).as_str(), "Table %s");
        assert_eq!(
            formats.for_kind(EnumerableKind::CodeBlock).as_str(),
            "Listing %s"
        );
        assert_eq!(
            formats.for_kind(EnumerableKind::Section).as_str(),
            "Section %s"
        );
    }

    #[test]
    fn test_a_table_overrides_only_the_kinds_it_names() {
        // Given / When
        let formats =
            read("[numfig_format]\nfigure = \"Figure %s\"\ncode-block = \"Code {number}\"\n")
                .expect("valid");

        // Then
        assert_eq!(
            formats.for_kind(EnumerableKind::Figure).as_str(),
            "Figure %s"
        );
        assert_eq!(
            formats.for_kind(EnumerableKind::CodeBlock).as_str(),
            "Code {number}"
        );
        assert_eq!(formats.for_kind(EnumerableKind::Table).as_str(), "Table %s");
    }

    #[test]
    fn test_refuses_a_format_that_cannot_be_applied() {
        // Given / When
        let error = read("[numfig_format]\nfigure = \"Figure\"\n").unwrap_err();

        // Then
        assert!(
            error.to_string().contains("no place for the number"),
            "{error}"
        );
    }

    #[test]
    fn test_refuses_a_name_field() {
        // Given / When
        let error = read("[numfig_format]\ntable = \"{name} {number}\"\n").unwrap_err();

        // Then
        assert!(error.to_string().contains("shows {name}"), "{error}");
    }

    #[test]
    fn test_refuses_an_unknown_kind() {
        // Given / When
        let result = read("[numfig_format]\nfigures = \"Fig. %s\"\n");

        // Then
        assert!(result.is_err());
    }
}
