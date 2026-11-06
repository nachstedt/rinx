//! `rinx init --from-sphinx`: a migration from the command line, of a
//! project nobody has open.
//!
//! It is the server's own discovery and scan, run once and in-process, so
//! the plan is the one the editor's source action makes of the same
//! project. Only the `conf.py` run differs: it is granted by `--python`
//! rather than asked of the editor.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::finding::MigrationFinding;
use super::plan::{MigrationOptions, MigrationPlan, plan_migration};
use crate::project::{
    CONF_RUN_TIMEOUT, ConfEvaluation, ConfEvaluations, Project, ProjectSource,
    discover_projects_with, run_conf, scan_projects,
};

/// The file that makes a directory a Sphinx project.
const CONF_FILE: &str = "conf.py";

/// What `rinx init --from-sphinx` is told.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InitOptions {
    /// The interpreter `conf.py` runs in, where reading it falls short.
    pub python: Option<PathBuf>,
    pub migration: MigrationOptions,
}

/// What migrating the Sphinx project whose `conf.py` is in `directory`
/// writes.
///
/// # Errors
///
/// When `directory` holds no `conf.py`.
pub fn init_from_sphinx(directory: &Path, options: &InitOptions) -> Result<MigrationPlan> {
    let conf = directory.join(CONF_FILE);
    if !conf.is_file() {
        bail!("{} holds no {CONF_FILE}", directory.display());
    }
    let mut run_failure = None;
    let mut evaluations = ConfEvaluations::new();
    if let Some(python) = &options.python {
        let conf_text = std::fs::read_to_string(&conf)?;
        let outcome = run_conf(python, &conf, CONF_RUN_TIMEOUT);
        if let Err(failure) = &outcome {
            run_failure = Some(MigrationFinding::ConfRunFailed {
                message: failure.message.clone(),
            });
        }
        evaluations.insert(
            conf.clone(),
            ConfEvaluation {
                conf_text,
                interpreter: python.clone(),
                outcome,
            },
        );
    }
    let Some(mut discovered) = discover_projects_with(directory, &evaluations, Vec::new())
        .into_iter()
        .find(|project| {
            matches!(&project.model.source, ProjectSource::SphinxConf { conf: found } if *found == conf)
        })
    else {
        bail!("{} is no Sphinx project", directory.display());
    };
    if options.migration.jinja && discovered.model.settings.parse.jinja.is_none() {
        // Rendered as the build will render it, so the templates an
        // `{% include %}` reads are among the documents' reads.
        discovered.model.settings.parse.jinja = Some(Vec::new());
    }
    let Some(scanned) = scan_projects(vec![discovered], &|_, _| {}).pop() else {
        bail!("{} could not be scanned", directory.display());
    };
    let mut project = Project::new(0, scanned.discovered);
    project.record_scanned(scanned.documents);
    let Some(mut plan) = plan_migration(&mut project, &options.migration) else {
        bail!("{} is no Sphinx project", directory.display());
    };
    plan.findings.splice(0..0, run_failure);
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::plan::{PlannedContent, PlannedFile};
    use std::fs;

    /// A fresh directory holding `files`, each a relative path and its text.
    fn temp_tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rinx_lsp_init_{name}"));
        let _ = fs::remove_dir_all(&root);
        for (file, text) in files {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
        }
        root
    }

    /// The text the plan creates at `file`, relative to `root`.
    fn created<'a>(plan: &'a MigrationPlan, root: &Path, file: &str) -> &'a str {
        match plan
            .files
            .iter()
            .find(|planned| planned.path == root.join(file))
        {
            Some(PlannedFile {
                content: PlannedContent::Create(text),
                ..
            }) => text,
            other => panic!("{file} is not created: {other:?}"),
        }
    }

    fn written(plan: &MigrationPlan, root: &Path) -> Vec<String> {
        let mut names: Vec<String> = plan
            .files
            .iter()
            .map(|file| {
                file.path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    const INDEX: &str = "Home\n====\n\n.. toctree::\n\n   guide\n\n\
        .. include:: includes/snippet.rst\n\n\
        .. image:: _static/logo.png\n\n\
        :download:`data <data/a.csv>`\n";

    #[test]
    fn test_init_from_sphinx_declares_what_the_documents_read_show_and_link() {
        // Given
        let root = temp_tree(
            "plain",
            &[
                (
                    "conf.py",
                    "project = 'Demo'\nrelease = '1.0'\n\
                     exclude_patterns = ['drafts', 'includes']\n\
                     rst_prolog = '.. |x| replace:: y'\n",
                ),
                ("index.rst", INDEX),
                ("guide.rst", "Guide\n=====\n"),
                ("includes/snippet.rst", "Snippet.\n"),
                ("drafts/old.rst", "Old\n===\n"),
                ("_static/logo.png", "png"),
                ("data/a.csv", "a,b\n"),
            ],
        );

        // When
        let plan = init_from_sphinx(&root, &InitOptions::default()).expect("a plan");

        // Then
        assert_eq!(
            written(&plan, &root),
            ["BUILD.bazel", "MODULE.bazel", "rinx.toml", "rst_prolog.txt"]
        );
        assert!(plan.findings.is_empty(), "{:?}", plan.findings);
        assert!(plan.conflicts().is_empty());
        let build = created(&plan, &root, "BUILD.bazel");
        for line in [
            "            \"bazel-*/**\",\n            \"drafts/**\",\n            \"includes/**\",\n",
            "    parse_data = [\"includes/snippet.rst\"],\n",
            "    images = [\"_static/logo.png\"],\n",
            "    downloads = [\"data/a.csv\"],\n",
            "    rst_prolog = \"rst_prolog.txt\",\n",
        ] {
            assert!(build.contains(line), "{line:?} missing from:\n{build}");
        }
        assert!(!build.contains("diagrams"), "{build}");
        assert_eq!(
            created(&plan, &root, "rst_prolog.txt"),
            ".. |x| replace:: y\n"
        );
        let site = created(&plan, &root, "rinx.toml");
        assert!(
            site.contains("project = \"Demo\"\nversion = \"1.0\"\n"),
            "{site}"
        );
        assert!(created(&plan, &root, "MODULE.bazel").contains("module(name = \"demo_docs\")"));
    }

    #[test]
    fn test_init_from_sphinx_extends_an_enclosing_module_and_names_documents_from_its_root() {
        // Given — CPython's shape: the workspace above, conf.py in `Doc/`
        let workspace = temp_tree(
            "enclosing",
            &[
                ("MODULE.bazel", "module(name = \"bench\")\n"),
                ("Doc/conf.py", "root_doc = 'contents'\n"),
                ("Doc/contents.rst", "Contents\n========\n"),
            ],
        );
        let root = workspace.join("Doc");

        // When
        let plan = init_from_sphinx(&root, &InitOptions::default()).expect("a plan");

        // Then
        let module = plan
            .files
            .iter()
            .find(|file| file.path == workspace.join("MODULE.bazel"))
            .expect("the module is extended");
        let PlannedContent::Append(appended) = &module.content else {
            panic!("appended, not {module:?}");
        };
        assert!(appended.starts_with("\n# Added by"), "{appended}");
        assert!(created(&plan, &root, "rinx.toml").contains("root_doc = \"Doc/contents\"\n"));
        assert!(!created(&plan, &root, "BUILD.bazel").contains("bazel-*"));
    }

    #[test]
    fn test_init_from_sphinx_leaves_a_module_depending_on_rinx_alone() {
        // Given
        let root = temp_tree(
            "depends",
            &[
                (
                    "MODULE.bazel",
                    "bazel_dep(name = \"rinx\", version = \"0.1.0\")\n",
                ),
                ("conf.py", ""),
                ("index.rst", "Home\n====\n"),
            ],
        );

        // When
        let plan = init_from_sphinx(&root, &InitOptions::default()).expect("a plan");

        // Then
        assert_eq!(written(&plan, &root), ["BUILD.bazel", "rinx.toml"]);
    }

    #[test]
    fn test_init_from_sphinx_writes_the_entity_schema_of_a_needs_project() {
        // Given
        let root = temp_tree(
            "needs",
            &[
                (
                    "conf.py",
                    "extensions = ['sphinx_needs']\n\
                     needs_types = [{'directive': 'req', 'title': 'Requirement', \
                     'prefix': 'R_', 'color': '#BFD8D2', 'style': 'node'}]\n",
                ),
                ("index.rst", "Home\n====\n\n.. req:: Login\n   :id: R_1\n"),
            ],
        );

        // When
        let plan = init_from_sphinx(&root, &InitOptions::default()).expect("a plan");

        // Then
        let schema = created(&plan, &root, "entities.toml");
        assert!(schema.contains("req"), "{schema}");
        assert_eq!(
            created(&plan, &root, "BUILD.bazel")
                .matches("entity_schema = \"entities.toml\"")
                .count(),
            2
        );
    }

    #[test]
    fn test_init_from_sphinx_reports_what_it_cannot_carry_over() {
        // Given
        let outside = temp_tree("outside_file", &[("shared.txt", "shared\n")]);
        let root = temp_tree(
            "findings",
            &[
                (
                    "conf.py",
                    "extensions = ['sphinx.ext.intersphinx']\n\
                     intersphinx_mapping = {'python': ('https://docs.python.org/3', None)}\n\
                     version = get_version()\n",
                ),
                (
                    "index.rst",
                    "Home\n====\n\n.. literalinclude:: ../rinx_lsp_init_outside_file/shared.txt\n\n\
                     .. image:: missing.png\n",
                ),
            ],
        );

        // When
        let plan = init_from_sphinx(&root, &InitOptions::default()).expect("a plan");

        // Then
        assert_eq!(
            plan.findings,
            [
                MigrationFinding::UnreadSetting {
                    setting: "version".to_string()
                },
                MigrationFinding::Inventory {
                    name: "python".to_string()
                },
                MigrationFinding::OutsideSourceRoot {
                    path: outside.join("shared.txt"),
                    build_path: PathBuf::from("rinx_lsp_init_outside_file/shared.txt"),
                },
                MigrationFinding::MissingFile {
                    path: root.join("missing.png")
                },
            ]
        );
    }

    #[test]
    fn test_init_from_sphinx_renders_templates_when_told_to() {
        // Given — a `source-read` hook, which reading conf.py cannot see
        let root = temp_tree(
            "jinja",
            &[
                ("conf.py", ""),
                ("index.rst", "Home\n====\n\n{% include \"snippet.txt\" %}\n"),
                ("snippet.txt", "Included.\n"),
            ],
        );
        let options = InitOptions {
            migration: MigrationOptions {
                jinja: true,
                ..MigrationOptions::default()
            },
            ..InitOptions::default()
        };

        // When
        let plan = init_from_sphinx(&root, &options).expect("a plan");

        // Then
        let build = created(&plan, &root, "BUILD.bazel");
        assert!(build.contains("    jinja = True,\n"), "{build}");
        assert!(
            build.contains("    parse_data = [\"snippet.txt\"],\n"),
            "{build}"
        );
    }

    #[test]
    fn test_init_from_sphinx_names_the_files_it_would_overwrite() {
        // Given
        let root = temp_tree(
            "conflict",
            &[
                ("conf.py", ""),
                ("index.rst", "Home\n====\n"),
                ("BUILD.bazel", ""),
            ],
        );

        // When
        let plan = init_from_sphinx(&root, &InitOptions::default()).expect("a plan");

        // Then
        assert_eq!(plan.conflicts(), [root.join("BUILD.bazel").as_path()]);
    }

    #[test]
    fn test_init_from_sphinx_refuses_a_directory_without_conf_py() {
        // Given
        let root = temp_tree("no_conf", &[("index.rst", "Home\n====\n")]);

        // When
        let error = init_from_sphinx(&root, &InitOptions::default()).unwrap_err();

        // Then
        assert!(error.to_string().contains("holds no conf.py"), "{error}");
    }
}
