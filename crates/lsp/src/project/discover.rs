//! Finding a workspace folder's projects, and each project's documents.
//!
//! A directory holding a `conf.py` is the source root of a Sphinx project,
//! and a file belongs to the nearest project above it (ADR-038 §2); what lies
//! under no `conf.py` belongs to the folder itself, a project with no
//! configuration. One walk finds both, top-down, so a directory a project
//! excludes is never entered — not even to look for a `conf.py` in it — as
//! Sphinx never enters it.
//!
//! Two rules hold for every project, whatever it configures:
//!
//! - **A symlinked directory is not followed.** A Bazel workspace holds
//!   `bazel-bin`, `bazel-out` and friends — symlinks into the build's output,
//!   which mirror the whole source tree — so following them would index every
//!   document twice. Following links also risks a cycle. A symlinked *file* is
//!   a document like any other.
//! - **A hidden directory is skipped**: `.git`, `.venv`, `.tox` and the like
//!   hold tooling, never documentation.
//!
//! `.gitignore` is deliberately not consulted: which files a project
//! documents is a decision of its own configuration — `exclude_patterns` —
//! and generated sources, exactly what an ignore file lists, are often part
//! of it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rinx_ast::{Diagnostic, DiagnosticCode};

use super::model::{ProjectModel, ProjectSource};
use super::sphinx_conf::read_sphinx_conf;

/// The file that makes a directory a Sphinx project's source root.
pub(crate) const CONF_FILE: &str = "conf.py";

/// One project of a workspace folder, as found on the disk: its model, what
/// reading its `conf.py` found, and its documents, before any is parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredProject {
    pub model: ProjectModel,
    /// What reading `conf.py` found, positioned in it; empty for a folder.
    pub findings: Vec<Diagnostic>,
    /// The text of `conf.py`, which the findings' positions count in.
    pub conf_text: String,
    /// Every document, by its name, with its file — in the order Sphinx
    /// finds them.
    pub sources: Vec<(String, PathBuf)>,
}

impl DiscoveredProject {
    /// The project of a folder with no configuration at `root`, before its
    /// documents are found.
    #[must_use]
    pub fn folder(root: PathBuf) -> Self {
        Self {
            model: ProjectModel::folder(root),
            findings: Vec::new(),
            conf_text: String::new(),
            sources: Vec::new(),
        }
    }
}

/// Every project under the workspace folder at `folder`, the folder's own
/// first — a Sphinx project when `conf.py` is at its root, else one with no
/// configuration — and nested ones in the order the walk meets them.
#[must_use]
pub fn discover_projects(folder: &Path) -> Vec<DiscoveredProject> {
    let mut projects = vec![
        sphinx_project_at(folder)
            .unwrap_or_else(|| DiscoveredProject::folder(folder.to_path_buf())),
    ];
    walk(folder, 0, &mut projects);
    projects
}

/// The documents of the project `model` under `directory`, which holds no
/// `conf.py` of its own (see [`contains_conf`]).
#[must_use]
pub fn sources_under(model: &ProjectModel, directory: &Path) -> Vec<(String, PathBuf)> {
    let mut projects = vec![DiscoveredProject {
        model: model.clone(),
        findings: Vec::new(),
        conf_text: String::new(),
        sources: Vec::new(),
    }];
    walk(directory, 0, &mut projects);
    projects.swap_remove(0).sources
}

/// Whether a `conf.py` is at `path` or anywhere the walk would find one
/// under it.
#[must_use]
pub fn contains_conf(path: &Path) -> bool {
    if is_conf(path) {
        return true;
    }
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() && !is_hidden(&path) => pending.push(path),
                Ok(_) if is_conf(&path) => return true,
                _ => {}
            }
        }
    }
    false
}

/// Whether `path` names a `conf.py`.
#[must_use]
pub fn is_conf(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == CONF_FILE)
}

/// Whether `path` lies under `root` with no hidden component in between,
/// itself included.
#[must_use]
pub fn is_visible_under(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok_and(|relative| {
        !relative
            .components()
            .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
    })
}

/// The Sphinx project whose `conf.py` is in `directory`, if there is one. A
/// `conf.py` that cannot be read configures nothing, as an empty one would.
fn sphinx_project_at(directory: &Path) -> Option<DiscoveredProject> {
    let conf = directory.join(CONF_FILE);
    if !conf.is_file() {
        return None;
    }
    let text = fs::read_to_string(&conf).unwrap_or_default();
    let reading = read_sphinx_conf(&text);
    Some(DiscoveredProject {
        model: ProjectModel {
            source_root: directory.to_path_buf(),
            source: ProjectSource::SphinxConf { conf },
            settings: reading.settings,
        },
        findings: reading.findings,
        conf_text: text,
        sources: Vec::new(),
    })
}

