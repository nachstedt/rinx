//! A Sphinx project's settings, read from its `conf.py` without running it
//! (ADR-038 §4).
//!
//! [`rinx_pyconf`] says what the module binds; this says what those bindings
//! mean to Sphinx, setting by setting, and reports — as a diagnostic in
//! `conf.py` itself — every setting it could not take as written:
//!
//! - a value that is no literal keeps the setting's default
//!   (`conf.unread-setting`);
//! - a literal changed afterwards keeps the literal, and the change is
//!   reported where it is made (`conf.modified-setting`) — Python's own
//!   documentation appends to its `exclude_patterns` inside an `if`, and
//!   dropping the whole literal for that would index every file it excludes;
//! - a value of the wrong kind, or one rinx does not support, keeps the
//!   default or loses the entry (`conf.invalid-value`).
//!
//! Only the settings the server uses are read, so only they are reported: a
//! computed `html_theme` changes nothing the editor shows.

use rinx_ast::{Diagnostic, DiagnosticCode, Domain, Position, ResolvedLanguage, Span};
use rinx_parser::{DefaultRole, ParseCtx, RejectParseFiles};
use rinx_pyconf::{BoundValue, ModuleReading, Value, ValueKind, read_module};

use super::exclusion::{Exclusion, INCLUDE_EVERYTHING};
use super::model::{ParseSettings, ProjectSettings};
use super::strictness::Strictness;

/// What reading one `conf.py` found: the settings, and every way the reading
/// fell short of running it.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfReading {
    pub settings: ProjectSettings,
    /// In source order, each positioned in `conf.py`.
    pub findings: Vec<Diagnostic>,
}

/// The settings `text`, a `conf.py`, configures.
#[must_use]
pub fn read_sphinx_conf(text: &str) -> ConfReading {
    let module = read_module(text);
    let mut reader = Reader {
        module: &module,
        findings: Vec::new(),
    };
    let settings = reader.settings();
    let mut findings = reader.findings;
    findings.extend(module.wildcard_imports.iter().map(|span| {
        Diagnostic::new(
            DiagnosticCode::ConfWildcardImport,
            "`from … import *` may set settings rinx cannot see",
            to_span(*span),
        )
    }));
    if let Some(error) = &module.error {
        findings.push(Diagnostic::new(
            DiagnosticCode::ConfSyntaxError,
            format!(
                "rinx stopped reading `conf.py` here ({}); every setting after it keeps its default",
                error.kind
            ),
            to_span(error.span),
        ));
    }
    findings.sort_by_key(|finding| finding.span.map(|span| span.start));
    ConfReading { settings, findings }
}

struct Reader<'a> {
    module: &'a ModuleReading,
    findings: Vec<Diagnostic>,
}

impl<'a> Reader<'a> {
    fn settings(&mut self) -> ProjectSettings {
        let defaults = ProjectSettings::default();
        let root_doc = self
            .string("root_doc")
            .or_else(|| self.string("master_doc"))
            .unwrap_or(defaults.root_doc);
        let domain = self.primary_domain();
        let default_role = self.default_role(domain);
        ProjectSettings {
            root_doc,
            source_suffixes: self.source_suffixes(),
            exclusion: self.exclusion(),
            parse: ParseSettings {
                domain,
                default_role,
            },
            numfig: self.boolean("numfig").unwrap_or(defaults.numfig),
            numfig_secnum_depth: self
                .count("numfig_secnum_depth")
                .unwrap_or(defaults.numfig_secnum_depth),
            highlight_language: self
                .highlight_language()
                .unwrap_or(defaults.highlight_language),
            strictness: self.strictness(),
        }
    }

