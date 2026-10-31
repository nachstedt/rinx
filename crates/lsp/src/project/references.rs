//! Where a project's documents use each of its definitions: the reverse of
//! going to a definition, what "find all references" lists.
//!
//! A use's target is a question of the whole project — a label two documents
//! define leads nowhere, a domain role searches every scope — so a document's
//! analysis records only what it *wrote* ([`DocumentUses`]), and each is
//! resolved against the project's current fold through
//! `rinx_renderer::UseResolver`, as rendering the page would resolve it. A
//! use the page would draw broken is no use of anything.
//!
//! Resolving every use of a project takes a while, so the project caches
//! what each document's uses resolved to until its fold changes, or that
//! document's analysis does. Like the definition sites, none of this is ever
//! part of the project index (`docs/decisions/045-definition-spans.md`).

use std::path::{Path, PathBuf};

use rinx_ast::{
    Directive, Document, EntityId, FieldMutationMode, Node, Position, SourceMap, Span, TargetName,
    walk_nodes,
};
use rinx_entity::EntitySchema;
use rinx_index::{DefinitionKey, ProjectIndex};
use rinx_renderer::config::SiteConfig;
use rinx_renderer::{Destination, ReferenceTarget, UseResolver, WrittenUse, written_uses};

use crate::diagnostics::{RST_EPILOG, RST_PROLOG};

/// What one document wrote that uses a definition of the project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentUses {
    /// Its cross-references and `.. toctree::` entries.
    written: Vec<WrittenUse>,
    /// The entity ids its relation options name.
    relations: Vec<RelationUse>,
    /// The ids `.. entity-update::` arguments name, each with exactly its
    /// characters.
    updated: Vec<(EntityId, Span)>,
    /// The files its spans name by [`rinx_ast::FileId`], as the parse
    /// recorded them.
    files: Vec<String>,
}

/// The ids one relation option line names: an entity's own, or one an
/// `.. entity-update::` changes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RelationUse {
    /// The option's name, its `+`/`-` stripped.
    field: String,
    ids: Vec<EntityId>,
    /// The option's line.
    span: Span,
    /// Whether an `.. entity-update::` wrote it, which names a field the
    /// schema may not declare a relation.
    update: bool,
}

impl DocumentUses {
    /// What `document` wrote, its names' characters found in the
    /// `source_map` its parse left beside it.
    #[must_use]
    pub fn of(document: &Document, source_map: &SourceMap) -> Self {
        Self {
            written: written_uses(document),
            relations: relation_uses(document),
            updated: updated_ids(document, source_map),
            files: document.source_files.clone(),
        }
    }
}

/// Every relation option line written in `document` with a span.
fn relation_uses(document: &Document) -> Vec<RelationUse> {
    let mut uses = Vec::new();
    walk_nodes(&document.nodes, &mut |node| match node {
        Node::Directive(Directive::Entity(entity)) => {
            uses.extend(entity.relations.iter().filter_map(|(field, ids)| {
                Some(RelationUse {
                    field: field.clone(),
                    ids: ids.clone(),
                    span: *entity.relation_spans.get(field)?,
                    update: false,
                })
            }));
        }
        Node::Directive(Directive::EntityUpdate(update)) => {
            uses.extend(update.fields.iter().filter_map(|mutation| {
                let (FieldMutationMode::Set(value)
                | FieldMutationMode::Append(value)
                | FieldMutationMode::Remove(value)) = &mutation.mode
                else {
                    return None;
                };
                Some(RelationUse {
                    field: mutation.field.clone(),
                    ids: value
                        .split(',')
                        .filter_map(|id| EntityId::new(id.trim()).ok())
                        .collect(),
                    span: mutation.span?,
                    update: true,
                })
            }));
        }
        _ => {}
    });
    uses
}

