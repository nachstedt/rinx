//! The report of what a project wrote that this build does not support
//! (roadmap #36): the `rinx/analyseProject` request a client draws its panel
//! from.
//!
//! `project::ConstructTally` holds the counts; this says what they mean — which
//! declared extension provides each construct, and where
//! `docs/compatibility.rst` speaks of it — so every client reads one answer.

use lsp_types::{TextDocumentIdentifier, Uri};
use serde::{Deserialize, Serialize};

use crate::progress::ProjectStatus;
use crate::project::{
    CompatibilityLink, CompatibilityLinks, ConstructCount, ConstructTally, ExtensionProfiles,
    Strictness,
};
use crate::uri::file_uri;

/// The request asking for the report.
pub const ANALYSE_PROJECT_METHOD: &str = "rinx/analyseProject";

/// What a `rinx/analyseProject` request asks for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalyseProjectParams {
    /// A document of the project asked about; without one, every project.
    pub text_document: Option<TextDocumentIdentifier>,
}

/// The answer to a `rinx/analyseProject` request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisReport {
    /// Whether the workspace has been scanned; until then a count may grow.
    pub complete: bool,
    pub projects: Vec<ProjectReport>,
}

/// One project's part of the report.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReport {
    /// How `rinx/status` names it.
    pub project: ProjectStatus,
    /// How many documents it holds.
    pub documents: usize,
    /// The extensions its `conf.py` declares.
    pub extensions: Vec<ExtensionReport>,
    /// What its documents wrote, by kind, then most frequent first.
    pub constructs: Vec<ConstructReport>,
}

/// How far this build supports a declared extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionSupport {
    /// Everything it adds works, or it needs nothing of a document.
    Complete,
    /// Some or all of what it adds does not work.
    Incomplete,
    /// This build does not know what it adds.
    Unknown,
}

/// One extension a project declares.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionReport {
    pub name: String,
    pub support: ExtensionSupport,
    /// Where the compatibility page speaks of it, if it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// What kind of thing a [`ConstructReport`] counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConstructKind {
    /// A directive no table, schema or extension of this build answers to.
    UnknownDirective,
    /// An option the reference implementation takes and this build does not,
    /// named `directive:option`.
    UnsupportedOption,
    /// A role docutils or Sphinx defines that this build does not implement.
    UnsupportedRole,
    /// A role nothing answers to: a typo, or an extension's.
    UnknownRole,
    /// A role the project's `rinx_parser.toml` ignores.
    IgnoredRole,
    /// A directive this build implements whose content it had to refuse: not
    /// a gap in coverage, but a document's mistake.
    MalformedDirective,
}

/// One construct a project's documents wrote.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstructReport {
    pub kind: ConstructKind,
    /// Its name as written.
    pub name: String,
    pub count: usize,
    /// The declared extension providing it, when one does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Where the compatibility page speaks of it — none for a malformed
    /// directive, which the page has nothing to say about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// Whether the link is its own row, rather than the section it would be
    /// listed in.
    pub listed: bool,
    /// The first few places it was written.
    pub examples: Vec<ExampleSite>,
}

/// A place a construct was written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExampleSite {
    pub uri: Uri,
    /// 1-based, as a warning names it.
    pub line: u32,
    /// 1-based and counted in characters, as a warning names it.
    pub column: u32,
}

/// The report of a project named `project`, holding `documents` documents,
/// judged by `strictness`, whose documents wrote `tally`.
#[must_use]
pub fn project_report(
    project: ProjectStatus,
    documents: usize,
    strictness: &Strictness,
    tally: &ConstructTally,
) -> ProjectReport {
    let links = CompatibilityLinks::known();
    let profiles = ExtensionProfiles::known();
    let extensions = strictness
        .declared_extensions()
        .into_iter()
        .map(|(name, modelled)| extension_report(name, modelled, links))
        .collect();
    let role = |name: &str| {
        (
            strictness.role_provider(name).map(str::to_string),
            Some(links.role(name, profiles)),
        )
    };
    let mut constructs = Vec::new();
    add_constructs(
        &mut constructs,
        ConstructKind::UnknownDirective,
        &tally.unknown_directives,
        &|name| {
            (
                strictness.directive_provider(name).map(str::to_string),
                Some(links.directive(name, profiles)),
            )
        },
    );
    add_constructs(
        &mut constructs,
        ConstructKind::UnsupportedOption,
        &tally.unsupported_options,
        &|name| {
            let option = name.rsplit(':').next().unwrap_or(name);
            (None, Some(links.option(option)))
        },
    );
    add_constructs(
        &mut constructs,
        ConstructKind::UnsupportedRole,
        &tally.unsupported_roles,
        &role,
    );
    add_constructs(
        &mut constructs,
        ConstructKind::UnknownRole,
        &tally.unknown_roles,
        &role,
    );
    add_constructs(
        &mut constructs,
        ConstructKind::IgnoredRole,
        &tally.ignored_roles,
        &role,
    );
    add_constructs(
        &mut constructs,
        ConstructKind::MalformedDirective,
        &tally.malformed_directives,
        &|_| (None, None),
    );
    ProjectReport {
        project,
        documents,
        extensions,
        constructs,
    }
}