/// Adds the documents in `directory`, and under it, to `projects[current]`
/// — or to the project a deeper `conf.py` starts, added to `projects`.
fn walk(directory: &Path, current: usize, projects: &mut Vec<DiscoveredProject>) {
    let Ok(entries) = fs::read_dir(directory) else {
        // An unreadable corner of a workspace costs its own documents, not
        // everyone else's.
        return;
    };
    let mut files = Vec::new();
    let mut directories = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        // `DirEntry::file_type` does not follow symlinks, which is the whole
        // point: a symlinked directory reports as a symlink.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if !is_hidden(&path) {
                directories.push(path);
            }
        } else if kind.is_file() || path.is_file() {
            files.push(path);
        }
    }
    // Sphinx walks each directory's files in name order, and the first file
    // of a document is the one it reads.
    files.sort();
    directories.sort();
    let mut seen: BTreeMap<String, PathBuf> = BTreeMap::new();
    for file in files {
        let project = &mut projects[current];
        let Some(doc_path) = project.model.doc_path_of(&file) else {
            continue;
        };
        if let Some(first) = seen.get(&doc_path) {
            project.findings.push(duplicate_source(first, &file));
            continue;
        }
        seen.insert(doc_path.clone(), file.clone());
        project.sources.push((doc_path, file));
    }
    for subdirectory in directories {
        if is_excluded(&projects[current].model, &subdirectory) {
            continue;
        }
        match sphinx_project_at(&subdirectory) {
            Some(nested) => {
                projects.push(nested);
                let index = projects.len() - 1;
                walk(&subdirectory, index, projects);
            }
            None => walk(&subdirectory, current, projects),
        }
    }
}

/// Whether `model` leaves out the directory at `path`, below its root.
fn is_excluded(model: &ProjectModel, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(&model.source_root) else {
        return false;
    };
    let relative: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    model
        .settings
        .exclusion
        .excludes_directory(&relative.join("/"))
}

/// The finding that `second` is the same document as `first`, which wins.
fn duplicate_source(first: &Path, second: &Path) -> Diagnostic {
    let name = |path: &Path| {
        path.file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    };
    Diagnostic::without_span(
        DiagnosticCode::ConfDuplicateSource,
        format!(
            "`{}` and `{}` are one document under two source suffixes; `{}` is used",
            name(first),
            name(second),
            name(first)
        ),
    )
}

