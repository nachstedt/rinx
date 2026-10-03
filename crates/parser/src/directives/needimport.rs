//! `.. needimport::` — reading entities out of a sphinx-needs `needs.json`.
//!
//! The third **splicing** directive, beside `.. include::` and
//! `.. if-builder::`, and for the same reason they are: what it contributes to
//! the enclosing block is not one node but any number of them. Each imported
//! need becomes an ordinary [`Directive::Entity`], indistinguishable from one
//! an author typed — which is the whole design. The analyzer indexes it, the
//! `:ref:` machinery registers it, back-links derive through it, and
//! `.. entity-table::` and `.. entity-flow::` see it, with not one line
//! changed in any of those phases. Wrapping the imports in a container node
//! would have cost every one of them.
//!
//! That is also why the import happens *here*, while parsing, rather than
//! being merged into the project index by a later action: an entity that no
//! document holds has no anchor, no `:ref:` target and nowhere to render, so
//! an index-time merge would need an "external entity" concept this model does
//! not have — and every view would then need to know about it.
//!
//! Two things about this directive are deliberately narrow, and both are
//! recorded in `docs/decisions/016-needimport.md`:
//!
//! - **The name is sphinx-needs' own, with no alias of this build's.** Every
//!   other borrowed construct here takes a local name and accepts the foreign
//!   spelling beside it. This one does not, because it is scoped as a
//!   migration bridge rather than as this project's import: `entity-import`
//!   stays unclaimed for the richer construct that may replace it, which would
//!   bring its own format, its own sections and its own diagnostic family.
//! - **Nothing is fetched.** sphinx-needs accepts a URL; a sandboxed build
//!   action may only read files declared before it runs, so a URL is refused
//!   by name rather than downloaded.
//!
//! An argument may also be a *name* rather than a path, which sphinx-needs
//! resolves through `needs_import_keys` in `conf.py`. This build declares the
//! same map as an `[import_keys]` table in the entity schema — the alias is
//! written in a document, so its meaning belongs with the project's own
//! vocabulary — and [`ParseCtx::import_keys`] carries it here already
//! resolved. A name matching no key is refused rather than opened as a file,
//! so a missing declaration cannot fail the build the way an undeclared
//! `parse_data` entry does.
//!
//! Positions are the other thing worth knowing. JSON carries no line numbers a
//! diagnostic could point at, so everything reported here lands on the
//! `.. needimport::` line itself — the line the author can actually act on —
//! with the offending need's id in the message. Imported prose parses under
//! [`ParseCtx::synthetic`], so a diagnostic from inside it is positionless
//! rather than wrong, exactly as a `.. csv-table::`'s `:file:` rows already
//! are.

use std::collections::{BTreeMap, BTreeSet};

use rinx_ast::{
    AttributeValue, Diagnostic, DiagnosticCode, Directive, EntityBody, EntityId, EntitySection,
    Node, Span,
};
use rinx_entity::{
    EntityType, NeedsVersion, RawNeed, field_is_list, field_is_null, field_text, is_internal_field,
    parse_attribute_value, read_needs_json, split_list,
};
use rinx_filter::{Expr, FieldName, FieldValue, FilterSubject};

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;

use super::entity_fields::{
    apply_defaults, collect_relation_targets, describe_options, report_id_pattern,
    report_missing_attributes, report_relation_cardinality, store_attribute,
};
use super::error_node::malformed_directive;
use super::filter_option::{FilterCodes, read_filter_option};
use super::options::{OptionLine, report_unknown_options, scan_option_lines};

/// The directive name, used throughout this module's diagnostics.
const DIRECTIVE: &str = "needimport";

/// The options sphinx-needs' `needimport` accepts that this build does not,
/// each with what an author should reach for instead.
///
/// A table rather than a match arm apiece, for the reason `.. entity-flow::`
/// keeps one: the sentence beside each name *is* the diagnostic. Every entry
/// here is presentation or Python — the two things an imported entity gets
/// from this project's own schema and site config instead.
const UNSUPPORTED_OPTIONS: [(&str, &str); 7] = [
    (
        "hide",
        "every imported entity is rendered; presentation belongs in the site config",
    ),
    (
        "collapse",
        "collapsing is a render-time setting in rinx.toml, not a per-import one",
    ),
    (
        "layout",
        "rendering is the schema's `template`, declared per entity type",
    ),
    (
        "style",
        "rendering is the schema's `template`, declared per entity type",
    ),
    (
        "setup",
        "this build runs no Python; the schema decides what a type accepts",
    ),
    (
        "pre_template",
        "this build runs no Python; the schema decides what a type accepts",
    ),
    (
        "post_template",
        "this build runs no Python; the schema decides what a type accepts",
    ),
];

