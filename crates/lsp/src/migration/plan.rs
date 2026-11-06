//! What migrating one Sphinx project writes, worked out from what the server
//! already knows of it.
//!
//! The project's documents were parsed by the server's own scan, so what
//! each read, showed, linked and drew is known exactly: the library declares
//! those files, and no globbing heuristic stands between a document and the
//! build that must find what it names.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use rinx_ast::Domain;

use super::build_file::BuildFile;
use super::finding::MigrationFinding;
use super::module_bazel::{MODULE_FILE, appended_dependency, depends_on_rinx, new_module};
use super::package::{PackageListing, enclosing_module, is_rst, list_package, relative_to};
use super::rinx_toml::SiteSettings;
use super::srcs::srcs_excludes;
use crate::project::{ConfFacts, ConvertedNeeds, PARSER_CONFIG_FILE, Project, ProjectSource};

/// The build file a migration writes beside `conf.py`.
const BUILD_FILE: &str = "BUILD.bazel";
/// The site configuration it writes.
const SITE_CONFIG_FILE: &str = "rinx.toml";
/// The entity schema it writes for a sphinx-needs project.
const ENTITY_SCHEMA_FILE: &str = "entities.toml";
/// The files `rst_prolog` and `rst_epilog` are written into — not `.rst`, so
/// neither the library's glob nor Sphinx takes them for documents.
const PROLOG_FILE: &str = "rst_prolog.txt";
const EPILOG_FILE: &str = "rst_epilog.txt";
/// The convenience symlinks Bazel creates in its workspace's root.
const OUTPUT_SYMLINKS: &str = "bazel-*/**";

/// What a migration may be told beyond what the project says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationOptions {
    /// Render every document as a Jinja template first — what a
    /// `source-read` hook in `conf.py` does, which reading it cannot see.
    pub jinja: bool,
    /// Further files the documents read at parse time, absolute: copies of
    /// what [`MigrationFinding::OutsideSourceRoot`] named.
    pub parse_data: Vec<PathBuf>,
}

/// The files a migration writes, and what it could not carry over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPlan {
    pub files: Vec<PlannedFile>,
    pub findings: Vec<MigrationFinding>,
}

/// One file a migration writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    /// Absolute.
    pub path: PathBuf,
    pub content: PlannedContent,
}

/// How a planned file is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedContent {
    /// A new file, with this text.
    Create(String),
    /// This text added to the end of the existing file.
    Append(String),
}

impl MigrationPlan {
    /// The files the plan would create that exist already — which a
    /// migration never overwrites.
    #[must_use]
    pub fn conflicts(&self) -> Vec<&Path> {
        self.files
            .iter()
            .filter(|file| matches!(file.content, PlannedContent::Create(_)) && file.path.exists())
            .map(|file| file.path.as_path())
            .collect()
    }
}

