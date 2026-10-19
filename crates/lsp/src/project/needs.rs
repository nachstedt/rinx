//! A Sphinx project's sphinx-needs configuration, turned into the entity
//! schema its documents are parsed against (ADR-038 §4).
//!
//! sphinx-needs reads its `needs_*` settings from `conf.py` and, when
//! `needs_from_toml` names one, from the `[needs]` table of a
//! `ubproject.toml`, which overrides `conf.py` one setting at a time; a
//! `schemas.json` named by `needs_schema_definitions_from_json` adds
//! validation rules. Reading happens in two steps, because `conf.py` is read
//! as text and the other two are files:
//!
//! - [`read_declaration`], called while `conf.py` is read, takes its literal
//!   `needs_*` settings into a [`NeedsDeclaration`], reporting a setting of
//!   the wrong shape where it is written;
//! - [`resolve_needs`], called where the project is found on the disk, reads
//!   the files the declaration names and converts the whole through
//!   `rinx_entity::needs_conf` — the conversion `rinx needs_schema` runs —
//!   into the schema, reporting what it cannot express on the declaration's
//!   line.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rinx_ast::{Diagnostic, DiagnosticCode, Span};
use rinx_entity::needs_conf::{
    NEEDS_SETTINGS, NeedsConfig, ReportEntry, SchemasFile, convert_needs,
};
use rinx_entity::{EntitySchema, load_raw_schema};
use rinx_pyconf::{Value, ValueKind};
use serde::Deserialize;

use super::python_value::PythonValue;

/// The module names a `conf.py` declares sphinx-needs under: its own, and
/// the `sphinxcontrib` one it was published as before.
pub(crate) const NEEDS_EXTENSIONS: &[&str] = &["sphinx_needs", "sphinxcontrib.needs"];

/// What `conf.py` says about a project's sphinx-needs configuration, before
/// the files it names are read.
#[derive(Debug, Clone, PartialEq)]
pub struct NeedsDeclaration {
    /// The literal `needs_*` settings.
    pub config: NeedsConfig,
    /// `needs_from_toml`: the `ubproject.toml`, relative to `conf.py`.
    pub from_toml: Option<String>,
    /// `needs_from_toml_table`: the table the `needs` table is under.
    pub toml_root: Vec<String>,
    /// Where a finding about the configuration as a whole is placed: the
    /// `needs_from_toml` setting, or else the `extensions` that declare
    /// sphinx-needs.
    pub anchor: Option<Span>,
}

/// The settings of one literal `needs_*` setting each, as `ubproject.toml`
/// keys them, deserialized into one configuration; each setting of the
/// wrong shape is reported with `invalid` and left out.
pub(crate) fn read_declaration(
    literals: &[(&str, &Value)],
    from_toml: Option<String>,
    toml_root: Vec<String>,
    anchor: Option<Span>,
    mut invalid: impl FnMut(&Value, String),
) -> NeedsDeclaration {
    let mut accepted = Vec::new();
    for (setting, value) in literals {
        let single = [(*setting, *value)];
        match deserialize_settings(&single) {
            Ok(_) => accepted.push((*setting, *value)),
            Err(message) => invalid(value, format!("`needs_{setting}` {message}")),
        }
    }
    NeedsDeclaration {
        config: deserialize_settings(&accepted).unwrap_or_default(),
        from_toml,
        toml_root,
        anchor,
    }
}

/// The configuration the settings `settings` write.
fn deserialize_settings(settings: &[(&str, &Value)]) -> Result<NeedsConfig, String> {
    let Some((_, first)) = settings.first() else {
        return Ok(NeedsConfig::default());
    };
    let key = |name: &str| Value {
        kind: ValueKind::Str(name.to_string()),
        span: first.span,
    };
    let table = Value {
        kind: ValueKind::Dict(
            settings
                .iter()
                .map(|(name, value)| (key(name), (*value).clone()))
                .collect(),
        ),
        span: first.span,
    };
    NeedsConfig::deserialize(PythonValue(&table)).map_err(|error| format!("is invalid: {error}"))
}

/// The `needs_*` settings the conversion reads, as `conf.py` spells them.
pub(crate) fn conf_setting_names() -> impl Iterator<Item = (&'static str, String)> {
    NEEDS_SETTINGS
        .iter()
        .map(|setting| (*setting, format!("needs_{setting}")))
}

/// What a project's sphinx-needs configuration makes of it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedNeeds {
    pub schema: EntitySchema,
    /// The schema's import keys, resolved for the parser.
    pub import_keys: BTreeMap<String, String>,
    /// The files read besides `conf.py`, existing or not.
    pub files: Vec<PathBuf>,
}

