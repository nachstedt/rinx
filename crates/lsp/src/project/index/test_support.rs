//! What the project's tests share: projects at `/work/docs`, and documents
//! to record in them.

use super::*;

pub(super) fn folder() -> Project {
    Project::new(0, DiscoveredProject::folder(PathBuf::from("/work/docs")))
}

/// The analysis of a document named `doc_path` defining the label `label`.
pub(super) fn labelled(doc_path: &str, label: &str) -> IndexedDocument {
    let document = rinx_parser::parse(doc_path, &format!(".. _{label}:\n\nText.\n"));
    IndexedDocument::of(
        Path::new("/work/docs").join(doc_path),
        &document,
        BTreeSet::new(),
    )
}

/// A Sphinx project at `/work/docs` whose `conf.py` says `conf`.
pub(super) fn sphinx(conf: &str) -> Project {
    let reading = crate::project::sphinx_conf::read_sphinx_conf(conf);
    Project::new(
        1,
        DiscoveredProject {
            model: ProjectModel {
                source_root: PathBuf::from("/work/docs"),
                source: ProjectSource::SphinxConf {
                    conf: PathBuf::from("/work/docs/conf.py"),
                },
                settings: reading.settings,
            },
            findings: reading.findings,
            conf_text: conf.to_string(),
            sources: Vec::new(),
            wants_conf_run: reading.falls_short,
        },
    )
}