/// What migrating `project` writes, or `None` when it is no Sphinx project.
pub fn plan_migration(project: &mut Project, options: &MigrationOptions) -> Option<MigrationPlan> {
    let ProjectSource::SphinxConf { .. } = project.model().source else {
        return None;
    };
    let py_modules = !project.project_index().modules.is_empty();
    let root = project.root().to_path_buf();
    let settings = &project.model().settings;
    let facts = settings.conf_facts.clone().unwrap_or_default();
    let module = enclosing_module(&root);
    let package = module
        .as_deref()
        .and_then(Path::parent)
        .and_then(|workspace| relative_to(&root, workspace))
        .unwrap_or_default();
    let listing = list_package(&root);
    let mut planner = Planner {
        root: &root,
        listing: &listing,
        findings: facts
            .unread
            .iter()
            .map(|setting| MigrationFinding::UnreadSetting {
                setting: setting.clone(),
            })
            .collect(),
    };

    let mut documents = BTreeSet::new();
    let mut reads = BTreeSet::new();
    let mut images = BTreeSet::new();
    let mut downloads = BTreeSet::new();
    let mut diagrams = false;
    for indexed in project.indexed() {
        if let Some(document) = planner.document(&indexed.path) {
            documents.insert(document);
        }
        reads.extend(indexed.reads.iter().cloned());
        images.extend(indexed.required.images.iter().map(|path| root.join(path)));
        downloads.extend(
            indexed
                .required
                .downloads
                .iter()
                .map(|path| root.join(path)),
        );
        diagrams |= indexed.required.diagrams;
    }
    reads.extend(options.parse_data.iter().cloned());

    let mut srcs_excludes = srcs_excludes(&listing.rst_files, &documents);
    srcs_excludes.extend(
        listing
            .linked_directories
            .iter()
            .map(|directory| format!("{directory}/**")),
    );
    if package.is_empty() {
        srcs_excludes.push(OUTPUT_SYMLINKS.to_string());
    }
    srcs_excludes.sort();
    srcs_excludes.dedup();

    let mut files = Vec::new();
    let mut create = |name: &str, text: String| {
        files.push(PlannedFile {
            path: root.join(name),
            content: PlannedContent::Create(text),
        });
        Some(name.to_string())
    };
    let rst_prolog = settings
        .parse
        .prolog
        .as_ref()
        .and_then(|splice| create(PROLOG_FILE, with_final_newline(&splice.text)));
    let rst_epilog = settings
        .parse
        .epilog
        .as_ref()
        .and_then(|splice| create(EPILOG_FILE, with_final_newline(&splice.text)));
    let entity_schema = match &facts.needs {
        Some(ConvertedNeeds::Schema { toml, unconverted }) => {
            planner.findings.extend(unconverted.iter().map(|entry| {
                MigrationFinding::NeedsUnconverted {
                    entry: entry.clone(),
                }
            }));
            create(ENTITY_SCHEMA_FILE, toml.clone())
        }
        Some(ConvertedNeeds::Invalid) => {
            planner.findings.push(MigrationFinding::NeedsInvalid);
            None
        }
        None => None,
    };
    planner
        .findings
        .extend(
            settings
                .intersphinx
                .entries
                .iter()
                .map(|entry| MigrationFinding::Inventory {
                    name: entry.name.as_str().to_string(),
                }),
        );

    let build = BuildFile {
        srcs_excludes,
        parse_data: planner.declared(&reads),
        images: planner.declared(&images),
        downloads: planner.declared(&downloads),
        default_domain: (settings.parse.domain != Domain::Py)
            .then(|| settings.parse.domain.as_str().to_string()),
        default_role: settings.parse.default_role.role_name().map(str::to_string),
        rst_prolog,
        rst_epilog,
        diagrams,
        jinja: options.jinja || settings.parse.jinja.is_some(),
        entity_schema,
        parse_config: root
            .join(PARSER_CONFIG_FILE)
            .is_file()
            .then(|| PARSER_CONFIG_FILE.to_string()),
        config: SITE_CONFIG_FILE.to_string(),
        py_modindex: facts.py_modindex && py_modules,
    };
    create(BUILD_FILE, build.render());
    let site = site_settings(&facts, settings, &package, project.root_doc());
    create(SITE_CONFIG_FILE, site.render());
    if let Some(file) = module_file(module, &root, &facts) {
        files.push(file);
    }
    Some(MigrationPlan {
        files,
        findings: planner.findings,
    })
}

/// The site settings `conf.py` sets, its root document named from the
/// workspace's root as the build names it.
fn site_settings(
    facts: &ConfFacts,
    settings: &crate::project::ProjectSettings,
    package: &str,
    root_doc: &str,
) -> SiteSettings {
    SiteSettings {
        project: facts.project.clone(),
        version: facts.version.clone(),
        root_doc: if package.is_empty() {
            root_doc.to_string()
        } else {
            format!("{package}/{root_doc}")
        },
        numfig: settings.numfig,
        numfig_secnum_depth: settings.numfig_secnum_depth,
        highlight_language: settings.highlight_language.clone(),
        pep_base_url: facts.pep_base_url.clone(),
        rfc_base_url: facts.rfc_base_url.clone(),
        modindex_common_prefix: facts.modindex_common_prefix.clone(),
    }
}

/// What the migration writes of the Bazel module: the dependency on rinx
/// appended to the enclosing `module` when it lacks one, or a module of its
/// own at `root` when there is none.
fn module_file(module: Option<PathBuf>, root: &Path, facts: &ConfFacts) -> Option<PlannedFile> {
    let Some(module) = module else {
        return Some(PlannedFile {
            path: root.join(MODULE_FILE),
            content: PlannedContent::Create(new_module(facts.project.as_deref())),
        });
    };
    let text = fs::read_to_string(&module).unwrap_or_default();
    (!depends_on_rinx(&text)).then(|| PlannedFile {
        content: PlannedContent::Append(appended_dependency(&text)),
        path: module,
    })
}