/// The entity schema `declaration` describes for the project whose
/// `conf.py` is in `conf_dir`, reading the files it names and reporting into
/// `findings` — positioned in `conf.py` — what could not be read or
/// converted.
pub(crate) fn resolve_needs(
    conf_dir: &Path,
    declaration: &NeedsDeclaration,
    findings: &mut Vec<Diagnostic>,
) -> ResolvedNeeds {
    let mut report = Reporter {
        anchor: declaration.anchor,
        findings,
    };
    let mut files = Vec::new();
    let mut config = declaration.config.clone();
    let mut toml_settings: Vec<String> = Vec::new();
    let mut toml_file: Option<PathBuf> = None;
    if let Some(written) = &declaration.from_toml {
        let path = conf_dir.join(written);
        files.push(path.clone());
        match fs::read_to_string(&path) {
            Err(error) => report.invalid(format!("cannot read `{written}`: {error}")),
            Ok(text) => {
                let root: Vec<&str> = declaration.toml_root.iter().map(String::as_str).collect();
                match NeedsConfig::from_toml(&text, &root) {
                    Ok((from_toml, written_settings)) => {
                        config = from_toml.or(config);
                        toml_settings = written_settings;
                        toml_file = Some(PathBuf::from(written));
                    }
                    Err(error) => report.invalid(format!("cannot read `{written}`: {error}")),
                }
            }
        }
    }
    // A relative path in the `ubproject.toml` is relative to it, as
    // sphinx-needs resolves one; in `conf.py`, to `conf.py`.
    let anchor_of = |setting: &str| -> String {
        let file = toml_file
            .as_ref()
            .filter(|_| toml_settings.iter().any(|written| written == setting))
            .map_or_else(|| PathBuf::from("conf.py"), Clone::clone);
        file.to_string_lossy().into_owned()
    };

    let schemas = config
        .schema_definitions_from_json
        .clone()
        .and_then(|written| {
            let relative = rinx_ast::resolve_from_document(
                &written,
                &anchor_of("schema_definitions_from_json"),
            );
            let path = conf_dir.join(relative);
            files.push(path.clone());
            let text = fs::read_to_string(&path)
                .map_err(|error| report.invalid(format!("cannot read `{written}`: {error}")))
                .ok()?;
            SchemasFile::from_json(&text)
                .map_err(|error| report.invalid(format!("cannot read `{written}`: {error}")))
                .ok()
        });

    let (declarations, unconverted) = convert_needs(&config, schemas.as_ref());
    for entry in unconverted {
        report.unconverted(&entry);
    }
    let schema = load_raw_schema(declarations, &rinx_parser::is_builtin_directive_name)
        .unwrap_or_else(|errors| {
            report.invalid(format!(
                "the need types it declares are no valid entity schema:\n{errors}"
            ));
            EntitySchema::empty()
        });
    let import_keys = schema.resolved_import_keys(&anchor_of("import_keys"));
    ResolvedNeeds {
        schema,
        import_keys,
        files,
    }
}

/// Reports findings about a sphinx-needs configuration where it is declared.
struct Reporter<'a> {
    anchor: Option<Span>,
    findings: &'a mut Vec<Diagnostic>,
}