/// The `:option:` lines this directive reads, as written.
#[derive(Default)]
struct ImportOptions {
    /// `:version:` — which version block to take.
    version: Option<String>,
    /// `:ids:` — the only needs to import, when given.
    ids: Option<Vec<String>>,
    /// `:filter:` — the needs to import, when given.
    filter: Option<Expr>,
    /// `:id_prefix:` — prepended to every imported id.
    id_prefix: String,
    /// `:tags:` — values added to each imported entity's `tags` attribute.
    tags: Vec<String>,
}

/// The one import in progress, as every need it builds sees it.
///
/// One value rather than four parameters because the four are read together by
/// every step below and answer the same question from different sides: what
/// was asked for (`options`), which ids this import owns (`imported`), which
/// file said so (`file_id`) and which line to report against (`span`). Passing
/// them apart is also what let two functions here grow past the argument count
/// clippy is willing to read.
struct Import<'a> {
    options: &'a ImportOptions,
    /// The ids this import brings in, so a relation naming one of them can be
    /// prefixed with it and one naming anything else is left alone.
    imported: BTreeSet<&'a str>,
    /// The resolved path of the `needs.json`, for a diagnostic to name.
    file_id: &'a str,
    /// The `.. needimport::` line — the only position anything here has.
    span: Option<Span>,
    /// The project's meta-model, for the back-links it derives.
    schema: &'a rinx_entity::EntitySchema,
}

/// Parses a `.. needimport::`, returning the entities its file contributed.
///
/// Returns a single degraded [`Directive::Malformed`] node when the import
/// could not happen at all, having reported why — the parser stays resilient,
/// and the `parse` subcommand turns the loader's recorded failure into a
/// failed build.
pub(in crate::directives) fn parse_needimport(
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<Node> {
    let path = match resolve_argument(argument, body_lines, directive_span, diagnostics, ctx) {
        Ok(path) => path,
        Err(refusal) => return vec![*refusal],
    };

    let unindented = unindent_body_lines(body_lines);
    let (option_lines, _) = scan_option_lines(&unindented);
    let options = parse_options(&option_lines, diagnostics, ctx);

    let file = match load_needs_file(path, ctx) {
        Ok(file) => file,
        Err((code, message)) => {
            return vec![refuse(
                argument,
                body_lines,
                code,
                message,
                directive_span,
                diagnostics,
            )];
        }
    };

    let version = match file.contents.select_version(options.version.as_deref()) {
        Ok((_, version)) => version,
        Err(error) => {
            return vec![refuse(
                argument,
                body_lines,
                DiagnosticCode::NeedImportUnknownVersion,
                format!("{DIRECTIVE}: '{}': {error}", file.id),
                directive_span,
                diagnostics,
            )];
        }
    };

    let selected = select_needs(version, &options, directive_span, diagnostics, ctx);
    if selected.is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::NeedImportEmptyResult,
            format!(
                "{DIRECTIVE}: '{}' contributed no entities; \
                 check the :ids: and :filter: options",
                file.id
            ),
            directive_span,
        ));
        return Vec::new();
    }

    let import = Import {
        options: &options,
        imported: selected
            .iter()
            .map(|(key, need)| need.id.as_deref().unwrap_or(key))
            .collect(),
        file_id: &file.id,
        span: directive_span,
        schema: ctx.schema,
    };

    selected
        .iter()
        .filter_map(|(key, need)| {
            build_entity(key, need, &import, adornment_order, diagnostics, ctx)
        })
        .map(|entity| Node::Directive(Directive::Entity(Box::new(entity))))
        .collect()
}

/// The `needs.json` a `.. needimport::` names, read and deserialized.
struct NeedsSource {
    /// The resolved path, as a diagnostic should name it.
    id: String,
    contents: rinx_entity::NeedsFile,
}

