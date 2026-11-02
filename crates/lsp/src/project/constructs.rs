//! What a project's documents wrote that this build does not support, for the
//! report that says so before a project migrates (roadmap #36).
//!
//! Read off a document's parse — its nodes and its whole `diagnostics` list —
//! rather than what an editor shows: a construct a `.. noqa:` silences or a
//! project's strictness softens is still unsupported. One [`DocumentConstructs`]
//! is kept per document, beside its analysis, and [`ConstructTally`] folds them
//! on request.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rinx_ast::{Diagnostic, DiagnosticCode, DiagnosticSubject, Directive, Document, Node};
use rinx_ast::{SourceMap, Span, ToctreeOptions, walk_nodes};
use serde::Serialize;

use super::index::site_path;
use crate::diagnostics::{RST_EPILOG, RST_PROLOG};

/// How many places a [`ConstructCount`] lists: enough to find the construct,
/// not every use of one a corpus writes thousands of times.
const MAX_EXAMPLES: usize = 5;

/// Which list of a [`ConstructTally`] a construct belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConstructKind {
    UnknownDirective,
    MalformedDirective,
    UnsupportedOption,
    UnknownRole,
    UnsupportedRole,
    IgnoredRole,
}

/// One use of a construct, in one document.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConstructUse {
    kind: ConstructKind,
    name: String,
    /// The `.. include::`d fragment it was written in, by its source-root
    /// path, when not the document itself.
    file: Option<String>,
    /// Where it stands, when it came from the source at all.
    place: Option<rinx_ast::Position>,
}

/// What one document wrote that this build does not support.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentConstructs {
    uses: Vec<ConstructUse>,
    /// The `.. toctree::` options it wrote, each once per toctree.
    toctree_options: Vec<&'static str>,
    /// Its diagnostics, by code, suppressed ones included.
    diagnostics: BTreeMap<&'static str, usize>,
}

impl DocumentConstructs {
    /// What `document`, whose parse left `source_map` beside it, wrote.
    #[must_use]
    pub fn of(document: &Document, source_map: &SourceMap) -> Self {
        let mut constructs = Self::default();
        let mut add = |kind, name: &str, span: Option<Span>| {
            constructs.uses.push(ConstructUse {
                kind,
                name: name.to_string(),
                file: span
                    .and_then(|span| span.file)
                    .and_then(|file| document.source_files.get(file.index()).cloned()),
                place: span.map(|span| span.start),
            });
        };
        for diagnostic in &document.diagnostics {
            if let Some((kind, name)) = diagnosed_construct(diagnostic) {
                add(kind, &name, diagnostic.span);
            }
        }
        walk_nodes(&document.nodes, &mut |node| {
            if let Node::Directive {
                directive: Directive::Malformed { name, .. },
                ..
            } = node
            {
                let lines = node.block().and_then(|id| source_map.block_lines(id));
                add(ConstructKind::MalformedDirective, name, lines);
            }
        });
        for role in &source_map.ignored_roles {
            add(ConstructKind::IgnoredRole, &role.name, Some(role.span));
        }
        walk_nodes(&document.nodes, &mut |node| {
            if let Node::Directive {
                directive: Directive::Toctree(toctree),
                ..
            } = node
            {
                constructs
                    .toctree_options
                    .extend(written_options(&toctree.options));
            }
        });
        for diagnostic in &document.diagnostics {
            *constructs
                .diagnostics
                .entry(diagnostic.code.as_str())
                .or_default() += 1;
        }
        constructs
    }
}

/// The construct `diagnostic` reports as unsupported, if it does: which list
/// it counts in, under which name.
fn diagnosed_construct(diagnostic: &Diagnostic) -> Option<(ConstructKind, String)> {
    match (diagnostic.code, diagnostic.subject.as_ref()?) {
        (DiagnosticCode::DirectiveUnknown, DiagnosticSubject::Directive(name)) => {
            Some((ConstructKind::UnknownDirective, name.clone()))
        }
        (DiagnosticCode::InterpretedUnknownRole, DiagnosticSubject::Role(name)) => {
            Some((ConstructKind::UnknownRole, name.clone()))
        }
        (DiagnosticCode::InterpretedUnsupportedRole, DiagnosticSubject::Role(name)) => {
            Some((ConstructKind::UnsupportedRole, name.clone()))
        }
        (
            DiagnosticCode::DirectiveUnsupportedOption,
            DiagnosticSubject::UnsupportedOption { directive, option },
        ) => Some((
            ConstructKind::UnsupportedOption,
            format!("{directive}:{option}"),
        )),
        _ => None,
    }
}

