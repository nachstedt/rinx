//! What the server knows about one project: where its sources are, and how
//! they are parsed, indexed and rendered (ADR-038 §2).
//!
//! Every feature reads a [`ProjectModel`] and none asks which source built
//! it: a folder with no configuration and a Sphinx project read from its
//! `conf.py` differ only in their settings. The Bazel manifest (roadmap #18)
//! will be a third [`ProjectSource`].

use std::path::{Path, PathBuf};

use rinx_analyzer::IndexSettings;
use rinx_ast::{Domain, ResolvedLanguage};
use rinx_parser::DefaultRole;
use rinx_renderer::config::SiteConfig;

use super::exclusion::Exclusion;

/// Where a project's configuration comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectSource {
    /// A workspace folder, or what of it lies outside every `conf.py`: no
    /// configuration, so every `.rst` file is a document.
    Folder,
    /// A Sphinx project, configured by the `conf.py` at `conf`.
    SphinxConf { conf: PathBuf },
}

/// How one project's documents are found, parsed, indexed and rendered.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectModel {
    /// The directory documents are named relative to: the folder, or the
    /// directory holding `conf.py`.
    pub source_root: PathBuf,
    pub source: ProjectSource,
    pub settings: ProjectSettings,
}

/// What a document's parse is configured with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSettings {
    /// The domain of an unqualified directive or role — `primary_domain`.
    pub domain: Domain,
    /// The role a bare `` `text` `` is read as — `default_role`.
    pub default_role: DefaultRole,
}

impl Default for ParseSettings {
    /// The build's own defaults, which are Sphinx's.
    fn default() -> Self {
        Self {
            domain: Domain::Py,
            default_role: DefaultRole::TITLE_REFERENCE,
        }
    }
}

/// A project's settings, each defaulting to Sphinx's default.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSettings {
    /// The root document's name, without a suffix — `root_doc`.
    pub root_doc: String,
    /// The file suffixes of documents, in the order a name is tried against
    /// them — the reStructuredText entries of `source_suffix`.
    pub source_suffixes: Vec<String>,
    /// Which files under the root are considered.
    pub exclusion: Exclusion,
    pub parse: ParseSettings,
    /// Whether captioned figures, tables and code blocks are numbered.
    pub numfig: bool,
    /// How many section-number components a `numfig` number starts with.
    pub numfig_secnum_depth: usize,
    /// The language a code block with none of its own is highlighted as.
    pub highlight_language: ResolvedLanguage,
}

/// Sphinx's default root document.
pub(crate) const DEFAULT_ROOT_DOC: &str = "index";

/// Sphinx's default source suffix, and the only one a folder with no
/// configuration has.
pub(crate) const RST_SUFFIX: &str = ".rst";

impl Default for ProjectSettings {
    /// A folder with no configuration: every `.rst` file under it, parsed,
    /// indexed and rendered with Sphinx's defaults.
    fn default() -> Self {
        Self {
            root_doc: DEFAULT_ROOT_DOC.to_string(),
            source_suffixes: vec![RST_SUFFIX.to_string()],
            exclusion: Exclusion::none(),
            parse: ParseSettings::default(),
            numfig: false,
            numfig_secnum_depth: rinx_index::DEFAULT_NUMFIG_SECNUM_DEPTH,
            highlight_language: ResolvedLanguage::default(),
        }
    }
}

impl ProjectModel {
    /// The project of a folder with no configuration, at `root`.
    #[must_use]
    pub fn folder(root: PathBuf) -> Self {
        Self {
            source_root: root,
            source: ProjectSource::Folder,
            settings: ProjectSettings::default(),
        }
    }