/// Reads and deserializes the file, or says which of the two failed.
///
/// Returns the code and message rather than reporting, so the one caller
/// builds the node a reader sees and the diagnostic a log records from the
/// same string.
fn load_needs_file(
    path: &str,
    ctx: &ParseCtx<'_>,
) -> Result<NeedsSource, (DiagnosticCode, String)> {
    let file = ctx
        .files
        .load(path, ctx.current_file())
        .map_err(|message| {
            (
                DiagnosticCode::NeedImportFileUnreadable,
                format!("{DIRECTIVE}: {message}"),
            )
        })?;
    let contents = read_needs_json(&file.text).map_err(|message| {
        (
            DiagnosticCode::NeedImportMalformedJson,
            format!(
                "{DIRECTIVE}: '{}' is not a readable needs.json: {message}",
                file.id
            ),
        )
    })?;
    Ok(NeedsSource {
        id: file.id,
        contents,
    })
}

/// Works out which file the argument names, or the refusal that says why it
/// names none.
///
/// The error is boxed because a [`Node`] is large and this is the rare path;
/// an unboxed one would cost every caller of this function the size of the
/// biggest node in the tree.
///
/// Four answers in one place because they are one question — *what is this
/// argument?* — and because the order matters: the import-key lookup runs
/// first, so every check below it sees a path.
fn resolve_argument<'a>(
    argument: &'a str,
    body_lines: &[&str],
    directive_span: Option<Span>,
    diagnostics: &mut Diagnostics,
    ctx: &'a ParseCtx<'_>,
) -> Result<&'a str, Box<Node>> {
    let written = argument.trim();
    // Looked up *before* every other check on the argument, which is the order
    // sphinx-needs itself resolves in: a key may legally be spelled with a
    // `.json` suffix, and it then wins over a file of that name.
    let path = match ctx.import_keys.get(written) {
        Some(resolved) => resolved.as_str(),
        None => written,
    };
    if path.is_empty() {
        return Err(Box::new(refuse(
            argument,
            body_lines,
            DiagnosticCode::NeedImportMissingPath,
            format!("{DIRECTIVE}: needs the path of a needs.json to import"),
            directive_span,
            diagnostics,
        )));
    }
    if is_url(path) {
        return Err(Box::new(refuse(
            argument,
            body_lines,
            DiagnosticCode::NeedImportRemoteSource,
            format!(
                "{DIRECTIVE}: '{path}' is a URL, and nothing is fetched while building — \
                 save the file next to the document and declare it in the library's \
                 parse_data attribute"
            ),
            directive_span,
            diagnostics,
        )));
    }

    if !names_a_json_file(path) {
        return Err(Box::new(refuse(
            argument,
            body_lines,
            DiagnosticCode::NeedImportUnsupportedImportKey,
            format!(
                "{DIRECTIVE}: '{path}' is neither a path to a .json file nor a name the \
                 schema's [import_keys] table declares{}",
                describe_declared_keys(ctx)
            ),
            directive_span,
            diagnostics,
        )));
    }

    Ok(path)
}

/// Whether the argument names something this build would have to fetch.
///
/// Checked by scheme rather than by looking for `://`, so a Windows-style
/// path or a filename holding a colon is still read as a path.
fn is_url(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    lowered.starts_with("http://") || lowered.starts_with("https://")
}

/// Whether the argument names a file this directive could read.
///
/// Reached only for an argument that matched no `[import_keys]` alias, and the
/// discriminator is the `.json` suffix — because what is left to separate out
/// is not a path at all but a *name*, and the likeliest cause is a key the
/// schema forgot to declare. Opening that as a file and reporting it missing
/// would blame the filesystem for a missing declaration, and would fail the
/// build under the `parse_data` contract; naming the real fault costs a
/// warning instead.
///
/// An alias spelled with a `.json` suffix never reaches here: the lookup runs
/// first, exactly as it does in sphinx-needs.
fn names_a_json_file(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".json")
}

/// Reports why the import could not happen and builds the node that says so on
/// the page.
fn refuse(
    argument: &str,
    body_lines: &[&str],
    code: DiagnosticCode,
    message: String,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) -> Node {
    Node::Directive(malformed_directive(
        DIRECTIVE,
        argument,
        body_lines,
        code,
        message,
        span,
        diagnostics,
    ))
}