/// What the report says of the declared extension `name`, which the profile
/// table describes if `modelled`.
fn extension_report(name: &str, modelled: bool, links: &CompatibilityLinks) -> ExtensionReport {
    let row = links.extension(name);
    ExtensionReport {
        name: name.to_string(),
        support: match (modelled, row) {
            (false, _) => ExtensionSupport::Unknown,
            (true, Some(_)) => ExtensionSupport::Incomplete,
            (true, None) => ExtensionSupport::Complete,
        },
        link: row.as_ref().map(CompatibilityLink::url),
    }
}

/// What the report says of a construct besides its count: the declared
/// extension providing it and where the compatibility page speaks of it.
type ConstructFacts<'a> = (Option<String>, Option<CompatibilityLink<'a>>);

/// Adds to `reports` each construct of `list`, most frequent first, naming
/// each one's provider and compatibility link by `describe`.
fn add_constructs<'a>(
    reports: &mut Vec<ConstructReport>,
    kind: ConstructKind,
    list: &std::collections::BTreeMap<String, ConstructCount>,
    describe: &dyn Fn(&str) -> ConstructFacts<'a>,
) {
    let mut added: Vec<ConstructReport> = list
        .iter()
        .map(|(name, count)| {
            let (provider, link) = describe(name);
            ConstructReport {
                kind,
                name: name.clone(),
                count: count.count,
                provider,
                link: link.as_ref().map(CompatibilityLink::url),
                listed: link.is_some_and(|link| link.listed),
                examples: count
                    .examples
                    .iter()
                    .filter_map(|site| {
                        Some(ExampleSite {
                            uri: file_uri(&site.path)?,
                            line: site.line,
                            column: site.column,
                        })
                    })
                    .collect(),
            }
        })
        .collect();
    added.sort_by_key(|report| std::cmp::Reverse(report.count));
    reports.append(&mut added);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ConstructSite;

    fn count(count: usize) -> ConstructCount {
        ConstructCount {
            count,
            examples: vec![ConstructSite {
                path: "/docs/index.rst".into(),
                line: 3,
                column: 1,
            }],
        }
    }

    fn sphinx() -> ProjectStatus {
        ProjectStatus::Sphinx {
            conf: "conf.py".to_string(),
            executed_with: None,
        }
    }

    fn declaring(extensions: &[&str]) -> Strictness {
        let extensions: Vec<String> = extensions.iter().map(ToString::to_string).collect();
        Strictness::legacy(&extensions)
    }

    #[test]
    fn test_a_declared_extension_is_complete_incomplete_or_unknown() {
        // Given
        let strictness = declaring(&["sphinx.ext.napoleon", "sphinx.ext.autodoc", "my_ext"]);

        // When
        let report = project_report(sphinx(), 1, &strictness, &ConstructTally::default());

        // Then
        let support: Vec<(&str, ExtensionSupport, bool)> = report
            .extensions
            .iter()
            .map(|extension| {
                (
                    extension.name.as_str(),
                    extension.support,
                    extension.link.is_some(),
                )
            })
            .collect();
        assert_eq!(
            support,
            [
                ("sphinx.ext.napoleon", ExtensionSupport::Complete, false),
                ("sphinx.ext.autodoc", ExtensionSupport::Incomplete, true),
                ("my_ext", ExtensionSupport::Unknown, false),
            ]
        );
    }

    #[test]
    fn test_a_directive_names_its_extension_and_its_row() {
        // Given
        let mut tally = ConstructTally::default();
        tally
            .unknown_directives
            .insert("automodule".to_string(), count(2));

        // When
        let report = project_report(sphinx(), 1, &declaring(&["sphinx.ext.autodoc"]), &tally);

        // Then
        assert_eq!(
            report.constructs,
            [ConstructReport {
                kind: ConstructKind::UnknownDirective,
                name: "automodule".to_string(),
                count: 2,
                provider: Some("sphinx.ext.autodoc".to_string()),
                link: Some(
                    "https://nachstedt.github.io/rinx/latest/compatibility.html#compatibility-autodoc"
                        .to_string()
                ),
                listed: true,
                examples: vec![ExampleSite {
                    uri: "file:///docs/index.rst".parse().expect("a URI"),
                    line: 3,
                    column: 1,
                }],
            }]
        );
    }

    #[test]
    fn test_a_directive_of_no_declared_extension_is_still_linked_to_its_extensions_row() {
        // Given
        let mut tally = ConstructTally::default();
        tally
            .unknown_directives
            .insert("automodule".to_string(), count(1));

        // When — a rinx project declares no extension
        let report = project_report(
            ProjectStatus::Bazel {
                site: "//docs:site".to_string(),
            },
            1,
            &Strictness::Rinx,
            &tally,
        );

        // Then
        assert_eq!(report.extensions, []);
        assert_eq!(report.constructs[0].provider, None);
        assert!(report.constructs[0].listed);
    }

    #[test]
    fn test_a_construct_no_row_names_links_to_its_section_as_not_listed() {
        // Given
        let mut tally = ConstructTally::default();
        tally
            .unknown_directives
            .insert("availability".to_string(), count(1));

        // When
        let report = project_report(sphinx(), 1, &declaring(&[]), &tally);

        // Then
        let construct = &report.constructs[0];
        assert!(!construct.listed);
        assert!(
            construct
                .link
                .as_deref()
                .is_some_and(|link| link.ends_with("#compatibility-directives")),
            "{construct:?}"
        );
    }

    #[test]
    fn test_constructs_are_listed_by_kind_then_most_frequent_first() {
        // Given
        let mut tally = ConstructTally::default();
        tally.unknown_roles.insert("gh".to_string(), count(1));
        tally.ignored_roles.insert("issue".to_string(), count(9));
        tally.unknown_directives.insert("a".to_string(), count(1));
        tally.unknown_directives.insert("b".to_string(), count(5));

        // When
        let report = project_report(sphinx(), 1, &declaring(&[]), &tally);

        // Then
        let names: Vec<&str> = report
            .constructs
            .iter()
            .map(|construct| construct.name.as_str())
            .collect();
        assert_eq!(names, ["b", "a", "gh", "issue"]);
    }

    #[test]
    fn test_an_option_is_linked_by_its_own_name_and_a_malformed_directive_not_at_all() {
        // Given
        let mut tally = ConstructTally::default();
        tally
            .unsupported_options
            .insert("py:method:no-typesetting".to_string(), count(1));
        tally
            .malformed_directives
            .insert("figure".to_string(), count(1));

        // When
        let report = project_report(sphinx(), 1, &declaring(&[]), &tally);

        // Then
        assert!(report.constructs[0].listed);
        assert_eq!(report.constructs[1].kind, ConstructKind::MalformedDirective);
        assert_eq!(report.constructs[1].link, None);
    }

    #[test]
    fn test_a_role_extlinks_defines_names_extlinks_as_its_provider() {
        // Given
        let mut tally = ConstructTally::default();
        tally.unknown_roles.insert("pypi".to_string(), count(1));
        let strictness = declaring(&["sphinx.ext.extlinks"])
            .with_extlinks(std::collections::BTreeSet::from(["pypi".to_string()]));

        // When
        let report = project_report(sphinx(), 1, &strictness, &tally);

        // Then
        assert_eq!(
            report.constructs[0].provider.as_deref(),
            Some("sphinx.ext.extlinks")
        );
    }

    #[test]
    fn test_the_params_may_be_empty() {
        // Given / When
        let params: AnalyseProjectParams =
            serde_json::from_value(serde_json::json!({})).expect("empty params");

        // Then
        assert_eq!(params.text_document, None);
    }
}