    /// The name the build gives the document at `path` — its path relative
    /// to the source root, `/`-separated, its suffix replaced by `.rst` — or
    /// `None` when `path` is no document of this project: outside the root,
    /// inside a hidden or an excluded directory, excluded itself, or without
    /// a source suffix.
    ///
    /// Every name ends in `.rst` whatever the file's suffix, since that is
    /// what a toctree entry resolves to; [`Self::source_path`] goes back.
    #[must_use]
    pub fn doc_path_of(&self, path: &Path) -> Option<String> {
        let relative = path.strip_prefix(&self.source_root).ok()?;
        let components: Vec<String> = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();
        let (file, directories) = components.split_last()?;
        let mut directory = String::new();
        for component in directories {
            if component.starts_with('.') {
                return None;
            }
            if !directory.is_empty() {
                directory.push('/');
            }
            directory.push_str(component);
            if self.settings.exclusion.excludes_directory(&directory) {
                return None;
            }
        }
        let relative = components.join("/");
        if !self.settings.exclusion.considers_file(&relative) {
            return None;
        }
        let stem = self.docname_of(file)?;
        Some(if directory.is_empty() {
            format!("{stem}{RST_SUFFIX}")
        } else {
            format!("{directory}/{stem}{RST_SUFFIX}")
        })
    }

    /// Whether the walk enters the directory at `directory`: under the root,
    /// with no hidden or excluded directory on the way, itself included.
    #[must_use]
    pub fn enters(&self, directory: &Path) -> bool {
        let Ok(relative) = directory.strip_prefix(&self.source_root) else {
            return false;
        };
        let mut walked = String::new();
        for component in relative.components() {
            let name = component.as_os_str().to_string_lossy();
            if name.starts_with('.') {
                return false;
            }
            if !walked.is_empty() {
                walked.push('/');
            }
            walked.push_str(&name);
            if self.settings.exclusion.excludes_directory(&walked) {
                return false;
            }
        }
        true
    }

