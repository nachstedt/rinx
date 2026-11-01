//! Reading a `rinx_site`'s manifest into a project (ADR-038 §2, roadmap #18).
//!
//! The site's `lsp_manifest` output group writes a JSON description of the
//! site — its `rinx.toml`, entity schema and inventories — and of every
//! library it collects: the sources it lists and how it parses them. Bazel
//! writes what the server needs, so the server never queries the build
//! graph; this module only turns that description into a [`ProjectModel`].
//!
//! The manifest is read as the build reads the same attributes: a schema is
//! loaded with the parser's directive names reserved, a `default_role` is
//! checked against the role table, a `rinx.toml` is deserialized as the
//! render action deserializes it. What the build would refuse is reported
//! and keeps its default here, so one mistake does not cost the project.
//!
//! What each library declares — the documents its toctrees may name, whether
//! it compiles diagrams, and the images and downloads its site bundles — is
//! read into its [`BuildDeclarations`], whose absence fails the build (#19).
//!
//! A library's `jinja` and `jinja_context` decide whether its documents are
//! rendered as Jinja templates before they are parsed, as its parse action
//! renders them (#20). Each library's `deps` labels and the documents it
//! lists are kept for the quick fix adding a missing dependency (#21).
//!
//! The site's template, stylesheet and entity templates are its pages'
//! [`Presentation`], which the preview renders with (#22). Unknown fields are
//! ignored, and only the `version` guards the shape.
//!
//! Every file the manifest names is a source file, named by its path in the
//! workspace, or a generated one, named by its path under the execution root,
//! the only place it exists.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rinx_ast::{Diagnostic, DiagnosticCode, Domain, Position, Span};
use rinx_entity::EntitySchema;
use rinx_index::{DomainIndex, UnknownDomainIndex};
use rinx_inventory::InventoryName;
use rinx_parser::{DefaultRole, ParseCtx, ParserConfig, load_parser_config};
use rinx_renderer::config::SiteConfig;
use rinx_toctree::DeclaredDocuments;
use serde::Deserialize;

use super::declarations::{BuildDeclarations, BundledFiles};
use super::discover::DiscoveredProject;
use super::intersphinx::{IntersphinxEntry, IntersphinxSettings, InventorySource};
use super::model::{
    LibraryModel, Listing, Membership, PageFile, ParseSettings, Presentation, ProjectModel,
    ProjectSettings, ProjectSource, Splice,
};
use super::strictness::Strictness;

/// The manifest version this server reads; `rules/site.bzl` writes it.
pub const MANIFEST_VERSION: u64 = 1;

/// Where a manifest's files are found: a source file under the workspace, a
/// generated one under the execution root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BazelRoots {
    /// The directory holding `MODULE.bazel` — what a document is named
    /// relative to, since `rinx_library` names one by its path there.
    pub workspace: PathBuf,
    /// `bazel info execution_root`.
    pub execution: PathBuf,
}

/// What the client said about one workspace folder's Bazel sites: where the
/// manifests are, and which site is preferred where two list a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BazelGrant {
    pub roots: BazelRoots,
    pub manifests: Vec<PathBuf>,
    /// The label of the site a document listed by several belongs to — the
    /// editor's `rinx.site`.
    pub preferred_site: Option<String>,
}

/// The projects of the sites `grant` names, read from their manifests on
/// the disk, and every manifest that could not be read.
///
/// A document is one project's only: the preferred site's, then the first
/// site by label listing it, as the build is never asked which site a
/// document is "for". The others are read without it.
#[must_use]
pub fn read_bazel_projects(grant: &BazelGrant) -> (Vec<DiscoveredProject>, Vec<Diagnostic>) {
    let disk = |path: &Path| std::fs::read_to_string(path);
    let mut failures = Vec::new();
    let mut readings = Vec::new();
    for manifest in &grant.manifests {
        let reading = std::fs::read_to_string(manifest)
            .map_err(|error| {
                Diagnostic::without_span(
                    DiagnosticCode::BazelManifestUnreadable,
                    format!("cannot read the manifest `{}`: {error}", manifest.display()),
                )
            })
            .and_then(|text| read_bazel_manifest(manifest, &text, &grant.roots, &disk));
        match reading {
            Ok(reading) => readings.push(reading),
            Err(failure) => failures.push(failure),
        }
    }
    (
        claim_documents(readings, grant.preferred_site.as_deref()),
        failures,
    )
}

/// `readings` as projects, ordered preferred site first and then by label,
/// each without the documents an earlier one lists.
fn claim_documents(
    mut readings: Vec<BazelReading>,
    preferred_site: Option<&str>,
) -> Vec<DiscoveredProject> {
    let site_of = |reading: &BazelReading| match &reading.model.source {
        ProjectSource::Bazel { site, .. } => site.clone(),
        _ => String::new(),
    };
    readings.sort_by_key(|reading| {
        let site = site_of(reading);
        (Some(site.as_str()) != preferred_site, site)
    });
    let mut claimed = BTreeSet::new();
    readings
        .into_iter()
        .map(|mut reading| {
            reading
                .sources
                .retain(|(doc_path, _)| !claimed.contains(doc_path));
            if let Membership::Listed(listing) = &mut reading.model.settings.membership {
                listing.retain(|doc_path| !claimed.contains(doc_path));
            }
            claimed.extend(reading.sources.iter().map(|(doc_path, _)| doc_path.clone()));
            DiscoveredProject {
                model: reading.model,
                findings: reading.findings,
                conf_text: String::new(),
                sources: reading.sources,
                wants_conf_run: false,
            }
        })
        .collect()
}