/// Where, from `root`, the build looks for `path`, a file outside it that a
/// document names: the build's parser resolves a path climbing above the
/// source root as if it stopped there, so what follows the climb — `path`
/// below its nearest common ancestor with `root` — is read from the root.
fn build_path_of_outside(path: &Path, root: &Path) -> PathBuf {
    root.ancestors()
        .find_map(|ancestor| path.strip_prefix(ancestor).ok())
        .map_or_else(|| path.to_path_buf(), Path::to_path_buf)
}

fn with_final_newline(text: &str) -> String {
    if text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{text}\n")
    }
}

/// Sorts the files a project's documents name into what its package can
/// declare, reporting the rest.
struct Planner<'a> {
    root: &'a Path,
    listing: &'a PackageListing,
    findings: Vec<MigrationFinding>,
}

impl Planner<'_> {
    /// The document at `path` as `srcs` holds it, or `None`, reported, when
    /// the library cannot hold it.
    fn document(&mut self, path: &Path) -> Option<String> {
        let relative = relative_to(path, self.root)?;
        if self.listing.in_subpackage(&relative) {
            self.findings.push(MigrationFinding::InSubpackage {
                path: path.to_path_buf(),
            });
            return None;
        }
        if !is_rst(path) {
            self.findings.push(MigrationFinding::NotReStructuredText {
                path: path.to_path_buf(),
            });
            return None;
        }
        Some(relative)
    }

    /// Each of `paths`, absolute, as the library declares it — those it
    /// cannot left out and reported.
    fn declared(&mut self, paths: &BTreeSet<PathBuf>) -> Vec<String> {
        let mut declared = Vec::new();
        for path in paths {
            let finding = match relative_to(path, self.root) {
                None => MigrationFinding::OutsideSourceRoot {
                    path: path.clone(),
                    build_path: build_path_of_outside(path, self.root),
                },
                Some(_) if !path.is_file() => MigrationFinding::MissingFile { path: path.clone() },
                Some(relative) if self.listing.in_subpackage(&relative) => {
                    MigrationFinding::InSubpackage { path: path.clone() }
                }
                Some(relative) => {
                    declared.push(relative);
                    continue;
                }
            };
            if !self.findings.contains(&finding) {
                self.findings.push(finding);
            }
        }
        declared.sort();
        declared
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_path_of_outside_is_what_follows_the_climb_above_the_root() {
        // When / Then — `../pharaoh.toml` from the root, `../../shared/a.txt`
        // from a document one level down
        assert_eq!(
            build_path_of_outside(Path::new("/w/pharaoh.toml"), Path::new("/w/docs")),
            PathBuf::from("pharaoh.toml")
        );
        assert_eq!(
            build_path_of_outside(Path::new("/w/shared/a.txt"), Path::new("/w/docs")),
            PathBuf::from("shared/a.txt")
        );
    }

    #[test]
    fn test_with_final_newline_ends_the_text_with_one_newline() {
        // When / Then
        assert_eq!(with_final_newline("a"), "a\n");
        assert_eq!(with_final_newline("a\n"), "a\n");
    }

    #[test]
    fn test_module_file_creates_a_module_where_there_is_none() {
        // Given
        let root = std::env::temp_dir().join("rinx_lsp_plan_no_module");

        // When
        let file = module_file(None, &root, &ConfFacts::default()).expect("a module");

        // Then
        assert_eq!(file.path, root.join(MODULE_FILE));
        assert!(matches!(file.content, PlannedContent::Create(_)));
    }

    #[test]
    fn test_site_settings_name_the_root_document_from_the_workspace_root() {
        // Given
        let settings = crate::project::ProjectSettings::default();

        // When
        let nested = site_settings(&ConfFacts::default(), &settings, "Doc", "contents");
        let rooted = site_settings(&ConfFacts::default(), &settings, "", "index");

        // Then
        assert_eq!(nested.root_doc, "Doc/contents");
        assert_eq!(rooted.root_doc, "index");
    }
}