/// The names of the options `options` holds, as an author writes them.
fn written_options(options: &ToctreeOptions) -> Vec<&'static str> {
    let valued = [
        ("maxdepth", options.maxdepth.is_some()),
        ("numbered", options.numbered.is_some()),
        ("caption", options.caption.is_some()),
        ("name", options.name.is_some()),
    ];
    valued
        .into_iter()
        .filter_map(|(name, written)| written.then_some(name))
        .chain(options.flags.iter().map(|flag| flag.option_name()))
        .collect()
}

/// A place a construct was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConstructSite {
    pub path: PathBuf,
    /// 1-based, as a warning names it.
    pub line: u32,
    /// 1-based, as a warning names it.
    pub column: u32,
}

/// How often a construct was written, and the first few places.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ConstructCount {
    pub count: usize,
    pub examples: Vec<ConstructSite>,
}

impl ConstructCount {
    /// Counts one more use, written at `site` if that is known.
    fn add(&mut self, site: Option<ConstructSite>) {
        self.count += 1;
        if self.examples.len() < MAX_EXAMPLES {
            self.examples.extend(site);
        }
    }
}

/// What a project's documents wrote that this build does not support — each
/// construct by its name as written — and the counts a report reads beside
/// them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ConstructTally {
    /// Directives no table, schema or extension of this build answers to.
    pub unknown_directives: BTreeMap<String, ConstructCount>,
    /// Directives this build implements whose content it had to refuse: not a
    /// gap in coverage, but a document's mistake.
    pub malformed_directives: BTreeMap<String, ConstructCount>,
    /// Options the reference implementation takes and this build does not, as
    /// `directive:option`.
    pub unsupported_options: BTreeMap<String, ConstructCount>,
    /// Roles nothing answers to: a typo, or an extension's.
    pub unknown_roles: BTreeMap<String, ConstructCount>,
    /// Roles docutils or Sphinx defines that this build does not implement.
    pub unsupported_roles: BTreeMap<String, ConstructCount>,
    /// Roles the project's parse configuration ignores.
    pub ignored_roles: BTreeMap<String, ConstructCount>,
    /// The `.. toctree::` options in use, by name.
    pub toctree_options: BTreeMap<String, usize>,
    /// The parse's diagnostics, by code, suppressed ones included.
    pub diagnostics: BTreeMap<String, usize>,
}

impl ConstructTally {
    /// Adds what the document at `document`, in the project at `source_root`,
    /// wrote.
    pub(super) fn add(
        &mut self,
        document: &Path,
        source_root: &Path,
        constructs: &DocumentConstructs,
    ) {
        for construct in &constructs.uses {
            let site = construct.place.and_then(|place| {
                in_a_file(construct.file.as_deref()).then(|| ConstructSite {
                    path: site_path(document, source_root, construct.file.as_deref()),
                    line: place.line,
                    column: place.column,
                })
            });
            self.list_mut(construct.kind)
                .entry(construct.name.clone())
                .or_default()
                .add(site);
        }
        for option in &constructs.toctree_options {
            *self
                .toctree_options
                .entry((*option).to_string())
                .or_default() += 1;
        }
        for (code, count) in &constructs.diagnostics {
            *self.diagnostics.entry((*code).to_string()).or_default() += count;
        }
    }

    /// The list counting constructs of `kind`.
    fn list_mut(&mut self, kind: ConstructKind) -> &mut BTreeMap<String, ConstructCount> {
        match kind {
            ConstructKind::UnknownDirective => &mut self.unknown_directives,
            ConstructKind::MalformedDirective => &mut self.malformed_directives,
            ConstructKind::UnsupportedOption => &mut self.unsupported_options,
            ConstructKind::UnknownRole => &mut self.unknown_roles,
            ConstructKind::UnsupportedRole => &mut self.unsupported_roles,
            ConstructKind::IgnoredRole => &mut self.ignored_roles,
        }
    }
}

/// Whether `file` names a file at all, rather than a `conf.py`'s `rst_prolog`
/// or `rst_epilog`, which no file holds.
fn in_a_file(file: Option<&str>) -> bool {
    file.is_none_or(|file| file != RST_PROLOG && file != RST_EPILOG)
}

#[cfg(test)]
mod tests;