/// Reads every `:option:` line, refusing sphinx-needs' by name and reporting
/// the rest as unknown.
fn parse_options(
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> ImportOptions {
    let mut options = ImportOptions::default();
    let mut unclaimed: Vec<&OptionLine> = Vec::new();

    for line in option_lines {
        match line.name.as_str() {
            "version" => options.version = Some(line.value.trim().to_string()),
            "ids" => options.ids = Some(split_list(&line.value)),
            "id_prefix" | "id-prefix" => options.id_prefix = line.value.trim().to_string(),
            "tags" => options.tags = split_list(&line.value),
            "filter" => {
                options.filter = read_filter_option(
                    line,
                    None,
                    DIRECTIVE,
                    FilterCodes {
                        invalid: DiagnosticCode::NeedImportInvalidFilter,
                        unknown_field: DiagnosticCode::NeedImportUnknownFilterField,
                    },
                    diagnostics,
                    ctx,
                );
            }
            name => match unsupported_advice(name) {
                Some(advice) => diagnostics.push(Diagnostic::at(
                    DiagnosticCode::NeedImportUnsupportedOption,
                    format!(
                        "{DIRECTIVE}: :{}: is not supported, so it was ignored — {advice}",
                        line.name
                    ),
                    ctx.line_span(line.line_index, &line.raw),
                )),
                None => unclaimed.push(line),
            },
        }
    }

    report_unknown_options(
        &unclaimed,
        DIRECTIVE,
        DiagnosticCode::NeedImportUnknownOption,
        diagnostics,
        ctx,
    );
    options
}

/// What to tell an author who wrote one of sphinx-needs' own options that this
/// build does not implement, or `None` when the name is not one of them.
fn unsupported_advice(name: &str) -> Option<&'static str> {
    UNSUPPORTED_OPTIONS
        .iter()
        .find(|(option, _)| *option == name)
        .map(|(_, advice)| *advice)
}

/// Narrows the version's needs to the ones `:ids:` and `:filter:` select.
///
/// Iterated in the map's own key order, which is what makes an import
/// deterministic: the nodes it produces end up in a `.ast` that Bazel caches
/// on its bytes, so the file's incidental ordering must not reach them.
fn select_needs<'a>(
    version: &'a NeedsVersion,
    options: &ImportOptions,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<(&'a str, &'a RawNeed)> {
    if let Some(ids) = &options.ids {
        for wanted in ids {
            if !version.needs.contains_key(wanted) {
                diagnostics.push(Diagnostic::at(
                    DiagnosticCode::NeedImportUnknownId,
                    format!("{DIRECTIVE}: :ids: names {wanted:?}, which this file does not hold"),
                    span,
                ));
            }
        }
    }

    version
        .needs
        .iter()
        .filter(|(key, _)| {
            options
                .ids
                .as_ref()
                .is_none_or(|ids| ids.iter().any(|wanted| wanted == *key))
        })
        .filter(|(key, need)| match &options.filter {
            None => true,
            Some(filter) => filter.matches(&ImportedNeed {
                key,
                need,
                ctx,
                entity_type: need
                    .type_name
                    .as_deref()
                    .and_then(|name| ctx.schema.entity_type(name)),
            }),
        })
        .map(|(key, need)| (key.as_str(), need))
        .collect()
}