    /// The file name `file` without the first source suffix it ends with,
    /// as Sphinx's `Project.path2doc` tries them; `None` without one.
    fn docname_of<'a>(&self, file: &'a str) -> Option<&'a str> {
        self.settings
            .source_suffixes
            .iter()
            .find_map(|suffix| file.strip_suffix(suffix.as_str()))
            .filter(|stem| !stem.is_empty())
    }

    /// Where the document named `doc_path` would be with the first source
    /// suffix, as Sphinx's `doc2path` places a document it has not found.
    #[must_use]
    pub fn source_path(&self, doc_path: &str) -> PathBuf {
        let stem = doc_path.strip_suffix(RST_SUFFIX).unwrap_or(doc_path);
        let suffix = self
            .settings
            .source_suffixes
            .first()
            .map_or(RST_SUFFIX, String::as_str);
        let named = format!("{stem}{suffix}");
        named
            .split('/')
            .fold(self.source_root.clone(), |path, component| {
                path.join(component)
            })
    }

    /// The configuration the server renders this project's documents with,
    /// `root_doc` being the root document it settled on.
    #[must_use]
    pub fn site_config(&self, root_doc: &str) -> SiteConfig {
        SiteConfig {
            root_doc: root_doc.to_string(),
            numfig: self.settings.numfig,
            numfig_secnum_depth: self.settings.numfig_secnum_depth,
            highlight_language: self.settings.highlight_language.clone(),
            ..SiteConfig::default()
        }
    }

    /// The settings this project's index is built with, `root_doc` being the
    /// root document it settled on.
    #[must_use]
    pub fn index_settings<'a>(&self, root_doc: &'a str) -> IndexSettings<'a> {
        IndexSettings {
            root_doc,
            numfig_secnum_depth: self.settings.numfig_secnum_depth,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sphinx(suffixes: &[&str], excluded: &[&str]) -> ProjectModel {
        let excluded: Vec<String> = excluded.iter().map(|p| (*p).to_string()).collect();
        ProjectModel {
            source_root: PathBuf::from("/work/docs"),
            source: ProjectSource::SphinxConf {
                conf: PathBuf::from("/work/docs/conf.py"),
            },
            settings: ProjectSettings {
                source_suffixes: suffixes.iter().map(|s| (*s).to_string()).collect(),
                exclusion: Exclusion::sphinx(&excluded, &["**".to_string()]),
                ..ProjectSettings::default()
            },
        }
    }

    #[test]
    fn test_doc_path_of_names_a_document_by_its_path_in_the_root() {
        // Given
        let model = ProjectModel::folder(PathBuf::from("/work/docs"));

        // When / Then
        assert_eq!(
            model.doc_path_of(Path::new("/work/docs/guide/setup.rst")),
            Some("guide/setup.rst".to_string())
        );
        assert_eq!(model.doc_path_of(Path::new("/elsewhere/setup.rst")), None);
        assert_eq!(model.doc_path_of(Path::new("/work/docs/data.csv")), None);
        assert_eq!(model.doc_path_of(Path::new("/work/docs/.venv/x.rst")), None);
        assert_eq!(model.doc_path_of(Path::new("/work/docs/.rst")), None);
    }

    #[test]
    fn test_doc_path_of_refuses_an_excluded_file_or_directory() {
        // Given
        let model = sphinx(&[".rst"], &["includes/*.rst", "_build"]);

        // When / Then
        assert_eq!(
            model.doc_path_of(Path::new("/work/docs/includes/a.rst")),
            None
        );
        assert_eq!(
            model.doc_path_of(Path::new("/work/docs/_build/html/x.rst")),
            None
        );
        assert_eq!(
            model.doc_path_of(Path::new("/work/docs/includes/deeper/a.rst")),
            Some("includes/deeper/a.rst".to_string())
        );
    }

    #[test]
    fn test_doc_path_of_names_every_suffix_as_rst() {
        // Given
        let model = sphinx(&[".rst", ".txt"], &[]);

        // When / Then
        assert_eq!(
            model.doc_path_of(Path::new("/work/docs/intro.txt")),
            Some("intro.rst".to_string())
        );
        assert_eq!(model.doc_path_of(Path::new("/work/docs/intro.md")), None);
    }

    #[test]
    fn test_doc_path_of_tries_the_suffixes_in_order() {
        // Given — a longer suffix first, as Sphinx tries them in order
        let model = sphinx(&[".en.rst", ".rst"], &[]);

        // When / Then
        assert_eq!(
            model.doc_path_of(Path::new("/work/docs/intro.en.rst")),
            Some("intro.rst".to_string())
        );
    }

    #[test]
    fn test_enters_refuses_a_hidden_an_excluded_or_an_outside_directory() {
        // Given
        let model = sphinx(&[".rst"], &["_build"]);

        // When / Then
        assert!(model.enters(Path::new("/work/docs")));
        assert!(model.enters(Path::new("/work/docs/guide/deep")));
        assert!(!model.enters(Path::new("/work/docs/_build/html")));
        assert!(!model.enters(Path::new("/work/docs/.venv")));
        assert!(!model.enters(Path::new("/elsewhere")));
    }

    #[test]
    fn test_source_path_places_a_name_under_the_first_suffix() {
        // Given
        let model = sphinx(&[".txt", ".rst"], &[]);

        // When / Then
        assert_eq!(
            model.source_path("guide/setup.rst"),
            PathBuf::from("/work/docs/guide/setup.txt")
        );
        assert_eq!(
            ProjectModel::folder(PathBuf::from("/w")).source_path("a/b.rst"),
            PathBuf::from("/w/a/b.rst")
        );
    }

    #[test]
    fn test_site_config_and_index_settings_carry_the_settings() {
        // Given
        let mut model = ProjectModel::folder(PathBuf::from("/w"));
        model.settings.numfig = true;
        model.settings.numfig_secnum_depth = 2;

        // When
        let config = model.site_config("contents");
        let index = model.index_settings("contents");

        // Then
        assert_eq!(config.root_doc, "contents");
        assert!(config.numfig);
        assert_eq!(config.numfig_secnum_depth, 2);
        assert_eq!(index.root_doc, "contents");
        assert_eq!(index.numfig_secnum_depth, 2);
    }

    #[test]
    fn test_parse_settings_default_to_the_builds() {
        // When
        let settings = ParseSettings::default();

        // Then
        assert_eq!(settings.domain, Domain::Py);
        assert_eq!(settings.default_role, DefaultRole::TITLE_REFERENCE);
    }
}