/// Every `.. entity-update::` argument written in `document` that reads as
/// an id, where `source_map` knows its characters.
fn updated_ids(document: &Document, source_map: &SourceMap) -> Vec<(EntityId, Span)> {
    let mut updated = Vec::new();
    walk_nodes(&document.nodes, &mut |node| {
        if let Node::Directive(Directive::EntityUpdate(update)) = node
            && let Some(id) = &update.target.candidate_id
            && let Some(span) = id.id().and_then(|name| source_map.name_span(name))
        {
            updated.push((id.value().clone(), span));
        }
    });
    updated
}

/// What a use leads to: a definition of the document `doc_path`, or the
/// document itself.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UseTarget {
    pub doc_path: String,
    /// `None` for the document itself, as a `:doc:` or a toctree entry
    /// names it.
    pub definition: Option<DefinitionKey>,
}

impl UseTarget {
    /// What `target` is, when it is in a document of this project — `None`
    /// for another site's page or a page the build writes.
    #[must_use]
    pub fn of(target: &ReferenceTarget) -> Option<Self> {
        let Destination::Document { doc_path, .. } = &target.destination else {
            return None;
        };
        // A label that is an entity's names the entity, as every other
        // reference to it does: one definition, whichever way it is named.
        let definition = match &target.entity {
            Some(id) => Some(DefinitionKey::Entity(id.clone())),
            None => target.definition.clone(),
        };
        Some(Self {
            doc_path: doc_path.clone(),
            definition,
        })
    }
}

/// Where one use was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseSite {
    /// The file: the using document's, or an `.. include::`d fragment's —
    /// `None` for a `conf.py` project's prolog or epilog, which no file holds.
    pub path: Option<PathBuf>,
    /// The use's first character.
    pub start: Position,
    /// Just past its last character.
    pub end: Position,
    /// How it was written, which says where in it the name stands.
    pub form: UseForm,
}

/// How a use was written, which says where within it the name it uses
/// stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseForm {
    /// A role, `` :ref:`Title <install>` ``: the target inside its
    /// backquotes, after any explicit title.
    Role,
    /// A `.. toctree::` entry, which names a document.
    TocEntry,
    /// A named reference written as the name it holds, which reaches the
    /// definition directly when that is the definition's name, and through
    /// the aliases its page writes otherwise.
    Named(TargetName),
    /// One id among others on a relation option's line, which the span
    /// holds only the line of.
    RelationId(EntityId),
    /// Exactly the name: an `.. entity-update::`'s argument.
    Name,
}

impl UseForm {
    /// How `written` was written.
    fn of(written: &WrittenUse) -> Self {
        match written {
            WrittenUse::Reference { .. } => Self::Role,
            WrittenUse::TocEntry { .. } => Self::TocEntry,
            WrittenUse::NamedReference { chain, .. } => Self::Named(chain.first().clone()),
        }
    }
}

/// Everything one project's documents are resolved against.
pub(super) struct Resolution<'a> {
    pub index: &'a ProjectIndex,
    pub config: &'a SiteConfig,
    pub schema: &'a EntitySchema,
    pub source_root: &'a Path,
}

