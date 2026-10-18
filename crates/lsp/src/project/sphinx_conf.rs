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
//!
//! Where the workspace allows running `conf.py` (see [`super::conf_eval`]),
//! what running it found is authoritative instead: every setting is the value
//! it computed, read through the same checks, and positioned on the line that
//! assigns it — so none of the three findings above is made, while a value of
//! the wrong kind still is.

use rinx_ast::{Diagnostic, DiagnosticCode, Domain, Position, ResolvedLanguage, Span};
use rinx_parser::{DefaultRole, ParseCtx, RejectParseFiles};
use rinx_pyconf::{BoundValue, ModuleReading, Value, ValueKind, read_module};
use std::collections::BTreeMap;

use super::conf_eval::{ConfValues, ExecutedValue, conf_settings};

use super::exclusion::{Exclusion, INCLUDE_EVERYTHING};
use super::intersphinx::{DEFAULT_CACHE_LIMIT_DAYS, IntersphinxSettings, read_mapping};
use super::model::{ParseSettings, ProjectSettings};
use super::needs::{NEEDS_EXTENSIONS, NeedsDeclaration, conf_setting_names, read_declaration};
use super::strictness::Strictness;

/// What reading one `conf.py` found: the settings, and every way the reading
/// fell short of running it.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfReading {
    pub settings: ProjectSettings,
    /// The project's sphinx-needs configuration, when `extensions` declares
    /// sphinx-needs — to be resolved against the files it names.
    pub needs: Option<NeedsDeclaration>,
    /// In source order, each positioned in `conf.py`.
    pub findings: Vec<Diagnostic>,
    /// Whether reading `conf.py` without running it fell short for a setting
    /// the server reads — which running it would make exact.
    pub falls_short: bool,
}

/// The settings `text`, a `conf.py`, configures, read without running it.
#[must_use]
pub fn read_sphinx_conf(text: &str) -> ConfReading {
    read_conf(text, None)
}

/// The settings `text`, a `conf.py`, configures, as running it found them
/// to be: `values`.
#[must_use]
pub fn read_executed_sphinx_conf(text: &str, values: &ConfValues) -> ConfReading {
    read_conf(text, Some(values))
}

fn read_conf(text: &str, executed: Option<&ConfValues>) -> ConfReading {
    let module = read_module(text);
    let executed = executed.map(|values| anchored_values(&module, values));
    let mut reader = Reader {
        module: &module,
        executed: executed.as_ref(),
        findings: Vec::new(),
        asked: Vec::new(),
    };
    let (settings, needs) = reader.settings();
    let mut findings = reader.findings;
    // What a run found does not depend on what reading the text could see.
    if executed.is_none() {
        findings.extend(static_reading_limits(&module));
    }
    findings.sort_by_key(|finding| finding.span.map(|span| span.start));
    ConfReading {
        settings,
        needs,
        findings,
        falls_short: falls_short(&module),
    }
}

/// Where reading `module` without running it had to stop or guess: each
/// wildcard import, and the syntax error it stopped at.
fn static_reading_limits(module: &ModuleReading) -> Vec<Diagnostic> {
    let mut findings: Vec<Diagnostic> = module
        .wildcard_imports
        .iter()
        .map(|span| {
            Diagnostic::new(
                DiagnosticCode::ConfWildcardImport,
                "`from … import *` may set settings rinx cannot see",
                to_span(*span),
            )
        })
        .collect();
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
    findings
}

/// Whether reading `module` without running it falls short for a setting
/// the server reads: one computed or changed afterwards, or a wildcard
/// import or syntax error that may hide any.
fn falls_short(module: &ModuleReading) -> bool {
    !module.wildcard_imports.is_empty()
        || module.error.is_some()
        || conf_settings().iter().any(|name| {
            module.binding(name).is_some_and(|binding| {
                matches!(binding.value, BoundValue::Unread(_)) || !binding.modifications.is_empty()
            })
        })
}

/// `values`, each positioned where `module` assigns it — or at the start of
/// `conf.py` when the assignment is not one the reader sees.
fn anchored_values(module: &ModuleReading, values: &ConfValues) -> BTreeMap<String, ExecutedValue> {
    values
        .values
        .iter()
        .map(|(name, value)| {
            let value = match value {
                ExecutedValue::Literal(value) => {
                    ExecutedValue::Literal(anchored(value, anchor(module, name)))
                }
                ExecutedValue::Unrepresentable(spelling) => {
                    ExecutedValue::Unrepresentable(spelling.clone())
                }
            };
            (name.clone(), value)
        })
        .collect()
}