    /// The literal `name` is bound to, reporting each change made to it
    /// afterwards — or `None`, reported, when it is bound to something else,
    /// and `None` silently when it is not bound at all.
    fn literal(&mut self, name: &str) -> Option<&'a Value> {
        let binding = self.module.binding(name)?;
        match &binding.value {
            BoundValue::Literal(value) => {
                for modification in &binding.modifications {
                    self.findings.push(Diagnostic::new(
                        DiagnosticCode::ConfModifiedSetting,
                        format!(
                            "`{name}` is changed here by code rinx does not run, so the change is not applied"
                        ),
                        to_span(*modification),
                    ));
                }
                Some(value)
            }
            BoundValue::Unread(span) => {
                self.findings.push(Diagnostic::new(
                    DiagnosticCode::ConfUnreadSetting,
                    format!("`{name}` is not a literal, so rinx uses its default"),
                    to_span(*span),
                ));
                None
            }
        }
    }

    fn invalid(&mut self, value: &Value, message: String) {
        self.findings.push(Diagnostic::new(
            DiagnosticCode::ConfInvalidValue,
            message,
            to_span(value.span),
        ));
    }

    fn string(&mut self, name: &str) -> Option<String> {
        let value = self.literal(name)?;
        if let Some(text) = value.as_str() {
            return Some(text.to_string());
        }
        self.invalid(value, format!("`{name}` must be a string"));
        None
    }

    fn boolean(&mut self, name: &str) -> Option<bool> {
        let value = self.literal(name)?;
        if let ValueKind::Bool(flag) = value.kind {
            return Some(flag);
        }
        self.invalid(value, format!("`{name}` must be `True` or `False`"));
        None
    }

    fn count(&mut self, name: &str) -> Option<usize> {
        let value = self.literal(name)?;
        if let ValueKind::Int(number) = value.kind
            && let Ok(count) = usize::try_from(number)
        {
            return Some(count);
        }
        self.invalid(value, format!("`{name}` must be a whole number, 0 or more"));
        None
    }

    /// The strictness the declared `extensions` imply, against the table
    /// this server was built with.
    fn strictness(&mut self) -> Strictness {
        Strictness::legacy(&self.strings("extensions").unwrap_or_default())
    }

    /// The strings of the list or tuple `name` is bound to, each other
    /// element reported and left out.
    fn strings(&mut self, name: &str) -> Option<Vec<String>> {
        let value = self.literal(name)?;
        self.strings_in(value, name)
    }

    /// The strings of `value`, the list or tuple `name` is bound to.
    fn strings_in(&mut self, value: &Value, name: &str) -> Option<Vec<String>> {
        let Some(items) = value.as_sequence() else {
            self.invalid(value, format!("`{name}` must be a list of strings"));
            return None;
        };
        let mut strings = Vec::new();
        for item in items {
            match item.as_str() {
                Some(text) => strings.push(text.to_string()),
                None => self.invalid(
                    item,
                    format!("`{name}` must hold strings only; this entry is left out"),
                ),
            }
        }
        Some(strings)
    }

    /// `source_suffix`: a string, a list of them, or a dict from suffix to
    /// the parser reading it, as Sphinx's `convert_source_suffix` takes it.
    /// Only reStructuredText's suffixes are kept.
    fn source_suffixes(&mut self) -> Vec<String> {
        let defaults = ProjectSettings::default().source_suffixes;
        let Some(value) = self.literal("source_suffix") else {
            return defaults;
        };
        match &value.kind {
            ValueKind::Str(suffix) => vec![suffix.clone()],
            ValueKind::List(_) | ValueKind::Tuple(_) => {
                self.strings_in(value, "source_suffix").unwrap_or(defaults)
            }
            ValueKind::Dict(entries) => {
                let mut suffixes = Vec::new();
                for (suffix, parser) in entries {
                    let Some(suffix_text) = suffix.as_str() else {
                        self.invalid(suffix, "a `source_suffix` key must be a string".to_string());
                        continue;
                    };
                    match &parser.kind {
                        ValueKind::Str(name) if name == "restructuredtext" => {
                            suffixes.push(suffix_text.to_string());
                        }
                        ValueKind::None => suffixes.push(suffix_text.to_string()),
                        ValueKind::Str(name) => self.invalid(
                            suffix,
                            format!(
                                "rinx reads reStructuredText only, so `{suffix_text}` files ({name}) are not documents"
                            ),
                        ),
                        _ => self.invalid(
                            parser,
                            "a `source_suffix` value must name a parser".to_string(),
                        ),
                    }
                }
                suffixes
            }
            _ => {
                self.invalid(
                    value,
                    "`source_suffix` must be a string, a list of strings or a dict".to_string(),
                );
                defaults
            }
        }
    }

    /// What Sphinx's `Environment.find_files` leaves out: `exclude_patterns`,
    /// plus `templates_path` and the HTML builder's asset paths, all read as
    /// patterns — and lets in: `include_patterns`.
    fn exclusion(&mut self) -> Exclusion {
        let mut excluded = Vec::new();
        for name in [
            "exclude_patterns",
            "templates_path",
            "html_static_path",
            "html_extra_path",
        ] {
            excluded.extend(self.strings(name).unwrap_or_default());
        }
        let included = self
            .strings("include_patterns")
            .unwrap_or_else(|| vec![INCLUDE_EVERYTHING.to_string()]);
        Exclusion::sphinx(&excluded, &included)
    }

    /// `primary_domain`, one of the two domains rinx can make the default.
    fn primary_domain(&mut self) -> Domain {
        let default = ParseSettings::default().domain;
        let Some(value) = self.literal("primary_domain") else {
            return default;
        };
        match &value.kind {
            ValueKind::Str(name) if name == "py" || name == "c" => name.parse().unwrap_or(default),
            ValueKind::Str(name) => {
                self.invalid(
                    value,
                    format!("rinx has no `{name}` domain to make the default, so `py` is used"),
                );
                default
            }
            ValueKind::None => {
                self.invalid(
                    value,
                    "rinx always has a default domain, so `py` is used".to_string(),
                );
                default
            }
            _ => {
                self.invalid(value, "`primary_domain` must be a string".to_string());
                default
            }
        }
    }

    /// `default_role`, which must name a role rinx knows under `domain`.
    fn default_role(&mut self, domain: Domain) -> DefaultRole {
        let default = ParseSettings::default().default_role;
        let Some(value) = self.literal("default_role") else {
            return default;
        };
        match &value.kind {
            ValueKind::None => default,
            ValueKind::Str(name) => {
                let ctx = ParseCtx::new(domain, &RejectParseFiles);
                DefaultRole::parse(name, &ctx).unwrap_or_else(|_| {
                    self.invalid(
                        value,
                        format!("`default_role` names no role rinx knows: `{name}`"),
                    );
                    default
                })
            }
            _ => {
                self.invalid(
                    value,
                    "`default_role` must be a string or `None`".to_string(),
                );
                default
            }
        }
    }

    fn highlight_language(&mut self) -> Option<ResolvedLanguage> {
        let value = self.literal("highlight_language")?;
        let Some(name) = value.as_str() else {
            self.invalid(value, "`highlight_language` must be a string".to_string());
            return None;
        };
        match ResolvedLanguage::parse(name) {
            Ok(language) => Some(language),
            Err(error) => {
                self.invalid(
                    value,
                    format!("`highlight_language` is not a language: {error}"),
                );
                None
            }
        }
    }
}

