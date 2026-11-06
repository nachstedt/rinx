//! What a migration could not carry over, and says so rather than guess.

use std::path::PathBuf;

use rinx_entity::needs_conf::ReportEntry;
use serde::Serialize;

/// One thing a migration left out of the files it writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum MigrationFinding {
    /// A `conf.py` setting bound to a value reading could not take, which
    /// the written files leave at its default.
    UnreadSetting { setting: String },
    /// Running `conf.py` failed, so what reading it found was used.
    ConfRunFailed { message: String },
    /// An `intersphinx_mapping` entry, which a build links into only through
    /// a pinned `rinx_inventory` the migration cannot write without
    /// fetching it.
    Inventory { name: String },
    /// A file a document reads, shows or links that lies outside `conf.py`'s
    /// directory, where no label of the package can name it — and where,
    /// from the source root, the build looks for it instead: a path climbing
    /// above the root stays at the root there, as docutils' does not.
    OutsideSourceRoot { path: PathBuf, build_path: PathBuf },
    /// A file a document names that does not exist, which no label may
    /// name.
    MissingFile { path: PathBuf },
    /// A file inside a directory that is a Bazel package of its own, whose
    /// files only that package can name.
    InSubpackage { path: PathBuf },
    /// A document whose suffix is not `.rst`, which a library's `srcs`
    /// cannot hold.
    NotReStructuredText { path: PathBuf },
    /// A sphinx-needs construct the entity schema could not carry over.
    NeedsUnconverted { entry: ReportEntry },
    /// The sphinx-needs configuration converts into no valid entity schema,
    /// so none is written.
    NeedsInvalid,
}

impl MigrationFinding {
    /// The finding, as a sentence an author can act on.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::UnreadSetting { setting } => format!(
                "`{setting}` is computed in conf.py, so the written files leave it at its default"
            ),
            Self::ConfRunFailed { message } => {
                format!("running conf.py failed, so it was read without running: {message}")
            }
            Self::Inventory { name } => format!(
                "intersphinx inventory `{name}` is not migrated; declare it as a pinned \
                 rinx_inventory in the site's `inventories`"
            ),
            Self::OutsideSourceRoot { path, build_path } => format!(
                "{} lies outside conf.py's directory, so the build cannot read it; copy it to \
                 {}, where the build looks for it, and declare it",
                path.display(),
                build_path.display()
            ),
            Self::MissingFile { path } => {
                format!("{} does not exist, so it is not declared", path.display())
            }
            Self::InSubpackage { path } => format!(
                "{} lies in a Bazel package of its own, so this library cannot declare it",
                path.display()
            ),
            Self::NotReStructuredText { path } => format!(
                "{} is no `.rst` file, so the library cannot hold it as a document",
                path.display()
            ),
            Self::NeedsUnconverted { entry } => format!(
                "sphinx-needs {} is not carried into entities.toml: {}",
                entry.category.as_str(),
                entry.detail
            ),
            Self::NeedsInvalid => "the sphinx-needs configuration converts into no valid entity \
                 schema, so no entities.toml is written; see conf.py's diagnostics"
                .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_finding_serializes_with_its_kind_and_fields() {
        // Given
        let finding = MigrationFinding::OutsideSourceRoot {
            path: PathBuf::from("/x/a.txt"),
            build_path: PathBuf::from("a.txt"),
        };

        // When
        let json = serde_json::to_value(&finding).unwrap();

        // Then
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "outside-source-root",
                "path": "/x/a.txt",
                "build_path": "a.txt",
            })
        );
    }

    #[test]
    fn test_message_names_what_to_do() {
        // When / Then
        assert!(
            MigrationFinding::Inventory {
                name: "python".to_string()
            }
            .message()
            .contains("rinx_inventory")
        );
        assert!(
            MigrationFinding::UnreadSetting {
                setting: "version".to_string()
            }
            .message()
            .starts_with("`version`")
        );
    }
}
