//! What the server knows about one project: where its sources are, and how
//! they are parsed, indexed and rendered (ADR-038 §2).
//!
//! Every feature reads a [`ProjectModel`] and none asks which source built
//! it: a folder with no configuration, a Sphinx project read from its
//! `conf.py` and a `rinx_site` described by its Bazel manifest differ only in
//! their settings.
//!
//! The one structural difference is [`Membership`]: a folder's or a Sphinx
//! project's documents are the files its rules match, all parsed alike, while
//! a Bazel project's are exactly the files its libraries list, each parsed
//! with its own library's settings.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rinx_analyzer::IndexSettings;
use rinx_ast::{Domain, ResolvedLanguage};
use rinx_entity::EntitySchema;
use rinx_index::DomainIndex;
use rinx_parser::DefaultRole;
use rinx_renderer::config::SiteConfig;

use super::declarations::BuildDeclarations;
use super::exclusion::Exclusion;
use super::intersphinx::IntersphinxSettings;
use super::strictness::Strictness;

/// Where a project's configuration comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectSource {
    /// A workspace folder, or what of it lies outside every `conf.py`: no
    /// configuration, so every `.rst` file is a document.
    Folder,
    /// A Sphinx project, configured by the `conf.py` at `conf`.
    SphinxConf { conf: PathBuf },
    /// A `rinx_site`, named by its label (`//docs:site`) and described by
    /// the manifest its `lsp_manifest` output group wrote at `manifest`.
    Bazel { site: String, manifest: PathBuf },
}

/// Text parsed with every document — `rst_prolog` or `rst_epilog`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Splice {
    /// The file it was read from, which its diagnostics name — or `None`
    /// for a `conf.py` string, which names none.
    pub file: Option<PathBuf>,
    /// The text, as read when the project was.
    pub text: String,
}

impl Splice {
    /// Text that comes from no file — a `conf.py` string.
    #[must_use]
    pub fn unnamed(text: String) -> Self {
        Self { file: None, text }
    }
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
    /// The text parsed at the start of every document — `rst_prolog`.
    pub prolog: Option<Splice>,
    /// The text parsed at the end of every document — `rst_epilog`.
    pub epilog: Option<Splice>,
    /// The entity vocabulary documents are parsed against — converted from
    /// the project's sphinx-needs configuration. Shared, since every parse
    /// and the project's index read the one schema.
    pub schema: Arc<EntitySchema>,
    /// The names a `.. needimport::` may write for a file, each resolved to
    /// a `/`-rooted path the source root resolves — `needs_import_keys`.
    pub import_keys: BTreeMap<String, String>,
    /// The only files a directive may read, when the build declares them —
    /// a library's `parse_data`, absolute — and `None` when any file may be.
    pub parse_data: Option<BTreeSet<PathBuf>>,
    /// The names a Jinja template may read, when the document is rendered as
    /// one before it is parsed — a Bazel library's `jinja_context`, given
    /// `jinja = True` — and `None` when it is not: always outside a Bazel
    /// project, since a `source-read` hook cannot be seen (ADR-038 §5).
    pub jinja: Option<Vec<(String, String)>>,
    /// What the document's library declared, whose absence fails the build
    /// — `None` outside a Bazel project, which declares nothing.
    pub declarations: Option<BuildDeclarations>,
}

impl Default for ParseSettings {
    /// The build's own defaults, which are Sphinx's.
    fn default() -> Self {
        Self {
            domain: Domain::Py,
            default_role: DefaultRole::TITLE_REFERENCE,
            prolog: None,
            epilog: None,
            schema: Arc::new(EntitySchema::empty()),
            import_keys: BTreeMap::new(),
            parse_data: None,
            jinja: None,
            declarations: None,
        }
    }
}

/// One `rinx_library` of a Bazel project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryModel {
    /// Its label, as the build file names it.
    pub label: String,
    /// The labels of the libraries in its `deps`, as the build file names
    /// them.
    pub deps: Vec<String>,
    /// Every document it lists, by name — including those another site
    /// claimed, which the [`Listing`] leaves out, since a toctree naming one
    /// still needs this library in its `deps`.
    pub documents: BTreeSet<String>,
    /// How its documents are parsed.
    pub parse: ParseSettings,
}

/// Which files under a project's source root are its documents.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Membership {
    /// Every file with a source suffix its exclusion leaves — a folder's or
    /// a Sphinx project's rule, with one [`ParseSettings`] for all.
    #[default]
    Matched,
    /// Exactly the files listed — a Bazel project's `srcs`.
    Listed(Listing),
}

/// The documents a Bazel project's libraries list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Listing {
    documents: BTreeMap<String, ListedDocument>,
    names: BTreeMap<PathBuf, String>,
}