impl Reporter<'_> {
    fn invalid(&mut self, message: String) {
        self.push(DiagnosticCode::ConfNeedsInvalid, message);
    }

    fn unconverted(&mut self, entry: &ReportEntry) {
        self.push(
            DiagnosticCode::ConfNeedsUnconverted,
            format!(
                "sphinx-needs {}, which rinx reads the project without: {}",
                entry.category.as_str(),
                entry.detail
            ),
        );
    }

    fn push(&mut self, code: DiagnosticCode, message: String) {
        let finding = match self.anchor {
            Some(span) => Diagnostic::new(code, message, span),
            None => Diagnostic::at(code, message, None),
        };
        self.findings.push(finding);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::read_sphinx_conf;

    /// A scratch directory holding `files`.
    fn temp_tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rinx_lsp_needs_{name}"));
        let _ = fs::remove_dir_all(&dir);
        for (relative, text) in files {
            let path = dir.join(relative);
            fs::create_dir_all(path.parent().expect("a parent")).expect("create dir");
            fs::write(path, text).expect("write file");
        }
        fs::create_dir_all(&dir).expect("create dir");
        dir
    }

    /// What reading `conf` and the files beside it in `dir` resolves to, and
    /// the codes reported, each with its line.
    fn resolved(dir: &Path, conf: &str) -> (ResolvedNeeds, Vec<(&'static str, u32)>) {
        let mut reading = read_sphinx_conf(conf);
        let declaration = reading.needs.clone().expect("sphinx-needs is declared");
        let needs = resolve_needs(dir, &declaration, &mut reading.findings);
        let codes = reading
            .findings
            .iter()
            .map(|finding| {
                (
                    finding.code.as_str(),
                    finding.span.map_or(0, |s| s.start.line),
                )
            })
            .collect();
        (needs, codes)
    }

    const NEEDS: &str = "extensions = ['sphinx_needs']\n";

    #[test]
    fn test_read_sphinx_conf_declares_needs_only_with_the_extension() {
        // Given / When / Then
        assert!(read_sphinx_conf("needs_types = []\n").needs.is_none());
        assert!(read_sphinx_conf(NEEDS).needs.is_some());
        assert!(
            read_sphinx_conf("extensions = ['sphinxcontrib.needs']\n")
                .needs
                .is_some()
        );
    }

    #[test]
    fn test_a_setting_of_the_wrong_shape_is_reported_where_it_is_written() {
        // Given
        let conf = format!("{NEEDS}needs_types = 'req'\nneeds_id_required = True\n");

        // When
        let reading = read_sphinx_conf(&conf);

        // Then the other settings are still read
        let declaration = reading.needs.expect("declared");
        assert!(declaration.config.id_required);
        assert!(declaration.config.types.is_empty());
        let invalid: Vec<u32> = reading
            .findings
            .iter()
            .filter(|f| f.code == DiagnosticCode::ConfInvalidValue)
            .map(|f| f.span.map_or(0, |s| s.start.line))
            .collect();
        assert_eq!(invalid, [2]);
    }

    #[test]
    fn test_ubproject_toml_overrides_conf_py_one_setting_at_a_time() {
        // Given `id_required` in `conf.py` and the types in the TOML, which
        // sets `types` again
        let dir = temp_tree(
            "override",
            &[("ubproject.toml", "[[needs.types]]\ndirective = \"spec\"\n")],
        );
        let conf = format!(
            "{NEEDS}needs_from_toml = 'ubproject.toml'\nneeds_id_required = True\nneeds_types = [{{'directive': 'req'}}]\n"
        );

        // When
        let (needs, _) = resolved(&dir, &conf);

        // Then
        assert!(needs.schema.entity_type("spec").is_some());
        assert!(needs.schema.entity_type("req").is_none());
        assert!(
            needs
                .schema
                .entity_type("spec")
                .is_some_and(|t| t.id.required)
        );
        assert_eq!(needs.files, [dir.join("ubproject.toml")]);
    }

    #[test]
    fn test_a_table_under_another_root_is_read_from_there() {
        // Given
        let dir = temp_tree(
            "root",
            &[(
                "pyproject.toml",
                "[[tool.needs.types]]\ndirective = \"req\"\n",
            )],
        );
        let conf = format!(
            "{NEEDS}needs_from_toml = 'pyproject.toml'\nneeds_from_toml_table = ['tool']\n"
        );

        // When
        let (needs, _) = resolved(&dir, &conf);

        // Then
        assert!(needs.schema.entity_type("req").is_some());
    }

    #[test]
    fn test_a_path_in_the_toml_is_relative_to_the_toml() {
        // Given a `ubproject.toml` beside its `schemas.json`, below `conf.py`
        let dir = temp_tree(
            "relative",
            &[
                (
                    "config/ubproject.toml",
                    "[needs]\nschema_definitions_from_json = \"schemas.json\"\nimport_keys = { up = \"up.json\" }\n[[needs.types]]\ndirective = \"req\"\n",
                ),
                (
                    "config/schemas.json",
                    r##"{"schemas": [{"id": "r", "select": {"$ref": "#/$defs/type-req"}, "validate": {"local": {"properties": {"id": {"pattern": "^R"}}}}}]}"##,
                ),
            ],
        );
        let conf = format!("{NEEDS}needs_from_toml = 'config/ubproject.toml'\n");

        // When
        let (needs, codes) = resolved(&dir, &conf);

        // Then
        assert!(
            codes.iter().all(|(code, _)| *code != "conf.needs-invalid"),
            "{codes:?}"
        );
        let pattern = needs
            .schema
            .entity_type("req")
            .and_then(|t| t.id.pattern.as_ref());
        assert_eq!(pattern.map(rinx_entity::ValuePattern::as_str), Some("^R"));
        assert_eq!(
            needs.import_keys.get("up").map(String::as_str),
            Some("/config/up.json")
        );
        assert!(needs.files.contains(&dir.join("config/schemas.json")));
    }

    #[test]
    fn test_findings_about_the_files_are_placed_on_needs_from_toml() {
        // Given a TOML that is none
        let dir = temp_tree("broken", &[("ubproject.toml", "[needs")]);
        let conf = format!("{NEEDS}\nneeds_from_toml = 'ubproject.toml'\n");

        // When
        let (needs, codes) = resolved(&dir, &conf);

        // Then
        assert!(codes.contains(&("conf.needs-invalid", 3)), "{codes:?}");
        assert!(needs.schema.is_empty());
    }

    #[test]
    fn test_types_that_contradict_each_other_are_reported_and_declare_nothing() {
        // Given one type declared twice
        let conf =
            format!("{NEEDS}needs_types = [{{'directive': 'req'}}, {{'directive': 'req'}}]\n");
        let dir = temp_tree("duplicate", &[]);

        // When
        let (needs, codes) = resolved(&dir, &conf);

        // Then — placed on `extensions`, with no `needs_from_toml` to name
        assert!(codes.contains(&("conf.needs-invalid", 1)), "{codes:?}");
        assert!(needs.schema.is_empty());
    }
}