/// What reading one manifest found: the project, what could not be taken
/// from it, and the project's documents with their files.
#[derive(Debug, Clone, PartialEq)]
pub struct BazelReading {
    pub model: ProjectModel,
    /// What the build would refuse, or the server could not read; the
    /// settings concerned keep their defaults.
    pub findings: Vec<Diagnostic>,
    /// Every document, by its name, with its file, in the order the
    /// libraries list them.
    pub sources: Vec<(String, PathBuf)>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    site: ManifestSite,
    #[serde(default)]
    inventories: Vec<ManifestInventory>,
    #[serde(default)]
    libraries: Vec<ManifestLibrary>,
}

#[derive(Debug, Deserialize)]
struct ManifestSite {
    label: String,
    config: ManifestFile,
    entity_schema: Option<ManifestFile>,
    /// The page template; absent, the one `rinx_site` defaults to.
    #[serde(default)]
    template: Option<ManifestFile>,
    #[serde(default)]
    css: Option<ManifestFile>,
    #[serde(default)]
    entity_templates: Vec<ManifestFile>,
    #[serde(default)]
    domain_indices: Vec<String>,
    #[serde(default)]
    strict_links: bool,
}

#[derive(Debug, Deserialize)]
struct ManifestInventory {
    name: String,
    base_url: String,
    file: ManifestFile,
}

#[derive(Debug, Deserialize)]
struct ManifestLibrary {
    label: String,
    srcs: Vec<ManifestFile>,
    /// The labels of the libraries in its `deps`.
    #[serde(default)]
    deps: Vec<String>,
    #[serde(default)]
    parse_data: Vec<String>,
    /// The documents its toctrees may name, without their `.rst`.
    #[serde(default)]
    allowed_docs: Vec<String>,
    #[serde(default)]
    images: Vec<String>,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(default)]
    diagrams: bool,
    #[serde(default)]
    jinja: bool,
    #[serde(default)]
    jinja_context: BTreeMap<String, String>,
    default_domain: String,
    default_role: Option<String>,
    rst_prolog: Option<ManifestFile>,
    rst_epilog: Option<ManifestFile>,
    entity_schema: Option<ManifestFile>,
    /// Its `parse_config`, the `rinx_parser.toml` every parse action reads.
    #[serde(default)]
    parse_config: Option<ManifestFile>,
}

/// A file the manifest names, as `rules/lsp_manifest.bzl`'s `lsp_file`
/// writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ManifestFile {
    /// A source file, by its path in its repository.
    Source(String),
    /// A generated file, by its path under the execution root.
    Generated(String),
}

impl ManifestFile {
    /// The path as the build's actions see it — what a flag passes.
    fn as_written(&self) -> &str {
        match self {
            Self::Source(path) | Self::Generated(path) => path,
        }
    }

    /// Where the file is on the disk: a source file of another repository,
    /// whose path starts `../<repository>/`, under the execution root's
    /// `external/`, where Bazel puts that repository.
    fn path(&self, roots: &BazelRoots) -> PathBuf {
        match self {
            Self::Source(path) => match path.strip_prefix("../") {
                Some(external) => join(&roots.execution.join("external"), external),
                None => join(&roots.workspace, path),
            },
            Self::Generated(path) => join(&roots.execution, path),
        }
    }

    /// The name `rinx_library` gives the document in this file — its path
    /// in the workspace, under `bazel-out/<configuration>/bin/` for a
    /// generated one — or `None` for one of another repository, which is
    /// no file of this workspace.
    fn doc_path(&self) -> Option<String> {
        let name = match self {
            Self::Source(path) => path.as_str(),
            Self::Generated(path) => {
                let mut components = path.splitn(4, '/');
                match (
                    components.next(),
                    components.next(),
                    components.next(),
                    components.next(),
                ) {
                    (Some("bazel-out"), Some(_), Some("bin"), Some(rest)) => rest,
                    _ => return None,
                }
            }
        };
        (!name.starts_with("../")).then(|| name.to_string())
    }
}

/// What a site of `libraries` bundles: every library's images and
/// downloads, by their paths in the workspace — the source root, which a
/// document's asset resolves against.
fn bundled_files(libraries: &[ManifestLibrary]) -> BundledFiles {
    BundledFiles {
        images: libraries
            .iter()
            .flat_map(|library| &library.images)
            .map(PathBuf::from)
            .collect(),
        downloads: libraries
            .iter()
            .flat_map(|library| &library.downloads)
            .map(PathBuf::from)
            .collect(),
    }
}