/// Turns one selected need into an entity, or reports why it could not be one.
fn build_entity(
    key: &str,
    need: &RawNeed,
    import: &Import<'_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<EntityBody> {
    let span = import.span;
    let Some(type_name) = need.type_name.as_deref() else {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::NeedImportUnknownType,
            format!(
                "{DIRECTIVE}: {} names no `type`, so it was not imported",
                need_label(import, key)
            ),
            span,
        ));
        return None;
    };
    let Some(entity_type) = ctx.schema.entity_type(type_name) else {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::NeedImportUnknownType,
            format!(
                "{DIRECTIVE}: {} is of type {type_name:?}, which the schema does not \
                 declare; it declares {}",
                need_label(import, key),
                declared_types(ctx)
            ),
            span,
        ));
        return None;
    };

    let id = import_id(key, need, import, diagnostics)?;
    report_id_pattern(entity_type, &id, span, diagnostics);

    let mut attributes = BTreeMap::new();
    let relations = collect_fields(entity_type, key, need, import, &mut attributes, diagnostics);
    apply_tags(entity_type, key, import, &mut attributes, diagnostics);
    apply_defaults(entity_type, &mut attributes);
    report_missing_attributes(entity_type, &attributes, span, diagnostics);
    report_relation_cardinality(entity_type, &relations, span, diagnostics);

    let sections = import_sections(need, entity_type, &id, adornment_order, diagnostics, ctx);

    Some(EntityBody {
        type_name: entity_type.name.clone(),
        id,
        attributes,
        relations,
        sections,
        span,
    })
}

/// How a diagnostic names one need: the file it came from, and its id.
///
/// A project may import several files into one document, and every diagnostic
/// below is reported against the same `.. needimport::` line, so the file is
/// the only thing distinguishing one import's faults from another's.
fn need_label(import: &Import<'_>, key: &str) -> String {
    format!("'{}' need {key:?}", import.file_id)
}

/// The entity id an imported need takes.
///
/// Always explicit — a need without an id is refused rather than given a
/// generated one, because the id is what every relation in the file points at
/// and inventing one would break links this import is meant to carry.
fn import_id(
    key: &str,
    need: &RawNeed,
    import: &Import<'_>,
    diagnostics: &mut Diagnostics,
) -> Option<EntityId> {
    let prefix = &import.options.id_prefix;
    // The map key and the `id` field are the same value in every file
    // sphinx-needs writes; the field wins where they disagree, since that is
    // what the need calls itself.
    let written = need.id.as_deref().unwrap_or(key);
    match EntityId::new(&format!("{prefix}{written}")) {
        Ok(id) => Some(id),
        Err(error) => {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::NeedImportInvalidId,
                format!(
                    "{DIRECTIVE}: {} has no usable id: {error}",
                    need_label(import, key)
                ),
                import.span,
            ));
            None
        }
    }
}

/// Reads every field of a need against its declared type, filling `attributes`
/// and returning the relations among them.
fn collect_fields(
    entity_type: &EntityType,
    key: &str,
    need: &RawNeed,
    import: &Import<'_>,
    attributes: &mut BTreeMap<String, AttributeValue>,
    diagnostics: &mut Diagnostics,
) -> BTreeMap<String, Vec<EntityId>> {
    let span = import.span;
    let mut relations: BTreeMap<String, Vec<EntityId>> = BTreeMap::new();

    for (name, value) in &need.fields {
        if is_internal_field(name) || is_derived_backlink(entity_type, name, import) {
            continue;
        }
        // An explicit `null` is the file saying this field is unset — JSON's
        // own spelling for it, and what an export writes for every `nullable`
        // field a need left blank. Skipped rather than reported, for the
        // reason an empty value is: the author wrote nothing here.
        if field_is_null(value) {
            continue;
        }
        let is_attribute = entity_type.attribute(name).is_some();
        let is_relation = entity_type.relation(name).is_some();
        if !is_attribute && !is_relation {
            // An undeclared field that carries no value is not reported. An
            // export writes *every* registered option for every need — the
            // sphinx-needs demo's four imported needs carry sixteen empty ones
            // apiece, registered by its github and jira services and declared
            // by the project nowhere — so reporting them would bury the one
            // case that matters under noise the author never wrote. A value
            // this build was handed rather than asked for degrades silently;
            // one that says something is still reported below.
            if field_text(value).is_none_or(|text| text.trim().is_empty()) {
                continue;
            }
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::NeedImportUnknownField,
                format!(
                    "{DIRECTIVE}: {} has a field {name:?} that `.. {}::` does not \
                     declare; it accepts {}",
                    need_label(import, key),
                    entity_type.name,
                    describe_options(entity_type)
                ),
                span,
            ));
            continue;
        }

        let Some(text) = field_text(value) else {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::NeedImportInvalidValue,
                format!(
                    "{DIRECTIVE}: {} field {name:?}: {value} has no value an option \
                     could have been written with",
                    need_label(import, key)
                ),
                span,
            ));
            continue;
        };

        if let Some(schema) = entity_type.attribute(name) {
            // An empty JSON value is an unset field, not a malformed one: a
            // needs.json spells "no owner" as `""`, and reporting that as an
            // invalid value would fire on every need in a real file.
            //
            // No exception for `bool`, deliberately. A bare `:deprecated:`
            // meaning *true* is a property of RST option syntax, not of JSON:
            // an export writes `true`/`false` for a flag it holds and `""`
            // only for one it does not, so reading the empty case as true
            // would invent a value the file never stated.
            if text.trim().is_empty() {
                continue;
            }
            store_attribute(schema, name, &text, span, attributes, diagnostics);
        } else {
            let written =
                prefix_internal_targets(&text, &import.options.id_prefix, &import.imported);
            relations.insert(
                name.clone(),
                collect_relation_targets(&written, span, diagnostics),
            );
        }
    }

    relations
}