/// Where `module` assigns `name`, or the start of `conf.py`.
fn anchor(module: &ModuleReading, name: &str) -> rinx_pyconf::Span {
    module.binding(name).map_or(
        rinx_pyconf::Span {
            start: rinx_pyconf::Position::START,
            end: rinx_pyconf::Position::START,
        },
        |binding| binding.target,
    )
}

/// `value`, and everything in it, positioned at `span`.
fn anchored(value: &Value, span: rinx_pyconf::Span) -> Value {
    let all = |items: &[Value]| items.iter().map(|item| anchored(item, span)).collect();
    let kind = match &value.kind {
        ValueKind::List(items) => ValueKind::List(all(items)),
        ValueKind::Tuple(items) => ValueKind::Tuple(all(items)),
        ValueKind::Set(items) => ValueKind::Set(all(items)),
        ValueKind::Dict(entries) => ValueKind::Dict(
            entries
                .iter()
                .map(|(key, item)| (anchored(key, span), anchored(item, span)))
                .collect(),
        ),
        other => other.clone(),
    };
    Value { kind, span }
}

/// The extension whose settings say which other sites a project links into.
const INTERSPHINX: &str = "sphinx.ext.intersphinx";

struct Reader<'a> {
    module: &'a ModuleReading,
    /// What running `conf.py` found, when it ran: then the only source of
    /// values.
    executed: Option<&'a BTreeMap<String, ExecutedValue>>,
    findings: Vec<Diagnostic>,
    /// Every setting asked for, in order.
    asked: Vec<String>,
}

impl<'a> Reader<'a> {
    fn settings(&mut self) -> (ProjectSettings, Option<NeedsDeclaration>) {
        let defaults = ProjectSettings::default();
        let root_doc = self
            .string("root_doc")
            .or_else(|| self.string("master_doc"))
            .unwrap_or(defaults.root_doc);
        let domain = self.primary_domain();
        let default_role = self.default_role(domain);
        let extensions = self.strings("extensions").unwrap_or_default();
        let needs = self.needs(&extensions);
        let settings = ProjectSettings {
            root_doc,
            source_suffixes: self.source_suffixes(),
            exclusion: self.exclusion(),
            parse: ParseSettings {
                domain,
                default_role,
                prolog: self.text("rst_prolog"),
                epilog: self.text("rst_epilog"),
                ..ParseSettings::default()
            },
            numfig: self.boolean("numfig").unwrap_or(defaults.numfig),
            numfig_secnum_depth: self
                .count("numfig_secnum_depth")
                .unwrap_or(defaults.numfig_secnum_depth),
            highlight_language: self
                .highlight_language()
                .unwrap_or(defaults.highlight_language),
            intersphinx: self.intersphinx(&extensions),
            strictness: Strictness::legacy(&extensions),
            ..defaults
        };
        (settings, needs)
    }

    /// The literal `needs_*` settings — read only when `extensions` declares
    /// sphinx-needs, as Sphinx ignores them otherwise.
    fn needs(&mut self, extensions: &[String]) -> Option<NeedsDeclaration> {
        if !extensions
            .iter()
            .any(|name| NEEDS_EXTENSIONS.contains(&name.as_str()))
        {
            return None;
        }
        let literals: Vec<(&str, &Value)> = conf_setting_names()
            .filter_map(|(setting, name)| Some((setting, self.literal(&name)?)))
            .collect();
        let from_toml = self.string("needs_from_toml");
        let toml_root = self.strings("needs_from_toml_table").unwrap_or_default();
        let anchor = self
            .module
            .binding("needs_from_toml")
            .or_else(|| self.module.binding("extensions"))
            .map(|binding| to_span(binding.target));
        let findings = &mut self.findings;
        Some(read_declaration(
            &literals,
            from_toml,
            toml_root,
            anchor,
            |value, message| {
                findings.push(Diagnostic::new(
                    DiagnosticCode::ConfInvalidValue,
                    message,
                    to_span(value.span),
                ));
            },
        ))
    }