/// What each use the document `doc_path`, the file at `path`, wrote leads
/// to, against `resolution` — every target of a `:glob:` entry, and none of
/// a broken reference.
pub(super) fn resolve_uses(
    doc_path: &str,
    path: &Path,
    uses: &DocumentUses,
    resolution: &Resolution<'_>,
) -> Vec<(UseTarget, UseSite)> {
    let resolver = UseResolver::new(
        doc_path,
        resolution.index,
        resolution.config,
        resolution.schema,
    );
    let site_of = |span: Span, form: UseForm| {
        let path = match span_path(span, path, uses, resolution.source_root) {
            SpanFile::File(path) => Some(path),
            SpanFile::Setting => None,
            SpanFile::Unlisted => return None,
        };
        Some(UseSite {
            path,
            start: span.start,
            end: span.end,
            form,
        })
    };
    let mut found: Vec<(UseTarget, UseSite)> = uses
        .written
        .iter()
        .flat_map(|written| {
            resolver
                .targets(written)
                .iter()
                .filter_map(UseTarget::of)
                .filter_map(|target| Some((target, site_of(written.span(), UseForm::of(written))?)))
                .collect::<Vec<_>>()
        })
        .collect();
    for relation in &uses.relations {
        if relation.update && resolution.schema.relation(&relation.field).is_none() {
            continue;
        }
        for id in &relation.ids {
            let Some(record) = resolution.index.entities.get(id) else {
                continue;
            };
            let target = UseTarget {
                doc_path: record.doc_path.clone(),
                definition: Some(DefinitionKey::Entity(id.clone())),
            };
            if let Some(site) = site_of(relation.span, UseForm::RelationId(id.clone())) {
                found.push((target, site));
            }
        }
    }
    // An argument naming an existing entity updates that entity, whatever
    // its filter reading would match — as the build applies it.
    for (id, span) in &uses.updated {
        let Some(record) = resolution.index.entities.get(id) else {
            continue;
        };
        let target = UseTarget {
            doc_path: record.doc_path.clone(),
            definition: Some(DefinitionKey::Entity(id.clone())),
        };
        if let Some(site) = site_of(*span, UseForm::Name) {
            found.push((target, site));
        }
    }
    found
}

/// Which file a span counts its lines in.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SpanFile {
    /// The document's own file, or an `.. include::`d fragment's.
    File(PathBuf),
    /// A `conf.py` project's prolog or epilog, which no file holds.
    Setting,
    /// A file id the document does not list.
    Unlisted,
}

/// The file `span` counts lines in: `path`, the document's own, or the
/// fragment its file id names.
fn span_path(span: Span, path: &Path, uses: &DocumentUses, source_root: &Path) -> SpanFile {
    let Some(file) = span.file else {
        return SpanFile::File(path.to_path_buf());
    };
    match uses.files.get(file.index()) {
        None => SpanFile::Unlisted,
        Some(name) if name == RST_PROLOG || name == RST_EPILOG => SpanFile::Setting,
        // The server's loader names a fragment by its absolute path, which
        // joining keeps as it is.
        Some(name) => SpanFile::File(source_root.join(name)),
    }
}