/// One listed document: its file, and the library listing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedDocument {
    pub path: PathBuf,
    /// Its library's index in [`ProjectSettings::libraries`].
    pub library: usize,
}

impl Listing {
    /// Lists the document `doc_path`, the file at `path`, as one of
    /// `library`'s — unless a document of that name or that file is listed
    /// already, returning whether it was.
    pub fn insert(&mut self, doc_path: String, path: PathBuf, library: usize) -> bool {
        if self.documents.contains_key(&doc_path) || self.names.contains_key(&path) {
            return false;
        }
        self.names.insert(path.clone(), doc_path.clone());
        self.documents
            .insert(doc_path, ListedDocument { path, library });
        true
    }

    /// The listed document of the name `doc_path`.
    #[must_use]
    pub fn get(&self, doc_path: &str) -> Option<&ListedDocument> {
        self.documents.get(doc_path)
    }

    /// The name of the listed document at `path`.
    #[must_use]
    pub fn name_of(&self, path: &Path) -> Option<&str> {
        self.names.get(path).map(String::as_str)
    }

    /// Keeps only the documents whose name `keep` accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.documents.retain(|doc_path, _| keep(doc_path));
        let documents = &self.documents;
        self.names
            .retain(|_, doc_path| documents.contains_key(doc_path.as_str()));
    }

    /// Whether the file at `path` is listed.
    #[must_use]
    pub fn lists(&self, path: &Path) -> bool {
        self.names.contains_key(path)
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
    /// How a document is parsed when no library says otherwise — every
    /// document, unless the [`Membership`] is listed. Its schema is the
    /// project's, which its index and every render read.
    pub parse: ParseSettings,
    /// Which files are documents.
    pub membership: Membership,
    /// The libraries a listed document's [`ListedDocument::library`]
    /// indexes; empty otherwise.
    pub libraries: Vec<LibraryModel>,
    /// The site configuration the build renders with, when the project
    /// declares one — a `rinx_site`'s `rinx.toml`; otherwise it is made of
    /// the settings here.
    pub site: Option<SiteConfig>,
    /// Whether captioned figures, tables and code blocks are numbered.
    pub numfig: bool,
    /// How many section-number components a `numfig` number starts with.
    pub numfig_secnum_depth: usize,
    /// The language a code block with none of its own is highlighted as.
    pub highlight_language: ResolvedLanguage,
    /// How findings are reported, by what the declared `extensions` explain.
    pub strictness: Strictness,
    /// Whether a broken reference fails the build — a `rinx_site`'s
    /// `strict_links`; never otherwise.
    pub strict_links: bool,
    /// The other sites the project links into — `intersphinx_mapping`.
    pub intersphinx: IntersphinxSettings,
    /// The domain index pages the site writes, which `:ref:` may name — a
    /// `rinx_site`'s `domain_indices`; none otherwise.
    pub domain_indices: BTreeSet<DomainIndex>,
    /// The files besides `conf.py` the project's configuration was read
    /// from — the `ubproject.toml` and `schemas.json` of sphinx-needs — so
    /// a change to one is a change of configuration.
    pub config_files: Vec<PathBuf>,
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
            membership: Membership::Matched,
            libraries: Vec::new(),
            site: None,
            numfig: false,
            numfig_secnum_depth: rinx_index::DEFAULT_NUMFIG_SECNUM_DEPTH,
            highlight_language: ResolvedLanguage::default(),
            strictness: Strictness::default(),
            strict_links: false,
            intersphinx: IntersphinxSettings::default(),
            domain_indices: BTreeSet::new(),
            config_files: Vec::new(),
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
        if let Membership::Listed(listing) = &self.settings.membership {
            return listing.name_of(path).map(str::to_string);
        }
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
    /// with no hidden or excluded directory on the way, itself included. A
    /// project listing its documents is never walked.
    #[must_use]
    pub fn enters(&self, directory: &Path) -> bool {
        if matches!(self.settings.membership, Membership::Listed(_)) {
            return false;
        }
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
        if let Membership::Listed(listing) = &self.settings.membership
            && let Some(listed) = listing.get(doc_path)
        {
            return listed.path.clone();
        }
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
        if let Some(site) = &self.settings.site {
            return SiteConfig {
                root_doc: root_doc.to_string(),
                ..site.clone()
            };
        }
        SiteConfig {
            root_doc: root_doc.to_string(),
            numfig: self.settings.numfig,
            numfig_secnum_depth: self.settings.numfig_secnum_depth,
            highlight_language: self.settings.highlight_language.clone(),
            ..SiteConfig::default()
        }
    }

    /// How the document named `doc_path` is parsed: with its library's
    /// settings when one lists it, else with the project's.
    #[must_use]
    pub fn parse_settings(&self, doc_path: Option<&str>) -> &ParseSettings {
        if let Membership::Listed(listing) = &self.settings.membership
            && let Some(listed) = doc_path.and_then(|doc_path| listing.get(doc_path))
            && let Some(library) = self.settings.libraries.get(listed.library)
        {
            return &library.parse;
        }
        &self.settings.parse
    }

    /// Whether this project lists the file at `path` as a document — which
    /// makes it this project's, whatever project's root is nearer.
    #[must_use]
    pub fn lists(&self, path: &Path) -> bool {
        matches!(&self.settings.membership, Membership::Listed(listing) if listing.lists(path))
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

    /// A Bazel project at `/work` whose library 0 lists `a/index.rst` and
    /// whose library 1, parsed under the `c` domain, lists `b/index.rst`.
    fn bazel() -> ProjectModel {
        let mut listing = Listing::default();
        assert!(listing.insert(
            "a/index.rst".to_string(),
            PathBuf::from("/work/a/index.rst"),
            0
        ));
        assert!(listing.insert(
            "b/index.rst".to_string(),
            PathBuf::from("/work/b/index.rst"),
            1
        ));
        let library = |label: &str, domain| LibraryModel {
            label: label.to_string(),
            deps: Vec::new(),
            documents: BTreeSet::new(),
            parse: ParseSettings {
                domain,
                ..ParseSettings::default()
            },
        };
        ProjectModel {
            source_root: PathBuf::from("/work"),
            source: ProjectSource::Bazel {
                site: "//:site".to_string(),
                manifest: PathBuf::from("/out/site.lsp_manifest.json"),
            },
            settings: ProjectSettings {
                membership: Membership::Listed(listing),
                libraries: vec![
                    library("//a:docs", Domain::Py),
                    library("//b:docs", Domain::C),
                ],
                ..ProjectSettings::default()
            },
        }
    }

    #[test]
    fn test_a_listed_project_has_exactly_its_listed_documents() {
        // Given
        let model = bazel();

        // When / Then
        assert_eq!(
            model.doc_path_of(Path::new("/work/a/index.rst")),
            Some("a/index.rst".to_string())
        );
        assert_eq!(model.doc_path_of(Path::new("/work/a/other.rst")), None);
        assert!(model.lists(Path::new("/work/b/index.rst")));
        assert!(!model.lists(Path::new("/work/b/other.rst")));
        assert!(!model.enters(Path::new("/work/a")));
        assert_eq!(
            model.source_path("b/index.rst"),
            PathBuf::from("/work/b/index.rst")
        );
    }

    #[test]
    fn test_parse_settings_are_the_listing_librarys() {
        // Given
        let model = bazel();

        // When / Then
        assert_eq!(model.parse_settings(Some("b/index.rst")).domain, Domain::C);
        assert_eq!(model.parse_settings(Some("a/index.rst")).domain, Domain::Py);
        assert_eq!(model.parse_settings(None).domain, Domain::Py);
        assert_eq!(
            ProjectModel::folder(PathBuf::from("/w"))
                .parse_settings(Some("b/index.rst"))
                .domain,
            Domain::Py
        );
    }

    #[test]
    fn test_a_listing_refuses_a_second_name_or_file_and_retains_by_name() {
        // Given
        let mut listing = Listing::default();
        assert!(listing.insert("a.rst".to_string(), PathBuf::from("/w/a.rst"), 0));

        // When / Then — neither the name nor the file is listed twice
        assert!(!listing.insert("a.rst".to_string(), PathBuf::from("/w/b.rst"), 0));
        assert!(!listing.insert("b.rst".to_string(), PathBuf::from("/w/a.rst"), 0));
        listing.retain(|doc_path| doc_path != "a.rst");
        assert!(listing.get("a.rst").is_none());
        assert!(!listing.lists(Path::new("/w/a.rst")));
    }

    #[test]
    fn test_site_config_is_the_sites_own_when_the_project_declares_one() {
        // Given
        let mut model = bazel();
        model.settings.site = Some(SiteConfig {
            project: "Docs".to_string(),
            ..SiteConfig::default()
        });

        // When
        let config = model.site_config("contents");

        // Then
        assert_eq!(config.project, "Docs");
        assert_eq!(config.root_doc, "contents");
    }

    #[test]
    fn test_parse_settings_default_to_the_builds() {
        // When
        let settings = ParseSettings::default();

        // Then
        assert_eq!(settings.domain, Domain::Py);
        assert_eq!(settings.default_role, DefaultRole::TITLE_REFERENCE);
        assert_eq!(settings.prolog, None);
        assert_eq!(settings.epilog, None);
    }
}