/// Whether `name` is a back-link this build *derives* for `entity_type`.
///
/// sphinx-needs stores both directions of every link — `links` and its
/// generated `links_back` — where this model declares only the outgoing side
/// and derives the incoming one project-wide (ADR-009). Ingesting the stored
/// copy would duplicate something already computed, and from a file that may
/// disagree with the graph actually being built.
///
/// Asked of the schema rather than matched as a `_back` suffix, so this is
/// exact: it skips precisely the names this project derives, and a field that
/// merely ends in `_back` is still reported.
fn is_derived_backlink(entity_type: &EntityType, name: &str, import: &Import<'_>) -> bool {
    import
        .schema
        .backlinks_for(&entity_type.name)
        .iter()
        .any(|backlink| backlink.name == name)
}

/// Adds the `:tags:` values to the entity's own `tags` attribute.
///
/// Nothing in this model privileges the name `tags`, so this only works for a
/// type that declares a list attribute called that — and says so when it does
/// not, rather than dropping the values the author asked for.
fn apply_tags(
    entity_type: &EntityType,
    key: &str,
    import: &Import<'_>,
    attributes: &mut BTreeMap<String, AttributeValue>,
    diagnostics: &mut Diagnostics,
) {
    let (tags, span) = (&import.options.tags, import.span);
    if tags.is_empty() {
        return;
    }
    let declares_tag_list = entity_type
        .attribute("tags")
        .is_some_and(|schema| schema.value_type.is_list());
    if !declares_tag_list {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::NeedImportNoTagsAttribute,
            format!(
                "{DIRECTIVE}: :tags: was written, but `.. {}::` ({}) declares no list \
                 attribute `tags` to add them to",
                entity_type.name,
                need_label(import, key)
            ),
            span,
        ));
        return;
    }

    let mut items = match attributes.remove("tags") {
        Some(AttributeValue::List(items)) => items,
        Some(other) => other.items(),
        None => Vec::new(),
    };
    for tag in tags {
        if !items.contains(tag) {
            items.push(tag.clone());
        }
    }
    // Re-validated through the same funnel a written value goes through, so an
    // `list<enum>` cannot gain a tag it does not permit.
    if let Some(schema) = entity_type.attribute("tags") {
        store_attribute(
            schema,
            "tags",
            &items.join(", "),
            span,
            attributes,
            diagnostics,
        );
    }
}