    /// The literal `name` is bound to, reporting each change made to it
    /// afterwards — or `None`, reported, when it is bound to something else,
    /// and `None` silently when it is not bound at all.
    fn literal(&mut self, name: &str) -> Option<&'a Value> {
        self.asked.push(name.to_string());
        if let Some(executed) = self.executed {
            return match executed.get(name)? {
                ExecutedValue::Literal(value) => Some(value),
                ExecutedValue::Unrepresentable(spelling) => {
                    self.findings.push(Diagnostic::new(
                        DiagnosticCode::ConfUnrepresentableValue,
                        format!(
                            "running `conf.py` makes `{name}` {}, which no literal spells, so rinx uses its default",
                            abbreviated(spelling)
                        ),
                        to_span(anchor(self.module, name)),
                    ));
                    None
                }
            };
        }
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

    /// A string setting whose empty value means none, as Sphinx's
    /// `if prolog:` reads `rst_prolog`.
    fn text(&mut self, name: &str) -> Option<String> {
        self.string(name).filter(|text| !text.is_empty())
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

    /// `intersphinx_mapping` and `intersphinx_cache_limit` — read only when
    /// `extensions` declares intersphinx, as Sphinx ignores them otherwise.
    fn intersphinx(&mut self, extensions: &[String]) -> IntersphinxSettings {
        if !extensions.iter().any(|name| name == INTERSPHINX) {
            return IntersphinxSettings::default();
        }
        let entries = match self.literal("intersphinx_mapping") {
            Some(mapping) => {
                let findings = &mut self.findings;
                read_mapping(
                    mapping,
                    |value| to_span(value.span),
                    |value, message| {
                        findings.push(Diagnostic::new(
                            DiagnosticCode::ConfInvalidValue,
                            message,
                            to_span(value.span),
                        ));
                    },
                )
            }
            None => Vec::new(),
        };
        IntersphinxSettings {
            entries,
            cache_limit_days: self
                .whole_number("intersphinx_cache_limit")
                .unwrap_or(DEFAULT_CACHE_LIMIT_DAYS),
        }
    }

    fn whole_number(&mut self, name: &str) -> Option<i64> {
        let value = self.literal(name)?;
        if let ValueKind::Int(number) = value.kind {
            return Some(number);
        }
        self.invalid(value, format!("`{name}` must be a whole number"));
        None
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

/// The longest `repr` a finding quotes whole.
const QUOTED_SPELLING: usize = 60;

/// `spelling`, quoted, cut short when long.
fn abbreviated(spelling: &str) -> String {
    if spelling.chars().count() <= QUOTED_SPELLING {
        return format!("`{spelling}`");
    }
    let start: String = spelling.chars().take(QUOTED_SPELLING).collect();
    format!("`{start}…`")
}

/// `span` as a diagnostic's span: the same lines and character columns.
pub(super) fn to_span(span: rinx_pyconf::Span) -> Span {
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
    fn test_read_sphinx_conf_reads_the_intersphinx_settings_of_a_project_declaring_it() {
        // Given
        let text = "extensions = ['sphinx.ext.intersphinx']\nintersphinx_mapping = {'python': ('https://docs.python.org/3', None)}\nintersphinx_cache_limit = -1\n";

        // When
        let settings = settings(text);

        // Then
        assert_eq!(settings.intersphinx.entries.len(), 1);
        assert_eq!(settings.intersphinx.entries[0].name.as_str(), "python");
        assert_eq!(settings.intersphinx.cache_limit_days, -1);
        assert!(findings(text).is_empty());
    }

    #[test]
    fn test_read_sphinx_conf_ignores_intersphinx_settings_without_the_extension() {
        // Given — Sphinx ignores the mapping when the extension is not loaded
        let text = "intersphinx_mapping = {'python': ('https://docs.python.org/3', None)}\nintersphinx_cache_limit = 'x'\n";

        // When / Then
        assert!(settings(text).intersphinx.entries.is_empty());
        assert!(findings(text).is_empty());
    }

    #[test]
    fn test_read_sphinx_conf_reports_a_malformed_intersphinx_entry_where_it_is_written() {
        // Given
        let text = "extensions = ['sphinx.ext.intersphinx']\nintersphinx_mapping = {\n    'ok': ('https://ok.org', None),\n    'https://old.org': None,\n}\nintersphinx_cache_limit = 1.5\n";

        // When
        let reading = read_sphinx_conf(text);

        // Then
        assert_eq!(reading.settings.intersphinx.entries.len(), 1);
        assert_eq!(reading.settings.intersphinx.entries[0].span.start.line, 3);
        assert_eq!(reading.settings.intersphinx.cache_limit_days, 5);
        assert_eq!(
            findings(text),
            [("conf.invalid-value", 4), ("conf.invalid-value", 6)]
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
    fn test_read_sphinx_conf_reads_the_prolog_and_the_epilog() {
        // When
        let parse = settings(
            "rst_prolog = '.. |a| replace:: A'\nrst_epilog = '''\n.. _b: https://b\n'''\n",
        )
        .parse;

        // Then
        assert_eq!(parse.prolog.as_deref(), Some(".. |a| replace:: A"));
        assert_eq!(parse.epilog.as_deref(), Some("\n.. _b: https://b\n"));
    }

    #[test]
    fn test_read_sphinx_conf_reads_an_empty_prolog_as_none() {
        // When / Then
        assert_eq!(settings("rst_prolog = ''\n").parse.prolog, None);
    }

    #[test]
    fn test_read_sphinx_conf_reports_a_computed_epilog_and_reads_none() {
        // Given
        let text = "version = '1'\nrst_epilog = f'.. |v| replace:: {version}'\n";

        // When / Then
        assert_eq!(settings(text).parse.epilog, None);
        assert_eq!(findings(text), [("conf.unread-setting", 2)]);
    }

    #[test]
    fn test_read_sphinx_conf_reports_a_change_to_the_prolog() {
        // Given
        let text = "rst_prolog = 'A'\nif x:\n    rst_prolog += 'B'\n";

        // When / Then
        assert_eq!(settings(text).parse.prolog.as_deref(), Some("A"));
        assert_eq!(findings(text), [("conf.modified-setting", 3)]);
    }

    #[test]
    fn test_read_sphinx_conf_reports_a_prolog_of_the_wrong_kind() {
        // When / Then
        assert_eq!(findings("rst_prolog = 3\n"), [("conf.invalid-value", 1)]);
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
        // changes too, but is not read. `rst_epilog` is an f-string over an
        // imported `version`.
        assert_eq!(
            findings(text),
            [
                ("conf.modified-setting", 51),
                ("conf.unread-setting", 79),
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

    /// What running a `conf.py` found: each of `values`, a name and the
    /// `repr` it printed.
    fn executed(values: &[(&str, &str)]) -> ConfValues {
        let output = serde_json::json!({
            "values": values.iter().copied().collect::<BTreeMap<&str, &str>>(),
        });
        super::super::conf_eval::read_output_for_tests(&output.to_string())
    }

    #[test]
    fn test_read_executed_sphinx_conf_takes_the_computed_values() {
        // Given
        let text = "version = '3.14'\nrst_epilog = f'|v| {version}'\nextensions = []\nextensions += ['x']\n";
        let values = executed(&[("rst_epilog", "'|v| 3.14'"), ("extensions", "['x']")]);

        // When
        let reading = read_executed_sphinx_conf(text, &values);

        // Then nothing is reported unread or unapplied
        assert_eq!(reading.settings.parse.epilog.as_deref(), Some("|v| 3.14"));
        assert_eq!(reading.settings.strictness.unmodelled_extensions(), ["x"]);
        assert_eq!(reading.findings, []);
        assert!(reading.falls_short);
    }

    #[test]
    fn test_read_executed_sphinx_conf_keeps_the_default_of_a_setting_conf_py_did_not_bind() {
        // Given a setting bound only on a path the run did not take
        let text = "import os\nif os.environ.get('NUMFIG'):\n    numfig = True\n";

        // When
        let reading = read_executed_sphinx_conf(text, &executed(&[]));

        // Then
        assert!(!reading.settings.numfig);
        assert_eq!(reading.findings, []);
    }

    #[test]
    fn test_read_executed_sphinx_conf_does_not_report_what_reading_could_not_see() {
        // Given a wildcard import and a line the reader stops at
        let text = "from settings import *\nproject = (\n";

        // When
        let reading = read_executed_sphinx_conf(text, &executed(&[]));

        // Then
        assert_eq!(reading.findings, []);
        assert_eq!(
            findings(text),
            [("conf.wildcard-import", 1), ("conf.syntax-error", 2)]
        );
    }

    #[test]
    fn test_read_executed_sphinx_conf_reports_a_wrong_value_where_it_is_assigned() {
        // Given
        let text = "\nnumfig = compute()\n";

        // When
        let reading = read_executed_sphinx_conf(text, &executed(&[("numfig", "'yes'")]));

        // Then
        let codes: Vec<(&str, u32)> = reading
            .findings
            .iter()
            .map(|finding| {
                (
                    finding.code.as_str(),
                    finding.span.map_or(0, |span| span.start.line),
                )
            })
            .collect();
        assert_eq!(codes, [("conf.invalid-value", 2)]);
    }

    #[test]
    fn test_read_executed_sphinx_conf_reports_a_value_no_literal_spells() {
        // Given
        let text = "rst_epilog = make()\n";

        // When
        let reading =
            read_executed_sphinx_conf(text, &executed(&[("rst_epilog", "<Epilog at 0x1>")]));

        // Then
        assert_eq!(reading.settings.parse.epilog, None);
        assert_eq!(reading.findings.len(), 1);
        assert_eq!(
            reading.findings[0].code,
            DiagnosticCode::ConfUnrepresentableValue
        );
        assert!(
            reading.findings[0].message.contains("`<Epilog at 0x1>`"),
            "{}",
            reading.findings[0].message
        );
    }

    #[test]
    fn test_read_sphinx_conf_says_whether_reading_fell_short() {
        // When / Then
        assert!(!read_sphinx_conf("project = 'P'\nnumfig = True\n").falls_short);
        assert!(!read_sphinx_conf("html_theme = pick()\n").falls_short);
        assert!(read_sphinx_conf("numfig = pick()\n").falls_short);
        assert!(read_sphinx_conf("extensions = []\nextensions.append('x')\n").falls_short);
        assert!(read_sphinx_conf("from x import *\n").falls_short);
        assert!(read_sphinx_conf("numfig = (\n").falls_short);
    }

    #[test]
    fn test_a_run_is_asked_for_every_setting_the_reader_reads() {
        // Given a `conf.py` declaring every extension whose settings are read
        let module = read_module(
            "extensions = ['sphinx.ext.intersphinx', 'sphinx_needs']\nneeds_from_toml = 'u.toml'\n",
        );
        let mut reader = Reader {
            module: &module,
            executed: None,
            findings: Vec::new(),
            asked: Vec::new(),
        };

        // When
        reader.settings();

        // Then
        let asked_for = conf_settings();
        let missing: Vec<&String> = reader
            .asked
            .iter()
            .filter(|name| !asked_for.contains(name))
            .collect();
        assert_eq!(missing, Vec::<&String>::new());
    }

    #[test]
    fn test_anchored_positions_a_value_and_everything_in_it() {
        // Given
        let module = read_module("\n\nexclude_patterns = compute()\n");
        let ExecutedValue::Literal(value) =
            super::super::conf_eval::executed_value_for_tests("{'a': ['b']}")
        else {
            panic!("a literal");
        };

        // When
        let span = anchor(&module, "exclude_patterns");
        let value = anchored(&value, span);

        // Then
        assert_eq!(span.start.line, 3);
        let ValueKind::Dict(entries) = &value.kind else {
            panic!("a dict");
        };
        let (key, item) = &entries[0];
        assert_eq!([value.span, key.span, item.span], [span; 3]);
        assert_eq!(item.as_sequence().map(|items| items[0].span), Some(span));
        assert_eq!(
            anchor(&module, "numfig").start,
            rinx_pyconf::Position::START
        );
    }

    #[test]
    fn test_abbreviated_cuts_a_long_spelling_short() {
        // When / Then
        assert_eq!(abbreviated("<x>"), "`<x>`");
        let long = "x".repeat(QUOTED_SPELLING + 5);
        assert_eq!(
            abbreviated(&long),
            format!("`{}…`", "x".repeat(QUOTED_SPELLING))
        );
    }
}