/// The characters `id` takes on the relation option `line` — `:links: A, B`,
/// `:+links: B` — as a range of character indices, or `None` when the line
/// lists no such id.
#[must_use]
pub fn relation_id_columns(line: &str, id: &EntityId) -> Option<std::ops::Range<usize>> {
    let chars: Vec<char> = line.chars().collect();
    let marker = chars.iter().position(|&c| c == ':')?;
    let value_start = marker + 1 + chars[marker + 1..].iter().position(|&c| c == ':')? + 1;
    let mut start = value_start;
    for item in chars[value_start..].split(|&c| c == ',') {
        let leading = item.iter().take_while(|c| c.is_whitespace()).count();
        let trailing = item.iter().rev().take_while(|c| c.is_whitespace()).count();
        let trimmed = &item[leading..item.len().saturating_sub(trailing).max(leading)];
        if trimmed.iter().copied().eq(id.as_str().chars()) {
            return Some(start + leading..start + leading + trimmed.len());
        }
        start += item.len() + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{FileId, TargetName};

    fn id(text: &str) -> EntityId {
        EntityId::new(text).unwrap()
    }

    fn label(doc_path: &str, name: &str) -> UseTarget {
        UseTarget {
            doc_path: doc_path.to_string(),
            definition: Some(DefinitionKey::Label(TargetName::new(name))),
        }
    }

    /// An index holding the label `install` in `setup.rst`.
    fn index() -> ProjectIndex {
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("install"), "setup.rst".to_string());
        index
    }

    /// What `document` uses, resolved against `index` as the file
    /// `/project/index.rst`.
    fn resolved(document: &Document, index: &ProjectIndex) -> Vec<(UseTarget, UseSite)> {
        let config = SiteConfig::default();
        let resolution = Resolution {
            index,
            config: &config,
            schema: EntitySchema::empty_ref(),
            source_root: Path::new("/project"),
        };
        resolve_uses(
            "index.rst",
            Path::new("/project/index.rst"),
            &DocumentUses::of(document, &SourceMap::default()),
            &resolution,
        )
    }

    #[test]
    fn test_resolve_uses_leads_each_reference_to_its_definition() {
        // Given
        let document = rinx_parser::parse(
            "index.rst",
            "See :ref:`install`.\n",
            &rinx_parser::ParseCtx::default(),
        )
        .document;

        // When
        let uses = resolved(&document, &index());

        // Then
        assert_eq!(
            uses,
            vec![(
                label("setup.rst", "install"),
                UseSite {
                    path: Some(PathBuf::from("/project/index.rst")),
                    start: Position::new(1, 5),
                    end: Position::new(1, 19),
                    form: UseForm::Role,
                }
            )]
        );
    }

    #[test]
    fn test_resolve_uses_drops_a_broken_reference() {
        // Given
        let document = rinx_parser::parse(
            "index.rst",
            "See :ref:`ghost`.\n",
            &rinx_parser::ParseCtx::default(),
        )
        .document;

        // When / Then
        assert!(resolved(&document, &index()).is_empty());
    }

    #[test]
    fn test_resolve_uses_leads_a_toctree_entry_to_the_document_itself() {
        // Given
        let mut index = index();
        index
            .document_titles
            .insert("setup.rst".to_string(), "Setup".to_string());
        let document = rinx_parser::parse(
            "index.rst",
            ".. toctree::\n\n   setup\n",
            &rinx_parser::ParseCtx::default(),
        )
        .document;

        // When
        let uses = resolved(&document, &index);

        // Then
        assert_eq!(
            uses.iter().map(|(target, _)| target).collect::<Vec<_>>(),
            vec![&UseTarget {
                doc_path: "setup.rst".to_string(),
                definition: None,
            }]
        );
    }

    #[test]
    fn test_resolve_uses_places_a_use_in_the_fragment_it_was_written_in() {
        // Given — the reference's span counts lines in `part.rst`
        let mut document = rinx_parser::parse(
            "index.rst",
            "See :ref:`install`.\n",
            &rinx_parser::ParseCtx::default(),
        )
        .document;
        let file = document.intern_source_file("/project/part.rst".to_string());
        let Node::Paragraph(inlines) = &mut document.nodes[0] else {
            unreachable!()
        };
        let span = inlines[1].span().unwrap();
        inlines[1] = inlines[1]
            .clone()
            .with_span(Some(span.with_file(Some(file))));

        // When
        let uses = resolved(&document, &index());

        // Then
        assert_eq!(uses[0].1.path, Some(PathBuf::from("/project/part.rst")));
    }

    #[test]
    fn test_span_path_is_a_setting_for_a_prolog_no_file_holds() {
        // Given
        let uses = DocumentUses {
            files: vec![RST_PROLOG.to_string()],
            ..DocumentUses::default()
        };
        let span =
            Span::new(Position::new(1, 1), Position::new(1, 5)).with_file(Some(FileId::new(0)));

        // When / Then
        assert_eq!(
            span_path(
                span,
                Path::new("/project/a.rst"),
                &uses,
                Path::new("/project")
            ),
            SpanFile::Setting
        );
    }

    #[test]
    fn test_relation_uses_records_each_entity_s_relation_lines() {
        // Given
        let mut entity = rinx_ast::EntityBody {
            type_name: "req".to_string(),
            id: id("REQ_2").into(),
            attributes: std::collections::BTreeMap::new(),
            relations: std::collections::BTreeMap::from([("links".to_string(), vec![id("REQ_1")])]),
            sections: Vec::new(),
            span: None,
            relation_spans: std::collections::BTreeMap::new(),
        };
        entity.relation_spans.insert(
            "links".to_string(),
            Span::new(Position::new(3, 4), Position::new(3, 18)),
        );
        let document = Document::new(
            "index.rst".to_string(),
            vec![Node::Directive(Directive::Entity(Box::new(entity)))],
        );

        // When
        let uses = relation_uses(&document);

        // Then
        assert_eq!(
            uses,
            vec![RelationUse {
                field: "links".to_string(),
                ids: vec![id("REQ_1")],
                span: Span::new(Position::new(3, 4), Position::new(3, 18)),
                update: false,
            }]
        );
    }

    #[test]
    fn test_updated_ids_are_the_placed_id_arguments_of_updates() {
        // Given — an id argument placed by the parser, one built unplaced,
        // and a filter, which names no id
        let span = Span::new(Position::new(1, 20), Position::new(1, 23));
        let mut map = SourceMap::default();
        let written = map.record_name(span);
        let update = |target: rinx_ast::UpdateTarget| {
            Node::Directive(Directive::EntityUpdate(Box::new(
                rinx_ast::EntityUpdate::new(rinx_ast::EntityUpdateSource::EntityUpdate, target),
            )))
        };
        let read = |raw: &str| rinx_ast::UpdateTarget::read(raw).expect("a target");
        let document = Document::new(
            "index.rst".to_string(),
            vec![
                update(read("S_1").named_as(Some(written))),
                update(read("S_2")),
                update(read("status == 'open'").named_as(Some(written))),
            ],
        );

        // When / Then
        assert_eq!(updated_ids(&document, &map), vec![(id("S_1"), span)]);
    }

    #[test]
    fn test_relation_uses_reads_the_ids_an_update_sets_appends_or_removes() {
        // Given — `+links: A_1, B_2` and `-tags:`, which names no id
        let line = |n| Span::new(Position::new(n, 4), Position::new(n, 20));
        let mut update = rinx_ast::EntityUpdate::new(
            rinx_ast::EntityUpdateSource::EntityUpdate,
            rinx_ast::UpdateTarget {
                candidate_id: Some(id("S_1").into()),
                filter: None,
                raw: "S_1".to_string(),
            },
        );
        update.fields = vec![
            rinx_ast::FieldMutation {
                field: "links".to_string(),
                mode: FieldMutationMode::Append("A_1, B_2".to_string()),
                span: Some(line(2)),
            },
            rinx_ast::FieldMutation {
                field: "tags".to_string(),
                mode: FieldMutationMode::Clear,
                span: Some(line(3)),
            },
        ];
        let document = Document::new(
            "index.rst".to_string(),
            vec![Node::Directive(Directive::EntityUpdate(Box::new(update)))],
        );

        // When
        let uses = relation_uses(&document);

        // Then
        assert_eq!(
            uses,
            vec![RelationUse {
                field: "links".to_string(),
                ids: vec![id("A_1"), id("B_2")],
                span: line(2),
                update: true,
            }]
        );
    }

    #[test]
    fn test_relation_id_columns_finds_the_id_among_the_listed() {
        // When / Then
        assert_eq!(
            relation_id_columns("   :links: A_1, B_2", &id("B_2")),
            Some(16..19)
        );
        assert_eq!(
            relation_id_columns("   :links: A_1, B_2", &id("A_1")),
            Some(11..14)
        );
        assert_eq!(
            relation_id_columns("   :+links: B_2", &id("B_2")),
            Some(12..15)
        );
        assert_eq!(relation_id_columns("   :links: A_10", &id("A_1")), None);
        assert_eq!(relation_id_columns("no option here", &id("A_1")), None);
    }

    #[test]
    fn test_use_target_of_is_none_for_a_target_in_no_document() {
        // Given
        let external = ReferenceTarget {
            title: "dict".to_string(),
            destination: Destination::External {
                url: "https://example.org/dict".to_string(),
                site: "(in Python)".to_string(),
            },
            entity: None,
            definition: None,
        };

        // When / Then
        assert_eq!(UseTarget::of(&external), None);
    }
}