/// `relative`, a `/`-separated path, under `root`.
fn join(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .filter(|component| !component.is_empty())
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

/// Reads the manifest at `manifest`, whose text is `text`, into the project
/// it describes, reading the files it names with `read`.
///
/// # Errors
///
/// A `bazel.manifest-unreadable` finding when `text` is no manifest this
/// server reads: no JSON, another version, or the wrong shape.
pub fn read_bazel_manifest(
    manifest: &Path,
    text: &str,
    roots: &BazelRoots,
    read: &dyn Fn(&Path) -> io::Result<String>,
) -> Result<BazelReading, Diagnostic> {
    let unreadable = |why: String| {
        Diagnostic::without_span(
            DiagnosticCode::BazelManifestUnreadable,
            format!("the manifest `{}` {why}", manifest.display()),
        )
    };
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| unreadable(format!("is no JSON: {error}")))?;
    match value.get("version").and_then(serde_json::Value::as_u64) {
        Some(MANIFEST_VERSION) => {}
        Some(version) => {
            return Err(unreadable(format!(
                "has version {version}, and this editor reads version {MANIFEST_VERSION}; \
                 update rinx or the rules so the two match"
            )));
        }
        None => return Err(unreadable("names no version".to_string())),
    }
    let parsed: Manifest = serde_json::from_value(value)
        .map_err(|error| unreadable(format!("is not shaped as a manifest: {error}")))?;
    let mut reader = Reader {
        roots,
        read,
        findings: Vec::new(),
        files: vec![manifest.to_path_buf()],
        schemas: BTreeMap::new(),
    };
    Ok(reader.project(manifest, parsed))
}

/// The state of reading one manifest.
struct Reader<'a> {
    roots: &'a BazelRoots,
    read: &'a dyn Fn(&Path) -> io::Result<String>,
    findings: Vec<Diagnostic>,
    /// Every file read, so editing one reloads the project.
    files: Vec<PathBuf>,
    /// Each schema loaded, by its file, so the libraries sharing one share
    /// it — as the fold must, to compare them.
    schemas: BTreeMap<PathBuf, Arc<EntitySchema>>,
}