/// `span` as a diagnostic's span: the same lines and character columns.
fn to_span(span: rinx_pyconf::Span) -> Span {
    let point = |position: rinx_pyconf::Position| {
        Position::new(
            u32::try_from(position.line).unwrap_or(u32::MAX),
            u32::try_from(position.column).unwrap_or(u32::MAX),
        )
    };
    Span::new(point(span.start), point(span.end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::Domain;

    /// The codes `text` is reported with, each with its line.
    fn findings(text: &str) -> Vec<(&'static str, u32)> {
        read_sphinx_conf(text)
            .findings
            .iter()
            .map(|finding| {
                (
                    finding.code.as_str(),
                    finding.span.map_or(0, |span| span.start.line),
                )
            })
            .collect()
    }

    fn settings(text: &str) -> ProjectSettings {
        read_sphinx_conf(text).settings
    }

    fn considers(settings: &ProjectSettings, file: &str) -> bool {
        settings.exclusion.considers_file(file)
    }

    #[test]
    fn test_read_sphinx_conf_defaults_every_setting_sphinx_defaults() {
        // When
        let settings = settings("project = 'P'\n");

        // Then
        assert_eq!(settings.root_doc, "index");
        assert_eq!(settings.source_suffixes, [".rst"]);
        assert_eq!(settings.parse, ParseSettings::default());
        assert!(!settings.numfig);
        assert_eq!(settings.highlight_language, ResolvedLanguage::default());
        assert!(considers(&settings, "guide.rst"));
        assert!(settings.exclusion.excludes_directory("a/_sources"));
    }

    #[test]
    fn test_read_sphinx_conf_reads_the_root_document_under_either_name() {
        // When / Then
        assert_eq!(settings("root_doc = 'contents'\n").root_doc, "contents");
        assert_eq!(settings("master_doc = 'contents'\n").root_doc, "contents");
        assert_eq!(
            settings("master_doc = 'old'\nroot_doc = 'new'\n").root_doc,
            "new"
        );
    }

    #[test]
    fn test_read_sphinx_conf_reads_every_spelling_of_source_suffix() {
        // When / Then
        assert_eq!(
            settings("source_suffix = '.txt'\n").source_suffixes,
            [".txt"]
        );
        assert_eq!(
            settings("source_suffix = ['.rst', '.txt']\n").source_suffixes,
            [".rst", ".txt"]
        );
        assert_eq!(
            settings("source_suffix = {'.rst': 'restructuredtext', '.txt': None}\n")
                .source_suffixes,
            [".rst", ".txt"]
        );
    }

    #[test]
    fn test_read_sphinx_conf_leaves_out_and_reports_a_markdown_suffix() {
        // Given
        let text =
            "source_suffix = {\n    '.rst': 'restructuredtext',\n    '.md': 'markdown',\n}\n";

        // When
        let reading = read_sphinx_conf(text);

        // Then
        assert_eq!(reading.settings.source_suffixes, [".rst"]);
        assert_eq!(findings(text), [("conf.invalid-value", 3)]);
        assert!(
            reading.findings[0]
                .message
                .contains("`.md` files (markdown)")
        );
    }

    #[test]
    fn test_read_sphinx_conf_reports_a_source_suffix_of_the_wrong_kind() {
        // When / Then
        assert_eq!(findings("source_suffix = 3\n"), [("conf.invalid-value", 1)]);
        assert_eq!(settings("source_suffix = 3\n").source_suffixes, [".rst"]);
        assert_eq!(
            findings("source_suffix = {1: 'restructuredtext', '.x': 2}\n"),
            [("conf.invalid-value", 1), ("conf.invalid-value", 1)]
        );
    }

    #[test]
    fn test_read_sphinx_conf_excludes_patterns_and_the_implicit_paths() {
        // Given
        let settings = settings(
            "exclude_patterns = ['_build', 'drafts/*']\ntemplates_path = ['_templates']\nhtml_static_path = ['_static']\nhtml_extra_path = ['extra']\n",
        );

        // When / Then
        assert!(settings.exclusion.excludes_directory("_build"));
        assert!(!considers(&settings, "drafts/a.rst"));
        assert!(settings.exclusion.excludes_directory("_templates"));
        assert!(settings.exclusion.excludes_directory("_static"));
        assert!(settings.exclusion.excludes_directory("extra"));
        assert!(considers(&settings, "guide.rst"));
    }

    #[test]
    fn test_read_sphinx_conf_includes_only_the_include_patterns() {
        // When
        let settings = settings("include_patterns = ['guide/**']\n");

        // Then
        assert!(considers(&settings, "guide/a.rst"));
        assert!(!considers(&settings, "other.rst"));
    }

    #[test]
    fn test_read_sphinx_conf_keeps_a_modified_literal_and_reports_the_change() {
        // Given — the shape of CPython's `Doc/conf.py`
        let text = "exclude_patterns = ['includes/*.rst']\nvenvdir = os.getenv('VENVDIR')\nif venvdir is not None:\n    exclude_patterns.append(venvdir + '/*')\n";

        // When
        let reading = read_sphinx_conf(text);

        // Then
        assert!(!considers(&reading.settings, "includes/a.rst"));
        assert_eq!(findings(text), [("conf.modified-setting", 4)]);
        assert!(reading.findings[0].message.contains("`exclude_patterns`"));
    }

    #[test]
    fn test_read_sphinx_conf_reports_a_computed_setting_and_keeps_its_default() {
        // Given
        let text = "import os\nroot_doc = os.getenv('ROOT')\n";

        // When
        let reading = read_sphinx_conf(text);

        // Then
        assert_eq!(reading.settings.root_doc, "index");
        assert_eq!(findings(text), [("conf.unread-setting", 2)]);
    }

    #[test]
    fn test_read_sphinx_conf_does_not_report_settings_it_does_not_read() {
        // When / Then
        assert!(
            findings("import os\nhtml_theme = os.getenv('T')\nnitpick_ignore += ['x']\n")
                .is_empty()
        );
    }

    #[test]
    fn test_read_sphinx_conf_reads_the_declared_extensions() {
        // Given
        let text = "extensions = ['sphinx.ext.autodoc', 'my_ext']\n";

        // When
        let reading = read_sphinx_conf(text);

        // Then — what the table models is not named as unmodelled
        assert_eq!(
            reading.settings.strictness.unmodelled_extensions(),
            ["my_ext"]
        );
    }

    #[test]
    fn test_read_sphinx_conf_declares_no_extensions_by_default() {
        // When
        let reading = read_sphinx_conf("project = 'x'\n");

        // Then
        assert_eq!(reading.settings.strictness, Strictness::default());
    }

    #[test]
    fn test_read_sphinx_conf_reports_values_of_the_wrong_kind() {
        // When / Then
        assert_eq!(findings("root_doc = 1\n"), [("conf.invalid-value", 1)]);
        assert_eq!(findings("numfig = 'yes'\n"), [("conf.invalid-value", 1)]);
        assert_eq!(
            findings("numfig_secnum_depth = -1\n"),
            [("conf.invalid-value", 1)]
        );
        assert_eq!(
            findings("exclude_patterns = 'x'\n"),
            [("conf.invalid-value", 1)]
        );
        assert_eq!(
            findings("exclude_patterns = ['a', 2]\n"),
            [("conf.invalid-value", 1)]
        );
        assert_eq!(
            findings("highlight_language = 3\n"),
            [("conf.invalid-value", 1)]
        );
        assert_eq!(
            findings("highlight_language = ''\n"),
            [("conf.invalid-value", 1)]
        );
        assert_eq!(findings("default_role = 3\n"), [("conf.invalid-value", 1)]);
        assert_eq!(
            findings("primary_domain = 3\n"),
            [("conf.invalid-value", 1)]
        );
    }

    #[test]
    fn test_read_sphinx_conf_reads_the_rendering_settings() {
        // When
        let settings =
            settings("numfig = True\nnumfig_secnum_depth = 2\nhighlight_language = 'python3'\n");

        // Then
        assert!(settings.numfig);
        assert_eq!(settings.numfig_secnum_depth, 2);
        assert_ne!(settings.highlight_language, ResolvedLanguage::default());
    }

    #[test]
    fn test_read_sphinx_conf_reads_the_primary_domain() {
        // When / Then
        assert_eq!(settings("primary_domain = 'c'\n").parse.domain, Domain::C);
        assert_eq!(settings("primary_domain = 'py'\n").parse.domain, Domain::Py);
    }

    #[test]
    fn test_read_sphinx_conf_reports_a_domain_it_cannot_make_the_default() {
        // When / Then
        for text in [
            "primary_domain = 'cpp'\n",
            "primary_domain = None\n",
            "primary_domain = 'std'\n",
        ] {
            assert_eq!(settings(text).parse.domain, Domain::Py, "{text}");
            assert_eq!(findings(text), [("conf.invalid-value", 1)], "{text}");
        }
    }

    #[test]
    fn test_read_sphinx_conf_reads_the_default_role_under_the_primary_domain() {
        // When
        let role = settings("primary_domain = 'c'\ndefault_role = 'func'\n")
            .parse
            .default_role;

        // Then
        assert_ne!(role, DefaultRole::TITLE_REFERENCE);
        assert_eq!(
            settings("default_role = None\n").parse.default_role,
            DefaultRole::TITLE_REFERENCE
        );
        assert_ne!(
            settings("default_role = 'any'\n").parse.default_role,
            DefaultRole::TITLE_REFERENCE
        );
    }

    #[test]
    fn test_read_sphinx_conf_reports_a_default_role_rinx_does_not_know() {
        // Given
        let text = "default_role = 'nonsense'\n";

        // When / Then
        assert_eq!(
            settings(text).parse.default_role,
            DefaultRole::TITLE_REFERENCE
        );
        assert_eq!(findings(text), [("conf.invalid-value", 1)]);
    }

    #[test]
    fn test_read_sphinx_conf_reports_a_wildcard_import_and_a_syntax_error_in_order() {
        // Given
        let text = "from base import *\nroot_doc = 'a'\nbroken = (\n";

        // When
        let reading = read_sphinx_conf(text);

        // Then
        assert_eq!(reading.settings.root_doc, "a");
        assert_eq!(
            findings(text),
            [("conf.wildcard-import", 1), ("conf.syntax-error", 3)]
        );
    }

    #[test]
    fn test_read_sphinx_conf_reads_cpythons_conf_py() {
        // Given
        let text = include_str!("../../../pyconf/testdata/cpython-3.14.2-conf.py");

        // When
        let reading = read_sphinx_conf(text);

        // Then
        assert_eq!(reading.settings.root_doc, "contents");
        assert!(!considers(
            &reading.settings,
            "includes/tzinfo-examples.rst"
        ));
        assert!(!considers(&reading.settings, "README.rst"));
        assert!(
            reading
                .settings
                .exclusion
                .excludes_directory("tools/templates")
        );
        assert!(considers(&reading.settings, "library/os.rst"));
        // The optional extensions appended on line 51; `nitpick_ignore`
        // changes too, but is not read.
        assert_eq!(
            findings(text),
            [
                ("conf.modified-setting", 51),
                ("conf.modified-setting", 119)
            ]
        );
        assert_eq!(
            reading.settings.strictness.unmodelled_extensions(),
            [
                "audit_events",
                "availability",
                "c_annotations",
                "changes",
                "glossary_search",
                "grammar_snippet",
                "implementation_detail",
                "issue_role",
                "lexers",
                "misc_news",
                "pydoc_topics",
                "pyspecific",
            ]
        );
    }
}