/// Whether the last component of `path` starts with a dot.
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory under the system's temporary one, holding `files`
    /// (relative paths, each written with `text`).
    fn temp_tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rinx_lsp_discover_{name}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("mkdir");
        for (file, text) in files {
            let path = root.join(file);
            fs::create_dir_all(path.parent().expect("a file has a parent")).expect("mkdir");
            fs::write(&path, text).expect("write");
        }
        root
    }

    fn names(project: &DiscoveredProject) -> Vec<&str> {
        project
            .sources
            .iter()
            .map(|(doc_path, _)| doc_path.as_str())
            .collect()
    }

    fn roots(root: &Path, projects: &[DiscoveredProject]) -> Vec<String> {
        projects
            .iter()
            .map(|project| {
                project
                    .model
                    .source_root
                    .strip_prefix(root)
                    .expect("under the folder")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn test_discover_projects_finds_every_rst_file_of_a_plain_folder() {
        // Given
        let root = temp_tree(
            "nested",
            &[
                ("index.rst", "x"),
                ("guide/setup.rst", "x"),
                ("guide/notes.txt", "x"),
                ("a/b/c.rst", "x"),
            ],
        );

        // When
        let projects = discover_projects(&root);

        // Then
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].model.source, ProjectSource::Folder);
        assert_eq!(
            names(&projects[0]),
            ["index.rst", "a/b/c.rst", "guide/setup.rst"]
        );
    }

    #[test]
    fn test_discover_projects_skips_hidden_directories() {
        // Given
        let root = temp_tree(
            "hidden",
            &[
                ("index.rst", "x"),
                (".venv/lib/readme.rst", "x"),
                (".git/x.rst", "x"),
            ],
        );

        // When / Then
        assert_eq!(names(&discover_projects(&root)[0]), ["index.rst"]);
    }

    #[cfg(unix)]
    #[test]
    fn test_discover_projects_does_not_follow_a_symlinked_directory() {
        // Given — a Bazel-style convenience link mirroring the tree.
        let root = temp_tree("symlink", &[("index.rst", "x")]);
        std::os::unix::fs::symlink(&root, root.join("bazel-out")).expect("symlink");

        // When / Then — the document once, not again through the link.
        assert_eq!(names(&discover_projects(&root)[0]), ["index.rst"]);
    }

    #[cfg(unix)]
    #[test]
    fn test_discover_projects_keeps_a_symlinked_file() {
        // Given — a generated document linked into the tree.
        let root = temp_tree("linked_file", &[("generated/api.rst", "x")]);
        std::os::unix::fs::symlink(root.join("generated/api.rst"), root.join("api.rst"))
            .expect("symlink");

        // When / Then
        assert_eq!(
            names(&discover_projects(&root)[0]),
            ["api.rst", "generated/api.rst"]
        );
    }

    #[test]
    fn test_discover_projects_reads_a_conf_py_at_the_folders_root() {
        // Given
        let root = temp_tree(
            "conf_at_root",
            &[
                (
                    "conf.py",
                    "root_doc = 'contents'\nexclude_patterns = ['drafts']\n",
                ),
                ("contents.rst", "x"),
                ("drafts/wip.rst", "x"),
            ],
        );

        // When
        let projects = discover_projects(&root);

        // Then
        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0].model.source,
            ProjectSource::SphinxConf {
                conf: root.join("conf.py")
            }
        );
        assert_eq!(projects[0].model.settings.root_doc, "contents");
        assert_eq!(names(&projects[0]), ["contents.rst"]);
        assert!(projects[0].conf_text.contains("contents"));
    }

    #[test]
    fn test_discover_projects_gives_a_file_to_the_nearest_project() {
        // Given
        let root = temp_tree(
            "nearest",
            &[
                ("README.rst", "x"),
                ("docs/conf.py", ""),
                ("docs/index.rst", "x"),
                ("docs/api/conf.py", ""),
                ("docs/api/index.rst", "x"),
                ("tools/notes.rst", "x"),
            ],
        );

        // When
        let projects = discover_projects(&root);

        // Then
        assert_eq!(roots(&root, &projects), ["", "docs", "docs/api"]);
        assert_eq!(names(&projects[0]), ["README.rst", "tools/notes.rst"]);
        assert_eq!(names(&projects[1]), ["index.rst"]);
        assert_eq!(names(&projects[2]), ["index.rst"]);
    }

    #[test]
    fn test_discover_projects_does_not_look_inside_an_excluded_directory() {
        // Given — a virtual environment inside the project, as CPython's
        // `venv/*` excludes
        let root = temp_tree(
            "excluded_conf",
            &[
                ("conf.py", "exclude_patterns = ['venv/*']\n"),
                ("index.rst", "x"),
                ("venv/lib/pkg/conf.py", ""),
                ("venv/lib/pkg/readme.rst", "x"),
            ],
        );

        // When
        let projects = discover_projects(&root);

        // Then
        assert_eq!(projects.len(), 1);
        assert_eq!(names(&projects[0]), ["index.rst"]);
    }

    #[test]
    fn test_discover_projects_reports_one_document_under_two_suffixes() {
        // Given
        let root = temp_tree(
            "duplicate",
            &[
                ("conf.py", "source_suffix = ['.txt', '.rst']\n"),
                ("intro.rst", "x"),
                ("intro.txt", "x"),
            ],
        );

        // When
        let projects = discover_projects(&root);

        // Then — the first file by name, as Sphinx's walk meets it
        assert_eq!(
            projects[0].sources,
            [("intro.rst".to_string(), root.join("intro.rst"))]
        );
        assert_eq!(projects[0].findings.len(), 1);
        assert_eq!(
            projects[0].findings[0].code,
            DiagnosticCode::ConfDuplicateSource
        );
        assert!(
            projects[0].findings[0]
                .message
                .contains("`intro.rst` is used")
        );
    }

    #[test]
    fn test_discover_projects_finds_nothing_in_a_missing_folder() {
        // Given
        let root = std::env::temp_dir().join("rinx_lsp_discover_missing_folder_does_not_exist");

        // When
        let projects = discover_projects(&root);

        // Then
        assert_eq!(projects.len(), 1);
        assert!(projects[0].sources.is_empty());
    }

    #[test]
    fn test_sources_under_finds_a_projects_documents_in_one_directory() {
        // Given
        let root = temp_tree(
            "sources_under",
            &[
                ("conf.py", "exclude_patterns = ['new/skip.rst']\n"),
                ("new/a.rst", "x"),
                ("new/skip.rst", "x"),
                ("old.rst", "x"),
            ],
        );
        let model = discover_projects(&root).swap_remove(0).model;

        // When
        let sources = sources_under(&model, &root.join("new"));

        // Then
        assert_eq!(sources, [("new/a.rst".to_string(), root.join("new/a.rst"))]);
    }

    #[test]
    fn test_contains_conf_finds_a_conf_py_at_or_under_a_path() {
        // Given
        let root = temp_tree(
            "contains_conf",
            &[
                ("a/b/conf.py", ""),
                ("c/x.rst", "x"),
                (".hidden/conf.py", ""),
            ],
        );

        // When / Then
        assert!(contains_conf(&root.join("a")));
        assert!(contains_conf(&root.join("a/b/conf.py")));
        assert!(!contains_conf(&root.join("c")));
        assert!(!contains_conf(&root.join(".hidden").join("x")));
    }

    #[test]
    fn test_is_visible_under_refuses_a_hidden_path_and_one_outside() {
        // Given
        let root = Path::new("/work/.config/docs");

        // When / Then — the root's own hidden ancestors do not count
        assert!(is_visible_under(
            root,
            Path::new("/work/.config/docs/guide")
        ));
        assert!(!is_visible_under(
            root,
            Path::new("/work/.config/docs/.git")
        ));
        assert!(!is_visible_under(root, Path::new("/work/other")));
    }
}