impl Reader<'_> {
    fn project(&mut self, manifest: &Path, parsed: Manifest) -> BazelReading {
        let site = self.site_config(&parsed.site.config);
        let schema = self.schema(parsed.site.entity_schema.as_ref());
        let bundled = Arc::new(bundled_files(&parsed.libraries));
        let mut listing = Listing::default();
        let mut sources = Vec::new();
        let mut libraries = Vec::new();
        for library in parsed.libraries {
            let index = libraries.len();
            for src in &library.srcs {
                let Some(doc_path) = src.doc_path() else {
                    continue;
                };
                let path = src.path(self.roots);
                if listing.insert(doc_path.clone(), path.clone(), index) {
                    sources.push((doc_path, path));
                }
            }
            libraries.push(self.library(library, &bundled));
        }
        let settings = ProjectSettings {
            root_doc: site.root_doc.clone(),
            parse: ParseSettings {
                schema,
                ..ParseSettings::default()
            },
            membership: Membership::Listed(listing),
            libraries,
            numfig: site.numfig,
            numfig_secnum_depth: site.numfig_secnum_depth,
            highlight_language: site.highlight_language.clone(),
            strictness: Strictness::Rinx,
            strict_links: parsed.site.strict_links,
            intersphinx: self.intersphinx(parsed.inventories),
            domain_indices: self.domain_indices(&parsed.site.domain_indices),
            site: Some(site),
            presentation: self.presentation(&parsed.site),
            config_files: std::mem::take(&mut self.files),
            ..ProjectSettings::default()
        };
        BazelReading {
            model: ProjectModel {
                source_root: self.roots.workspace.clone(),
                source: ProjectSource::Bazel {
                    site: parsed.site.label,
                    manifest: manifest.to_path_buf(),
                },
                settings,
            },
            findings: std::mem::take(&mut self.findings),
            sources,
        }
    }

    /// The files the site's pages are drawn with. Not read here: a page
    /// reads them when it is rendered, so an edit shows on the next one.
    fn presentation(&self, site: &ManifestSite) -> Presentation {
        let page_file = |file: Option<&ManifestFile>| {
            file.map_or(PageFile::Builtin, |file| {
                PageFile::File(file.path(self.roots))
            })
        };
        Presentation {
            template: page_file(site.template.as_ref()),
            stylesheet: page_file(site.css.as_ref()),
            entity_templates: site
                .entity_templates
                .iter()
                .map(|file| file.path(self.roots))
                .collect(),
        }
    }

    /// One library's parse settings, in a site bundling `bundled`.
    fn library(&mut self, library: ManifestLibrary, bundled: &Arc<BundledFiles>) -> LibraryModel {
        let schema = self.schema(library.entity_schema.as_ref());
        let domain = library.default_domain.parse().unwrap_or_else(|()| {
            self.invalid(format!(
                "`{}` sets `default_domain = \"{}\"`, which is no domain; it reads `py`",
                library.label, library.default_domain
            ));
            Domain::Py
        });
        let default_role =
            library
                .default_role
                .as_deref()
                .map_or(DefaultRole::TITLE_REFERENCE, |name| {
                    // As the build checks it: against every role, the schema's
                    // entity roles among them, whatever the domain.
                    let ctx = ParseCtx::with_domain(Domain::Py).with_schema(&schema);
                    DefaultRole::parse(name, &ctx).unwrap_or_else(|error| {
                        self.invalid(format!(
                            "`{}` sets `default_role = \"{name}\"`: {error}; it reads bare text as \
                         `title-reference`",
                            library.label
                        ));
                        DefaultRole::TITLE_REFERENCE
                    })
                });
        let import_keys = library
            .entity_schema
            .as_ref()
            .map(|file| schema.resolved_import_keys(file.as_written()))
            .unwrap_or_default();
        let parse_data: BTreeSet<PathBuf> = library
            .parse_data
            .iter()
            .map(|path| join(&self.roots.workspace, path))
            .collect();
        LibraryModel {
            parse: ParseSettings {
                domain,
                default_role,
                prolog: self.splice(library.rst_prolog.as_ref()),
                epilog: self.splice(library.rst_epilog.as_ref()),
                config: self.parser_config(&library.label, library.parse_config.as_ref()),
                schema,
                import_keys,
                parse_data: Some(parse_data),
                // As `library.bzl` passes it: the context only with `--jinja`,
                // in the order of its names.
                jinja: library
                    .jinja
                    .then(|| library.jinja_context.into_iter().collect()),
                declarations: Some(BuildDeclarations {
                    toctree_documents: DeclaredDocuments::from_names(
                        library.allowed_docs.iter().map(String::as_str),
                    ),
                    diagrams: library.diagrams,
                    bundled: Arc::clone(bundled),
                }),
            },
            documents: library
                .srcs
                .iter()
                .filter_map(ManifestFile::doc_path)
                .collect(),
            deps: library.deps,
            label: library.label,
        }
    }

    /// The site's `rinx.toml`, or the defaults when it cannot be read.
    fn site_config(&mut self, file: &ManifestFile) -> SiteConfig {
        let Some(text) = self.read_file(file) else {
            return SiteConfig::default();
        };
        toml::from_str(&text).unwrap_or_else(|error| {
            self.invalid(format!(
                "the site's config `{}` is no valid rinx.toml: {error}",
                file.as_written()
            ));
            SiteConfig::default()
        })
    }

    /// The schema in `file`, or the empty one without a file or when it
    /// cannot be loaded.
    fn schema(&mut self, file: Option<&ManifestFile>) -> Arc<EntitySchema> {
        let Some(file) = file else {
            return Arc::new(EntitySchema::empty());
        };
        let path = file.path(self.roots);
        if let Some(schema) = self.schemas.get(&path) {
            return Arc::clone(schema);
        }
        let schema = self
            .read_file(file)
            .and_then(|text| match rinx_parser::load_entity_schema(&text) {
                Ok(schema) => Some(schema),
                Err(errors) => {
                    self.invalid(format!(
                        "the entity schema `{}` is invalid:\n{errors}",
                        file.as_written()
                    ));
                    None
                }
            })
            .unwrap_or_else(EntitySchema::empty);
        let schema = Arc::new(schema);
        self.schemas.insert(path, Arc::clone(&schema));
        schema
    }

    /// The parse configuration in `file`, or the empty one without a file
    /// or when it cannot be loaded — which fails the build, so is reported.
    fn parser_config(&mut self, label: &str, file: Option<&ManifestFile>) -> ParserConfig {
        let Some(file) = file else {
            return ParserConfig::default();
        };
        let Some(text) = self.read_file(file) else {
            return ParserConfig::default();
        };
        load_parser_config(&text).unwrap_or_else(|errors| {
            self.invalid(format!(
                "`{label}`'s parse config `{}` is invalid: {}; it ignores no role",
                file.as_written(),
                errors.join("; ")
            ));
            ParserConfig::default()
        })
    }

    /// The prolog or epilog in `file`, named by it.
    fn splice(&mut self, file: Option<&ManifestFile>) -> Option<Splice> {
        let file = file?;
        let text = self.read_file(file)?;
        Some(Splice {
            file: Some(file.path(self.roots)),
            text,
        })
    }

    /// The inventories the site links into, each read from its one file.
    fn intersphinx(&mut self, inventories: Vec<ManifestInventory>) -> IntersphinxSettings {
        let mut entries = Vec::new();
        for inventory in inventories {
            let Ok(name) = InventoryName::new(&inventory.name) else {
                self.invalid(format!(
                    "the inventory name `{}` is invalid",
                    inventory.name
                ));
                continue;
            };
            let path = inventory.file.path(self.roots);
            self.files.push(path.clone());
            entries.push(IntersphinxEntry {
                name,
                base_url: inventory.base_url,
                sources: vec![InventorySource::File(path)],
                // No line of any file declares it that the editor could show.
                span: Span::new(Position::new(1, 1), Position::new(1, 1)),
            });
        }
        IntersphinxSettings {
            entries,
            ..IntersphinxSettings::default()
        }
    }

    /// The domain index pages the site writes; `rinx_site` refuses any
    /// other name, so one here is a manifest out of step with the rules.
    fn domain_indices(&mut self, names: &[String]) -> BTreeSet<DomainIndex> {
        names
            .iter()
            .filter_map(|name| {
                name.parse()
                    .map_err(|error: UnknownDomainIndex| self.invalid(error.to_string()))
                    .ok()
            })
            .collect()
    }

    /// The text of `file`, recorded as read — or `None`, reported, when it
    /// cannot be read.
    fn read_file(&mut self, file: &ManifestFile) -> Option<String> {
        let path = file.path(self.roots);
        self.files.push(path.clone());
        match (self.read)(&path) {
            Ok(text) => Some(text),
            Err(error) => {
                self.findings.push(Diagnostic::without_span(
                    DiagnosticCode::BazelFileUnreadable,
                    format!("cannot read `{}`: {error}", path.display()),
                ));
                None
            }
        }
    }

    fn invalid(&mut self, message: String) {
        self.findings.push(Diagnostic::without_span(
            DiagnosticCode::BazelInvalidSetting,
            message,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn roots() -> BazelRoots {
        BazelRoots {
            workspace: PathBuf::from("/work"),
            execution: PathBuf::from("/cache/execroot/_main"),
        }
    }

    /// A reader serving `files` (absolute path, text), and nothing else.
    fn disk(files: &[(&str, &str)]) -> impl Fn(&Path) -> io::Result<String> + use<> {
        let files: BTreeMap<PathBuf, String> = files
            .iter()
            .map(|(path, text)| (PathBuf::from(path), (*text).to_string()))
            .collect();
        move |path: &Path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not found"))
        }
    }

    /// A library of the label `label` listing `srcs`, with `extra` fields.
    fn library(label: &str, srcs: &[&str], extra: serde_json::Value) -> serde_json::Value {
        let mut library = json!({
            "label": label,
            "srcs": srcs.iter().map(|src| json!({"source": src})).collect::<Vec<_>>(),
            "parse_data": [],
            "default_domain": "py",
            "default_role": null,
            "rst_prolog": null,
            "rst_epilog": null,
            "entity_schema": null,
        });
        if let (Some(fields), serde_json::Value::Object(extra)) = (library.as_object_mut(), extra) {
            fields.extend(extra);
        }
        library
    }

    /// A manifest of `//docs:site` with `libraries`, and `site` fields.
    fn manifest(libraries: Vec<serde_json::Value>, site: serde_json::Value) -> String {
        let mut described = json!({
            "label": "//docs:site",
            "config": {"source": "docs/rinx.toml"},
            "entity_schema": null,
        });
        if let (Some(fields), serde_json::Value::Object(site)) = (described.as_object_mut(), site) {
            fields.extend(site);
        }
        json!({
            "version": MANIFEST_VERSION,
            "site": described,
            "inventories": [],
            "libraries": serde_json::Value::Array(libraries),
        })
        .to_string()
    }

    fn read(text: &str, files: &[(&str, &str)]) -> BazelReading {
        read_bazel_manifest(
            Path::new("/out/docs/site.lsp_manifest.json"),
            text,
            &roots(),
            &disk(files),
        )
        .expect("a manifest")
    }

    const CONFIG: (&str, &str) = (
        "/work/docs/rinx.toml",
        "project = \"Docs\"\nnumfig = true\n",
    );

    fn codes(reading: &BazelReading) -> Vec<&str> {
        reading
            .findings
            .iter()
            .map(|finding| finding.code.as_str())
            .collect()
    }

    #[test]
    fn test_a_manifest_lists_each_librarys_documents_in_the_workspace() {
        // Given
        let text = manifest(
            vec![
                library("//docs:a", &["docs/index.rst", "docs/guide.rst"], json!({})),
                library("//docs/api:b", &["docs/api/ref.rst"], json!({})),
            ],
            json!({}),
        );

        // When
        let reading = read(&text, &[CONFIG]);

        // Then
        assert_eq!(
            reading.sources,
            [
                (
                    "docs/index.rst".to_string(),
                    PathBuf::from("/work/docs/index.rst")
                ),
                (
                    "docs/guide.rst".to_string(),
                    PathBuf::from("/work/docs/guide.rst")
                ),
                (
                    "docs/api/ref.rst".to_string(),
                    PathBuf::from("/work/docs/api/ref.rst")
                ),
            ]
        );
        let model = &reading.model;
        assert_eq!(model.source_root, PathBuf::from("/work"));
        assert_eq!(
            model.source,
            ProjectSource::Bazel {
                site: "//docs:site".to_string(),
                manifest: PathBuf::from("/out/docs/site.lsp_manifest.json"),
            }
        );
        assert_eq!(
            model.doc_path_of(Path::new("/work/docs/api/ref.rst")),
            Some("docs/api/ref.rst".to_string())
        );
        assert_eq!(model.doc_path_of(Path::new("/work/docs/other.rst")), None);
        assert!(reading.findings.is_empty(), "{:?}", reading.findings);
    }

    #[test]
    fn test_each_document_is_parsed_with_its_own_librarys_settings() {
        // Given
        let text = manifest(
            vec![
                library("//a:docs", &["a/index.rst"], json!({"default_domain": "c"})),
                library(
                    "//b:docs",
                    &["b/index.rst"],
                    json!({"default_role": "any", "parse_data": ["b/data.csv"]}),
                ),
            ],
            json!({}),
        );

        // When
        let model = read(&text, &[CONFIG]).model;

        // Then
        let a = model.parse_settings(Some("a/index.rst"));
        assert_eq!(a.domain, Domain::C);
        assert_eq!(a.default_role, DefaultRole::TITLE_REFERENCE);
        let b = model.parse_settings(Some("b/index.rst"));
        assert_eq!(b.domain, Domain::Py);
        assert_ne!(b.default_role, DefaultRole::TITLE_REFERENCE);
        assert_eq!(
            b.parse_data,
            Some([PathBuf::from("/work/b/data.csv")].into_iter().collect())
        );
    }

    #[test]
    fn test_a_library_asking_for_jinja_parses_with_its_context() {
        // Given a templated library binding two names, beside a plain one
        let text = manifest(
            vec![
                library(
                    "//a:docs",
                    &["a/index.rst"],
                    json!({"jinja": true, "jinja_context": {"version": "2", "release": "0.1"}}),
                ),
                library("//b:docs", &["b/index.rst"], json!({})),
            ],
            json!({}),
        );

        // When
        let model = read(&text, &[CONFIG]).model;

        // Then the first renders with its context, sorted as the build
        // passes it, and the second is not rendered at all
        assert_eq!(
            model.parse_settings(Some("a/index.rst")).jinja,
            Some(vec![
                ("release".to_string(), "0.1".to_string()),
                ("version".to_string(), "2".to_string()),
            ])
        );
        assert_eq!(model.parse_settings(Some("b/index.rst")).jinja, None);
    }

    #[test]
    fn test_a_jinja_context_without_jinja_is_ignored() {
        // Given a context on a library that did not ask for Jinja
        let text = manifest(
            vec![library(
                "//a:docs",
                &["a/index.rst"],
                json!({"jinja": false, "jinja_context": {"release": "0.1"}}),
            )],
            json!({}),
        );

        // When
        let reading = read(&text, &[CONFIG]);

        // Then nothing is rendered, and nothing is reported: the build
        // passes no context without `--jinja` either
        assert_eq!(
            reading.model.parse_settings(Some("a/index.rst")).jinja,
            None
        );
        assert!(reading.findings.is_empty(), "{:?}", reading.findings);
    }

    #[test]
    fn test_each_library_declares_its_toctree_documents_and_diagrams() {
        // Given
        let text = manifest(
            vec![
                library(
                    "//a:docs",
                    &["a/index.rst"],
                    json!({"allowed_docs": ["a/index", "b/index"], "diagrams": true}),
                ),
                library("//b:docs", &["b/index.rst"], json!({})),
            ],
            json!({}),
        );

        // When
        let model = read(&text, &[CONFIG]).model;

        // Then
        let a = model
            .parse_settings(Some("a/index.rst"))
            .declarations
            .as_ref()
            .expect("a library declares");
        assert_eq!(
            a.toctree_documents,
            DeclaredDocuments::from_names(["a/index.rst", "b/index.rst"])
        );
        assert!(a.diagrams);
        let b = model
            .parse_settings(Some("b/index.rst"))
            .declarations
            .as_ref()
            .expect("a library declares");
        assert_eq!(b.toctree_documents, DeclaredDocuments::default());
        assert!(!b.diagrams);
    }

    #[test]
    fn test_the_site_bundles_every_librarys_images_and_downloads() {
        // Given — as the build's bundle directories, a site-wide union
        let text = manifest(
            vec![
                library(
                    "//a:docs",
                    &["a/index.rst"],
                    json!({"images": ["a/logo.png"]}),
                ),
                library(
                    "//b:docs",
                    &["b/index.rst"],
                    json!({"downloads": ["b/data.csv"]}),
                ),
            ],
            json!({}),
        );

        // When
        let model = read(&text, &[CONFIG]).model;

        // Then
        let bundled = BundledFiles {
            images: [PathBuf::from("a/logo.png")].into_iter().collect(),
            downloads: [PathBuf::from("b/data.csv")].into_iter().collect(),
        };
        for doc_path in ["a/index.rst", "b/index.rst"] {
            let declared = model.parse_settings(Some(doc_path)).declarations.as_ref();
            assert_eq!(
                declared.map(|declared| declared.bundled.as_ref()),
                Some(&bundled)
            );
        }
    }

    #[test]
    fn test_a_site_is_strict_as_its_build_is() {
        // Given
        let text = manifest(vec![], json!({"strict_links": true}));

        // When
        let settings = read(&text, &[CONFIG]).model.settings;

        // Then
        assert_eq!(settings.strictness, Strictness::Rinx);
        assert!(settings.strict_links);
        assert_eq!(settings.parse.declarations, None);
    }

    #[test]
    fn test_a_librarys_parse_config_is_read_and_watched() {
        // Given
        let text = manifest(
            vec![library(
                "//a:docs",
                &["a/index.rst"],
                json!({"parse_config": {"source": "a/rinx_parser.toml"}}),
            )],
            json!({}),
        );

        // When
        let reading = read(
            &text,
            &[
                CONFIG,
                ("/work/a/rinx_parser.toml", "ignored_roles = [\"gh\"]\n"),
            ],
        );

        // Then
        let settings = reading.model.parse_settings(Some("a/index.rst"));
        assert!(settings.config.ignored_roles.contains("gh"));
        assert!(
            reading
                .model
                .settings
                .config_files
                .contains(&PathBuf::from("/work/a/rinx_parser.toml"))
        );
    }

    #[test]
    fn test_an_invalid_parse_config_is_reported_and_ignores_nothing() {
        // Given
        let text = manifest(
            vec![library(
                "//a:docs",
                &["a/index.rst"],
                json!({"parse_config": {"source": "a/rinx_parser.toml"}}),
            )],
            json!({}),
        );

        // When
        let reading = read(
            &text,
            &[
                CONFIG,
                ("/work/a/rinx_parser.toml", "ignored_roles = [\"ref\"]\n"),
            ],
        );

        // Then
        assert!(
            reading
                .model
                .parse_settings(Some("a/index.rst"))
                .config
                .ignored_roles
                .iter()
                .next()
                .is_none()
        );
        assert!(
            reading.findings.iter().any(|finding| finding.code
                == DiagnosticCode::BazelInvalidSetting
                && finding.message.contains("`//a:docs`'s parse config")),
            "{:?}",
            reading.findings
        );
    }

    #[test]
    fn test_a_prolog_is_read_and_named_by_its_file() {
        // Given
        let text = manifest(
            vec![library(
                "//a:docs",
                &["a/index.rst"],
                json!({"rst_prolog": {"source": "a/prolog.rst"}}),
            )],
            json!({}),
        );

        // When
        let reading = read(
            &text,
            &[CONFIG, ("/work/a/prolog.rst", ".. |x| replace:: X\n")],
        );

        // Then
        let settings = reading.model.parse_settings(Some("a/index.rst"));
        assert_eq!(
            settings.prolog,
            Some(Splice {
                file: Some(PathBuf::from("/work/a/prolog.rst")),
                text: ".. |x| replace:: X\n".to_string(),
            })
        );
        assert!(
            reading
                .model
                .settings
                .config_files
                .contains(&PathBuf::from("/work/a/prolog.rst"))
        );
    }

    #[test]
    fn test_a_schema_is_loaded_once_and_shared_with_the_site() {
        // Given
        let schema = ("/work/e/entities.toml", "[[entity_type]]\nname = \"req\"\n");
        let text = manifest(
            vec![library(
                "//e:docs",
                &["e/index.rst"],
                json!({"entity_schema": {"source": "e/entities.toml"}}),
            )],
            json!({"entity_schema": {"source": "e/entities.toml"}}),
        );

        // When
        let model = read(&text, &[CONFIG, schema]).model;

        // Then — one schema, so the fold sees the library's as the site's
        let library = &model.parse_settings(Some("e/index.rst")).schema;
        assert!(library.entity_type("req").is_some());
        assert!(Arc::ptr_eq(library, &model.settings.parse.schema));
    }

    #[test]
    fn test_the_sites_config_decides_the_root_and_numbering() {
        // Given
        let text = manifest(vec![], json!({}));

        // When
        let model = read(&text, &[CONFIG]).model;

        // Then
        assert!(model.settings.numfig);
        assert_eq!(model.site_config("index").project, "Docs");
        assert_eq!(model.settings.root_doc, "index");
    }

    #[test]
    fn test_the_sites_domain_indices_are_read() {
        // Given
        let text = manifest(
            vec![],
            json!({"domain_indices": ["py-modindex", "c-modindex"]}),
        );

        // When
        let reading = read(&text, &[CONFIG]);

        // Then
        assert_eq!(
            reading.model.settings.domain_indices,
            [DomainIndex::PyModindex].into_iter().collect()
        );
        assert_eq!(codes(&reading), ["bazel.invalid-setting"]);
    }

    #[test]
    fn test_a_generated_file_is_found_under_the_execution_root() {
        // Given
        let text = manifest(
            vec![library(
                "//g:docs",
                &[],
                json!({"srcs": [{"generated": "bazel-out/k8-fastbuild/bin/g/made.rst"}]}),
            )],
            json!({}),
        );

        // When
        let reading = read(&text, &[CONFIG]);

        // Then
        assert_eq!(
            reading.sources,
            [(
                "g/made.rst".to_string(),
                PathBuf::from("/cache/execroot/_main/bazel-out/k8-fastbuild/bin/g/made.rst")
            )]
        );
    }

    #[test]
    fn test_the_sites_template_stylesheet_and_entity_templates_are_its_presentation() {
        // Given
        let text = manifest(
            Vec::new(),
            json!({
                "template": {"source": "docs/page.html"},
                "css": {"generated": "bazel-out/k8-fastbuild/bin/docs/site.css"},
                "entity_templates": [{"source": "docs/req.html"}],
            }),
        );

        // When
        let reading = read(&text, &[CONFIG]);

        // Then
        assert_eq!(
            reading.model.settings.presentation,
            Presentation {
                template: PageFile::File(PathBuf::from("/work/docs/page.html")),
                stylesheet: PageFile::File(PathBuf::from(
                    "/cache/execroot/_main/bazel-out/k8-fastbuild/bin/docs/site.css"
                )),
                entity_templates: vec![PathBuf::from("/work/docs/req.html")],
            }
        );
    }

    #[test]
    fn test_a_site_naming_no_template_is_presented_with_the_builtin_one() {
        // Given
        let text = manifest(Vec::new(), json!({}));

        // When / Then
        assert_eq!(
            read(&text, &[CONFIG]).model.settings.presentation,
            Presentation::default()
        );
    }

    #[test]
    fn test_a_source_file_of_another_repository_is_found_under_the_execution_root() {
        // Given — what `rinx_site`'s default template is in a consuming module
        let file = ManifestFile::Source("../rinx+/templates/default.html".to_string());

        // When / Then
        assert_eq!(
            file.path(&roots()),
            PathBuf::from("/cache/execroot/_main/external/rinx+/templates/default.html")
        );
    }

    #[test]
    fn test_a_document_of_another_repository_is_not_listed() {
        // Given
        let text = manifest(
            vec![library("//x:docs", &["../dep/index.rst"], json!({}))],
            json!({}),
        );

        // When / Then
        assert!(read(&text, &[CONFIG]).sources.is_empty());
    }

    #[test]
    fn test_an_inventory_is_one_file_read_from_the_disk() {
        // Given
        let mut text: serde_json::Value =
            serde_json::from_str(&manifest(vec![], json!({}))).expect("JSON");
        text["inventories"] = json!([{
            "name": "python",
            "base_url": "https://docs.python.org/3/",
            "file": {"generated": "bazel-out/k8/bin/py/objects.inv"},
        }]);

        // When
        let model = read(&text.to_string(), &[CONFIG]).model;

        // Then
        let entries = &model.settings.intersphinx.entries;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name.as_str(), "python");
        assert_eq!(
            entries[0].sources,
            [InventorySource::File(PathBuf::from(
                "/cache/execroot/_main/bazel-out/k8/bin/py/objects.inv"
            ))]
        );
    }

    #[test]
    fn test_what_the_build_would_refuse_is_reported_and_defaulted() {
        // Given
        let text = manifest(
            vec![library(
                "//a:docs",
                &["a/index.rst"],
                json!({
                    "default_domain": "rust",
                    "default_role": "no-such-role",
                    "entity_schema": {"source": "a/entities.toml"},
                    "rst_epilog": {"source": "a/missing.rst"},
                }),
            )],
            json!({}),
        );
        let files = [
            ("/work/docs/rinx.toml", "numfig = \"yes\"\n"),
            ("/work/a/entities.toml", "[[entity_type]]\n"),
        ];

        // When
        let reading = read(&text, &files);

        // Then
        assert_eq!(
            codes(&reading),
            [
                "bazel.invalid-setting",
                "bazel.invalid-setting",
                "bazel.invalid-setting",
                "bazel.invalid-setting",
                "bazel.file-unreadable",
            ]
        );
        let settings = reading.model.parse_settings(Some("a/index.rst"));
        assert_eq!(settings.domain, Domain::Py);
        assert_eq!(settings.default_role, DefaultRole::TITLE_REFERENCE);
        assert!(settings.schema.is_empty());
        assert_eq!(settings.epilog, None);
    }

    #[test]
    fn test_an_unknown_version_is_refused_by_name() {
        // Given
        let text = json!({"version": 2, "site": {}}).to_string();

        // When
        let refused = read_bazel_manifest(Path::new("/m.json"), &text, &roots(), &disk(&[]))
            .expect_err("version 2");

        // Then
        assert_eq!(refused.code, DiagnosticCode::BazelManifestUnreadable);
        assert!(refused.message.contains("version 2"), "{}", refused.message);
    }

    #[test]
    fn test_no_json_and_the_wrong_shape_are_refused() {
        for text in ["{", r#"{"version": 1}"#, r#"{"site": {}}"#] {
            // When
            let refused = read_bazel_manifest(Path::new("/m.json"), text, &roots(), &disk(&[]));

            // Then
            assert_eq!(
                refused.map(|_| ()).map_err(|refused| refused.code),
                Err(DiagnosticCode::BazelManifestUnreadable),
                "{text}"
            );
        }
    }

    #[test]
    fn test_a_document_belongs_to_the_preferred_site_then_the_first_by_label() {
        // Given three sites listing `shared.rst`, read in no particular order
        let site = |label: &str| {
            let text = manifest(
                vec![library("//lib:docs", &["shared.rst", "own.rst"], json!({}))],
                json!({"label": label}),
            );
            read(&text, &[CONFIG])
        };
        let readings = || vec![site("//c:site"), site("//a:site"), site("//b:site")];
        let owner = |projects: &[DiscoveredProject]| {
            projects
                .iter()
                .find(|project| project.sources.iter().any(|(doc, _)| doc == "shared.rst"))
                .map(|project| match &project.model.source {
                    ProjectSource::Bazel { site, .. } => site.clone(),
                    _ => String::new(),
                })
        };

        // When
        let preferred = claim_documents(readings(), Some("//b:site"));
        let unpreferred = claim_documents(readings(), None);

        // Then — one owner each, the others reading the site without it
        assert_eq!(owner(&preferred).as_deref(), Some("//b:site"));
        assert_eq!(owner(&unpreferred).as_deref(), Some("//a:site"));
        let later = &unpreferred[1].model;
        assert_eq!(later.doc_path_of(Path::new("/work/shared.rst")), None);
        assert_eq!(unpreferred[1].sources, []);
    }

    #[test]
    fn test_each_library_keeps_its_deps_and_documents() {
        // Given
        let text = manifest(
            vec![
                library(
                    "//docs:a",
                    &["docs/index.rst"],
                    json!({"deps": ["//docs/api:b"]}),
                ),
                library("//docs/api:b", &["docs/api/ref.rst"], json!({})),
            ],
            json!({}),
        );

        // When
        let reading = read(&text, &[CONFIG]);

        // Then
        let libraries = &reading.model.settings.libraries;
        assert_eq!(libraries[0].deps, ["//docs/api:b"]);
        assert_eq!(libraries[1].deps, Vec::<String>::new());
        assert_eq!(
            libraries[1].documents,
            BTreeSet::from(["docs/api/ref.rst".to_string()])
        );
    }

    #[test]
    fn test_a_library_keeps_the_documents_another_site_claimed() {
        // Given two sites listing `shared.rst`, the first claiming it
        let site = |label: &str| {
            let text = manifest(
                vec![library("//lib:docs", &["shared.rst"], json!({}))],
                json!({"label": label}),
            );
            read(&text, &[CONFIG])
        };

        // When
        let projects = claim_documents(vec![site("//a:site"), site("//b:site")], None);

        // Then — the second site's library still lists it, for the deps a
        // toctree naming it needs
        let later = &projects[1].model;
        assert_eq!(later.doc_path_of(Path::new("/work/shared.rst")), None);
        assert!(later.settings.libraries[0].documents.contains("shared.rst"));
    }

    #[test]
    fn test_doc_path_strips_the_output_directory_of_a_generated_file() {
        assert_eq!(
            ManifestFile::Generated("bazel-out/k8-opt/bin/a/b.rst".to_string()).doc_path(),
            Some("a/b.rst".to_string())
        );
        assert_eq!(
            ManifestFile::Generated("elsewhere/b.rst".to_string()).doc_path(),
            None
        );
        assert_eq!(
            ManifestFile::Source("a/b.rst".to_string()).doc_path(),
            Some("a/b.rst".to_string())
        );
    }
}