/// Rewrites the relation targets that point at another need in *this* import.
///
/// Only those. A target naming something outside the file is a reference to an
/// entity the project already holds under its own id, and prefixing it would
/// break a link that works. Scoping it this way is what lets the same file be
/// imported twice under two prefixes without the two copies linking into each
/// other.
///
/// Applied to the written text rather than to the built ids, so the one place
/// an id becomes an [`EntityId`] stays [`collect_relation_targets`] — and a
/// prefix that makes an id illegal is reported there, once.
fn prefix_internal_targets(text: &str, prefix: &str, imported: &BTreeSet<&str>) -> String {
    if prefix.is_empty() {
        return text.to_string();
    }
    split_list(text)
        .into_iter()
        .map(|target| {
            if imported.contains(target.as_str()) {
                format!("{prefix}{target}")
            } else {
                target
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parses a need's `content` into the entity's unnamed content section.
///
/// Named sections are not importable: the format has no way to express one, so
/// an imported entity has exactly the body its `content` holds. The text runs
/// under [`ParseCtx::synthetic`], which is what makes a diagnostic from inside
/// it positionless rather than pointed at a line of the document that has
/// nothing to do with it.
fn import_sections(
    need: &RawNeed,
    entity_type: &EntityType,
    id: &EntityId,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<EntitySection> {
    let Some(content) = need.content.as_deref() else {
        return Vec::new();
    };
    if content.trim().is_empty() {
        return Vec::new();
    }
    // A need's content takes no section titles, as sphinx-needs parses it
    // with `match_titles=False` — whatever surrounds the `.. needimport::`.
    let body_ctx = ctx
        .synthetic()
        .without_section_titles()
        .inside_entity(entity_type)
        .inside_entity_id(id);
    let lines: Vec<&str> = content.lines().collect();
    let nodes = parse_blocks(&lines, adornment_order, diagnostics, &body_ctx);
    if nodes.is_empty() {
        return Vec::new();
    }
    vec![EntitySection::content(nodes)]
}

/// The aliases the schema's `[import_keys]` table declares, for the refusal
/// above — or a pointer at the table when it declares none.
///
/// Offering the vocabulary rather than only naming the fault, the way an
/// unknown entity option already lists what the type accepts.
fn describe_declared_keys(ctx: &ParseCtx<'_>) -> String {
    if ctx.import_keys.is_empty() {
        return "; write the path to the needs.json, or declare the name in [import_keys]"
            .to_string();
    }
    let names: Vec<&str> = ctx.import_keys.keys().map(String::as_str).collect();
    format!("; it declares {}", names.join(", "))
}

/// The entity types the schema declares, for an unknown-type diagnostic.
fn declared_types(ctx: &ParseCtx<'_>) -> String {
    let names: Vec<&str> = ctx
        .schema
        .types()
        .iter()
        .map(|entity_type| entity_type.name.as_str())
        .collect();
    if names.is_empty() {
        "no entity types at all".to_string()
    } else {
        names.join(", ")
    }
}

/// One need from the file, as a `:filter:` sees it.
///
/// The values it answers with come from the *declared* attribute type wherever
/// there is one, by running the JSON through the same
/// [`parse_attribute_value`] funnel the entity itself will go through. That is
/// what stops `"api" in tags` from depending on whether the file spelled the
/// tags as an array or as a comma-separated string.
struct ImportedNeed<'a> {
    key: &'a str,
    need: &'a RawNeed,
    entity_type: Option<&'a EntityType>,
    ctx: &'a ParseCtx<'a>,
}

impl FilterSubject for ImportedNeed<'_> {
    fn field(&self, name: &FieldName) -> FieldValue {
        match name.as_str() {
            "id" => FieldValue::Text(self.need.id.as_deref().unwrap_or(self.key).to_string()),
            "type" => self
                .need
                .type_name
                .as_deref()
                .map_or(FieldValue::Missing, |name| {
                    FieldValue::Text(name.to_string())
                }),
            "type_name" => self.entity_type.map_or(FieldValue::Missing, |entity_type| {
                FieldValue::Text(entity_type.display_label().to_string())
            }),
            // The importing document, which is where the entity will live.
            "docname" => FieldValue::Text(self.ctx.doc_path.to_string()),
            other => self.declared_field(other),
        }
    }
}

impl ImportedNeed<'_> {
    /// What a non-built-in field is worth, read through the declared type.
    fn declared_field(&self, name: &str) -> FieldValue {
        let Some(value) = self.need.fields.get(name) else {
            return FieldValue::Missing;
        };
        let Some(text) = field_text(value) else {
            return FieldValue::Missing;
        };
        let declared = self
            .entity_type
            .and_then(|entity_type| entity_type.attribute(name));
        match declared {
            Some(schema) => parse_attribute_value(&schema.value_type, &text)
                .as_ref()
                .map_or(FieldValue::Missing, FieldValue::from),
            // A relation, or a field the type does not declare: the text as
            // written, split when the file spelled a list.
            None if field_is_list(value) => FieldValue::List(split_list(&text)),
            None => FieldValue::Text(text),
        }
    }
}

#[cfg(test)]
mod tests;
