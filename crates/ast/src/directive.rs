use serde::{Deserialize, Serialize};

use crate::admonition_kind::AdmonitionKind;
use crate::button_link::ButtonLink;
use crate::code_block::CodeBlock;
use crate::code_language::ResolvedLanguage;
use crate::contents::Contents;
use crate::doctest::DocTestBlock;
use crate::domain_object_body::DomainObjectBody;
use crate::dropdown::Dropdown;
use crate::entity::EntityBody;
use crate::entity_bar::EntityBar;
use crate::entity_flow::EntityFlow;
use crate::entity_pie::EntityPie;
use crate::entity_sequence::EntitySequence;
use crate::entity_table::EntityTable;
use crate::entity_update::EntityUpdate;
use crate::glossary_entry::GlossaryEntry;
use crate::grid::{Grid, GridItem};
use crate::image::{Figure, ImageOptions};
use crate::index_entry::IndexEntry;
use crate::node::Node;
use crate::sectnum::SectnumOptions;
use crate::span::Span;
use crate::substitution::SubstitutionDefinition;
use crate::table::TableAlign;
use crate::table::TableRow;
use crate::table::TableSource;
use crate::table::TableWidths;
use crate::target_name::TargetName;
use crate::toctree::Toctree;
use crate::uml::Uml;
use crate::version_change_kind::VersionChangeKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Directive {
    Toctree(Toctree),
    Contents(Contents),
    /// `.. sectnum::` / `.. section-numbering::` — numbers every section in
    /// the document it's written in, wherever it's written, unless an
    /// ancestor `:numbered:` toctree already numbers that document (see
    /// `rinx_analyzer::section_numbering`, which resolves that
    /// precedence). Produces no output of its own, like [`Self::Highlight`]:
    /// its effect is entirely in the numbers looked up while rendering
    /// headings.
    Sectnum(SectnumOptions),
    /// An instance of a project-declared entity type — `.. req::`,
    /// `.. audit-event::`, whatever the schema names.
    ///
    /// Boxed because [`EntityBody`] is by far the largest payload here, and an
    /// enum costs its largest variant everywhere one is stored.
    ///
    /// The type is carried as a name rather than as a variant, which is the
    /// one place this model deliberately departs from how the built-in py/c/std
    /// domain objects are built: a type declared in a config file cannot be a
    /// Rust enum. Everything in the body was already validated against the
    /// schema while parsing, so no later phase re-checks it.
    Entity(Box<EntityBody>),
    /// A named prose section *inside* an entity — `.. verification-criteria::`.
    ///
    /// Transient: the entity directive's parser collects these out of its
    /// parsed body and folds them into [`EntityBody::sections`], so a
    /// well-formed document never keeps one. One that survives to a later
    /// phase was written outside any entity, and is diagnosed rather than
    /// rendered.
    EntitySection {
        name: String,
        body: Vec<Node>,
        span: Option<Span>,
    },
    /// A `PlantUML` diagram — every spelling of one: `.. plantuml::`/`.. uml::`,
    /// `.. entity-diagram::`/`.. needuml::` and
    /// `.. entity-arch::`/`.. needarch::`.
    ///
    /// Boxed like [`Self::Dropdown`] and [`Self::EntityTable`], for the same
    /// reason: a template, an option set and a span make it a large payload,
    /// and an enum costs its largest variant everywhere one is stored.
    ///
    /// Carries the diagram's *template*, not its picture: the templated
    /// spellings ask the entity graph questions that only the project index
    /// can answer, so the text that finally reaches `PlantUML` — and the hash
    /// naming the SVG it compiles to — is produced after indexing. See
    /// [`Uml`] for why one variant serves all three constructs.
    Uml(Box<Uml>),
    Admonition {
        kind: AdmonitionKind,
        title: Option<String>,
        collapsible: Option<bool>,
        body: Vec<Node>,
    },
    /// `.. dropdown::` — sphinx-design's collapsible container.
    ///
    /// Boxed for the same reason [`Self::Entity`] is: ten options, an
    /// inline-markup title and a body make it one of the largest payloads
    /// here, and an enum costs its largest variant everywhere one is stored.
    ///
    /// The one variant here that is neither docutils' nor Sphinx's. It earns
    /// its place because an *unknown* directive never parses its body, so a
    /// missing `dropdown` silently swallows every construct written inside
    /// one — targets, sections and entities alike.
    Dropdown(Box<Dropdown>),
    /// `.. grid::` — sphinx-design's responsive row of cells.
    ///
    /// Boxed like [`Self::Dropdown`], for the same reason, and here for the
    /// same reason too: the demo corpus this build benchmarks against draws a
    /// `PlantUML` diagram inside a `.. grid-item::`, and an unknown `grid`
    /// swallowed it whole.
    Grid(Box<Grid>),
    /// `.. grid-item::` — one cell of a [`Self::Grid`].
    ///
    /// Its own variant rather than a field of the grid: sphinx-design only
    /// *warns* when an item is written outside a grid, or when a grid holds
    /// something that is not one, and still renders both. Nesting the type
    /// would turn those warnings into content loss.
    GridItem(Box<GridItem>),
    /// `.. button-link::` — sphinx-design's button-shaped external link.
    ///
    /// Boxed like [`Self::Dropdown`], for the same reason: nine options, an
    /// inline-markup label and a target make it a large payload beside the
    /// enum's small variants.
    ///
    /// The one sphinx-design variant here with no block body at all — its
    /// content is a *label*, parsed as inline markup — so unlike its three
    /// neighbours it needs no arm in the traversals that walk block content.
    ButtonLink(Box<ButtonLink>),
    /// `.. entity-table::`, and its sphinx-needs spelling `.. needtable::` —
    /// a table of the entities matching a filter.
    ///
    /// Boxed like [`Self::Dropdown`], for the same reason: a filter
    /// expression, a column list and the shared table options make it a large
    /// payload, and an enum costs its largest variant everywhere one is
    /// stored.
    ///
    /// The one directive here whose content comes from *other documents*. It
    /// carries the question rather than the rows, because the entities it
    /// lists are declared in documents this one never mentions — only the
    /// project index knows them, so the rows are resolved while rendering.
    EntityTable(Box<EntityTable>),
    /// `.. entity-flow::`, and its sphinx-needs spelling `.. needflow::` — a
    /// picture of the entities matching a filter and the relations between
    /// them.
    ///
    /// Boxed like [`Self::EntityTable`], for the same reason, and carrying a
    /// question for the same reason too. What separates the two is only how
    /// the answer is presented: a table renders rows, a flowchart generates
    /// `PlantUML` that a build action compiles — which is why this is not a
    /// [`Self::Uml`] with a template. There is no template here; nothing the
    /// author wrote reaches `PlantUML` at all.
    EntityFlow(Box<EntityFlow>),
    /// `.. entity-sequence::`, and its sphinx-needs spelling
    /// `.. needsequence::` — a sequence diagram walked from start entities
    /// along the relations that carry messages.
    ///
    /// Boxed like [`Self::EntityFlow`] and carrying a question for its reason:
    /// the participants and messages live in other documents, so the
    /// `PlantUML` is generated while rendering and compiled like any diagram.
    EntitySequence(Box<EntitySequence>),
    /// `.. entity-pie::`, and its sphinx-needs spelling `.. needpie::` — a pie
    /// chart of how many entities each of several filters selects.
    ///
    /// Boxed like [`Self::EntityFlow`], for the same reason, and carrying a
    /// question for the same reason too. It is the third presentation of the
    /// one question [`Self::EntityTable`] asks: rows, a graph, or proportions.
    ///
    /// Unlike the other two pictures this build draws, nothing here is
    /// compiled — the chart is SVG produced by the render action itself, so a
    /// library holding one needs no `diagrams = True`.
    EntityPie(Box<EntityPie>),
    /// `.. entity-bar::`, and its sphinx-needs spelling `.. needbar::` — a bar
    /// chart of how many entities each cell of a grid of filters selects.
    ///
    /// The fourth presentation of [`Self::EntityTable`]'s question, and
    /// [`Self::EntityPie`]'s sibling in everything but shape: its body is a
    /// grid rather than a list, and it is drawn along an axis. Boxed for the
    /// reason every entity view is, and never compiled for the pie's reason.
    EntityBar(Box<EntityBar>),
    /// `.. entity-update::`, and its sphinx-needs spelling `.. needextend::` —
    /// a project-wide mutation of one or many entities' fields.
    ///
    /// Boxed like [`Self::EntityTable`], for the same reason: a target with
    /// two possible readings, a field-mutation list and a justification body
    /// make it a large payload.
    ///
    /// Unlike its three siblings above, this directive produces no
    /// presentation of its own by default — its effect is entirely in what
    /// `rinx_analyzer::apply_entity_updates` writes into a derived,
    /// non-destructive history beside `ProjectIndex::entities` before
    /// anything renders (see `docs/decisions/019-entity-update.md`). Its
    /// `body` is ordinary block content, exactly as [`Self::Dropdown`]'s is,
    /// so it needs the same traversal arms that container does.
    EntityUpdate(Box<EntityUpdate>),
    VersionChange {
        kind: VersionChangeKind,
        version: String,
        body: Vec<Node>,
    },
    SeeAlso {
        body: Vec<Node>,
    },
    Glossary {
        entries: Vec<GlossaryEntry>,
        sorted: bool,
    },
    /// A `.. index::` directive. `id` is the anchor the genindex page links
    /// back to — assigned by a post-parse pass (unique within this document
    /// only, see `rinx_parser`'s `assign_index_ids`), not at
    /// construction time, since there's no content-derived identity for a
    /// directive that marks a bare location.
    Index {
        entries: Vec<IndexEntry>,
        id: String,
    },
    /// A table written as *data* plus options rather than as character-art:
    /// `.. list-table::` (rows as a nested bullet list) and `.. csv-table::`
    /// (rows as CSV). Both spell their rows out differently in source, but
    /// that difference is fully consumed while parsing — what survives into
    /// the AST is identical, so they share one variant and record which
    /// directive they came from in `source`.
    ///
    /// Reuses [`TableRow`]/[`TableCell`] from the grid-table implementation
    /// for its rows (`colspan`/`rowspan` always 1, since neither directive
    /// has a span syntax), but gets its own variant rather than folding into
    /// `Node::Table` because grid tables have none of these options and would
    /// otherwise carry meaningless defaults forever.
    DataTable {
        /// Which directive produced this table — drives the rendered CSS
        /// class and nothing else about the table's structure.
        source: TableSource,
        /// The directive argument — the table's title/caption. `None` when
        /// no argument was given.
        title: Option<String>,
        /// `:header-rows:` — how many leading rows in `rows` are header
        /// rows. 0 (the spec default) when the option is omitted; never
        /// exceeds `rows.len()` (list-table clamps it, csv-table rejects an
        /// oversized value outright, as docutils does).
        header_rows: usize,
        /// `:stub-columns:` — how many leading columns in every row are
        /// stub (row-header) columns. 0 by default; bounded against the
        /// actual column count at parse time the same way.
        stub_columns: usize,
        widths: Option<TableWidths>,
        /// `:width:` — an opaque CSS length/percentage (e.g. `"100%"`),
        /// passed through verbatim since it's only ever re-emitted as a
        /// `style` attribute.
        width: Option<String>,
        align: Option<TableAlign>,
        /// `:class:` — space-separated class names, already split.
        classes: Vec<String>,
        /// `:name:` — reuses [`TargetName`] (the same type explicit
        /// hyperlink targets use) so it can be registered in
        /// `ProjectIndex::targets` with no extra conversion.
        name: Option<TargetName>,
        rows: Vec<TableRow>,
    },
    /// `.. table::` — wraps an existing grid or simple table (given as the
    /// directive's own content) with a title/caption and the layout options
    /// neither ASCII-art syntax has notation of its own for.
    ///
    /// Reuses `TableRow`/`TableCell` like [`Self::DataTable`], but keeps the
    /// header/body split [`crate::Node::Table`] already produces rather than
    /// [`Self::DataTable`]'s flat `rows` + `header_rows: usize`: the wrapped
    /// table arrives pre-split from its own grid or simple table syntax, so
    /// there is no count left to derive. Gets its own variant rather than
    /// reusing `DataTable`'s for the same reason `DataTable` isn't folded into
    /// `Node::Table` — `:header-rows:`/`:stub-columns:` aren't options this
    /// directive has, and would otherwise carry meaningless defaults forever.
    Table {
        /// The directive argument — the table's title/caption. `None` when
        /// no argument was given.
        title: Option<String>,
        widths: Option<TableWidths>,
        /// `:width:` — an opaque CSS length/percentage (e.g. `"100%"`),
        /// passed through verbatim since it's only ever re-emitted as a
        /// `style` attribute.
        width: Option<String>,
        align: Option<TableAlign>,
        /// `:class:` — space-separated class names, already split.
        classes: Vec<String>,
        /// `:name:` — reuses [`TargetName`] (the same type explicit
        /// hyperlink targets use) so it can be registered in
        /// `ProjectIndex::targets` with no extra conversion.
        name: Option<TargetName>,
        header_rows: Vec<TableRow>,
        body_rows: Vec<TableRow>,
    },
    /// `.. math::` — one or more display equations written in LaTeX.
    ///
    /// The LaTeX is stored exactly as the author wrote it and only turned into
    /// markup while rendering, so the AST stays a faithful record of the
    /// source and the choice of math backend never leaks into a `.ast` file.
    Math {
        /// The equations, in source order. RST separates several equations in
        /// one directive with blank lines; that split is consumed here rather
        /// than left to the renderer, so the structure the author expressed is
        /// visible in the AST. Under `:nowrap:` this always holds exactly one
        /// entry — the body verbatim, blank lines included.
        parts: Vec<String>,
        /// `:label:` (or its `:name:` spelling) — reuses [`TargetName`] like
        /// the table directives do, so an `:eq:` reference resolves against it
        /// with no extra conversion.
        label: Option<TargetName>,
        /// `:nowrap:` — the author supplies their own LaTeX environment, so
        /// nothing may be wrapped around the body. Suppresses equation
        /// numbering too, matching Sphinx.
        nowrap: bool,
        /// `:class:` — space-separated class names, already split.
        classes: Vec<String>,
        /// Where the directive was written. The only [`Directive`] to carry a
        /// span, because it is the only one whose content can still fail to
        /// render *after* parsing succeeds — invalid LaTeX is diagnosed by the
        /// renderer, which would otherwise have no position to report (see
        /// `docs/decisions/003-diagnostics.md`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// `.. image::` — a picture on its own, with no caption.
    ///
    /// A newtype variant because [`ImageOptions`] is exactly this directive's
    /// option set, shared verbatim with [`Self::Figure`]. Boxed because that
    /// option set is large and an enum costs its largest variant everywhere:
    /// unboxed, these two directives would grow every [`Node`] in every
    /// document by half again.
    Image(Box<ImageOptions>),
    /// `.. figure::` — an image plus the two things that make it a figure: a
    /// caption naming it, and a legend explaining it.
    ///
    /// Not folded into [`Self::Image`] with everything optional, even though a
    /// captionless figure renders much like an image: `:figwidth:` and
    /// `:figclass:` are meaningless on an `.. image::`, and docutils wraps the
    /// two in different elements regardless of what is filled in.
    Figure(Box<Figure>),
    /// A `.. code-block::` or `.. code::` — a code block with presentation
    /// options. A newtype variant, like [`Self::DomainObject`], because the
    /// two directives form their own closed family with their own option set.
    CodeBlock(CodeBlock),
    /// A `.. highlight::`, which sets the language every following code block
    /// inherits, until the next one.
    ///
    /// Produces no output of its own; it exists purely to change the state the
    /// renderer walks with. Carries a [`ResolvedLanguage`] rather than a
    /// [`crate::CodeLanguage`] because a `.. highlight::` always names a
    /// language — "inherit" would be a meaningless value on it.
    Highlight {
        language: ResolvedLanguage,
        /// `:linenothreshold:` — blocks with at least this many lines get line
        /// numbers without asking. `None` when the option was omitted, which
        /// is Sphinx's "never".
        linenothreshold: Option<std::num::NonZeroU32>,
        /// `:force:` — applies to every block inheriting this language.
        force: bool,
    },
    DomainObject(DomainObjectBody),
    /// One block of the `sphinx.ext.doctest` family (`doctest`, `testcode`,
    /// `testoutput`, `testsetup`, `testcleanup`).
    ///
    /// A newtype variant, like [`Self::DomainObject`], because the five
    /// directives form their own closed family with their own option sets —
    /// see [`DocTestBlock`] for why they are not one struct.
    ///
    /// Note that *rendering* these is entirely independent of *executing*
    /// them: the AST carries what a page needs to display, and a separate,
    /// opt-in Bazel test target runs the code. Nothing on this variant depends
    /// on a test having been run.
    DocTest(DocTestBlock),
    /// `.. py:currentmodule::` — sets the `py`-domain module context for the
    /// rest of the document without documenting a module. `None` is the
    /// reset form (`.. currentmodule:: None`); the sentinel is resolved by
    /// the parser so no later phase re-interprets the literal string.
    PyCurrentModule {
        module: Option<String>,
    },
    /// `.. c:namespace::` — sets the `c`-domain scope absolutely for the rest
    /// of the document and resets the namespace push/pop stack. `None` is the
    /// reset-to-global form (`NULL` or `0`); both sentinels are resolved by
    /// the parser so no later phase re-interprets the literal string.
    CNamespace {
        namespace: Option<String>,
    },
    /// `.. c:namespace-push::` — extends the current `c`-domain scope
    /// relatively. Always carries a scope: an empty argument becomes a
    /// [`Self::Malformed`] rather than a no-op push, which a later
    /// [`Self::CNamespacePop`] would then unbalance.
    CNamespacePush {
        namespace: String,
    },
    /// `.. c:namespace-pop::` — undoes the most recent
    /// [`Directive::CNamespacePush`] in its entirety.
    CNamespacePop,
    /// `.. program::` — sets the `std`-domain "current program" context for
    /// `.. option::`/`.. cmdoption::` definitions and `:option:` references
    /// for the rest of the document. `None` is the reset form
    /// (`.. program:: None`), mirroring [`Directive::PyCurrentModule`]/
    /// [`Directive::CNamespace`] exactly. `name` is already normalized
    /// (whitespace runs collapsed to a single `-`, matching real Sphinx's
    /// `ws_re.sub('-', name)`) by the parser.
    StdProgram {
        name: Option<String>,
    },
    /// A directive whose name this build does not recognize at all.
    ///
    /// The body is kept as written but deliberately *not* parsed — an
    /// unrecognized name says nothing about what its content means — so
    /// everything inside it is invisible to every later phase. That is why the
    /// renderer draws it as a visible error block quoting the source rather
    /// than omitting it, and why the parser reports
    /// [`crate::DiagnosticCode::DirectiveUnknown`] alongside.
    Unknown {
        name: String,
        argument: String,
        body: String,
    },
    /// A directive whose *name* this build recognizes but whose content it
    /// could not accept — a `.. figure::` with no URI, a `.. table::` holding
    /// no table.
    ///
    /// Distinct from [`Self::Unknown`] so the page never claims the name was
    /// the problem. `message` is the same text the parse-time diagnostic
    /// reports, so a reader of the page and a reader of the build log get one
    /// explanation.
    Malformed {
        name: String,
        argument: String,
        body: String,
        message: String,
    },
    /// A `.. |name| replace::`/`unicode::`/`image::` substitution definition.
    ///
    /// Produces no output where it stands, like [`Self::Highlight`]: every
    /// [`crate::InlineNode::SubstitutionReference`] naming it elsewhere in the
    /// document is spliced with its resolved content by a whole-document pass
    /// that runs once parsing finishes, since references may be written
    /// before their definition. See [`SubstitutionDefinition`] for why only
    /// three of docutils' five substitution-only directives are modelled.
    SubstitutionDefinition(SubstitutionDefinition),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_node::InlineNode;
    use crate::non_empty_vector::NonEmptyVector;

    #[test]
    fn test_glossary_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::Glossary {
            entries: vec![GlossaryEntry {
                terms: vec!["term".to_string()],
                definition: vec![],
            }],
            sorted: true,
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_index_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::Index {
            entries: vec![IndexEntry::Term {
                primary: "foo".to_string(),
                subentry: None,
                main: false,
            }],
            id: "index-0".to_string(),
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_domain_object_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::DomainObject(DomainObjectBody::CFunction {
            signatures: NonEmptyVector::single("int add(int a, int b)".into()),
            body: vec![Node::Paragraph(vec![InlineNode::Text(
                "Adds two numbers.".to_string(),
            )])],
        });

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_domain_object_directive_serialization_roundtrip_with_module_options() {
        // Given
        let directive = Directive::DomainObject(DomainObjectBody::PyModule {
            name: "greetings".to_string(),
            platform: Some("Unix, Windows".to_string()),
            synopsis: Some("Greeting utilities.".to_string()),
            deprecated: true,
            body: vec![],
        });

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_domain_object_directive_serialization_roundtrip_with_data_options() {
        // Given
        let directive = Directive::DomainObject(DomainObjectBody::PyData {
            module: None,
            signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
            type_: Some("int".to_string()),
            value: Some("30".to_string()),
            body: vec![],
        });

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_py_current_module_directive_serialization_roundtrip_with_module() {
        // Given
        let directive = Directive::PyCurrentModule {
            module: Some("enum".to_string()),
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_py_current_module_directive_serialization_roundtrip_with_reset() {
        // Given
        let directive = Directive::PyCurrentModule { module: None };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_c_namespace_directive_serialization_roundtrip_with_scope() {
        // Given
        let directive = Directive::CNamespace {
            namespace: Some("A.B".to_string()),
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_c_namespace_directive_serialization_roundtrip_with_reset() {
        // Given — the `NULL`/`0` reset form.
        let directive = Directive::CNamespace { namespace: None };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_program_directive_serialization_roundtrip_with_name() {
        // Given
        let directive = Directive::StdProgram {
            name: Some("dis".to_string()),
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_program_directive_serialization_roundtrip_with_reset() {
        // Given — the `.. program:: None` reset form.
        let directive = Directive::StdProgram { name: None };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_c_namespace_push_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::CNamespacePush {
            namespace: "C.D".to_string(),
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_c_namespace_pop_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::CNamespacePop;

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_sectnum_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::Sectnum(SectnumOptions {
            depth: std::num::NonZeroUsize::new(2),
            start: std::num::NonZeroU32::new(3),
            prefix: "Sec ".to_string(),
            suffix: ".".to_string(),
        });

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_data_table_directive_serialization_roundtrip() {
        // Given
        use crate::table::TableCell;

        let directive = Directive::DataTable {
            source: TableSource::List,
            title: Some("Fruit".to_string()),
            header_rows: 1,
            stub_columns: 0,
            widths: Some(TableWidths::Explicit(vec![30, 70])),
            width: Some("100%".to_string()),
            align: Some(TableAlign::Center),
            classes: vec!["custom".to_string()],
            name: Some(TargetName::new("fruit-table")),
            rows: vec![TableRow {
                cells: vec![TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: vec![Node::Paragraph(vec![InlineNode::Text("Fruit".to_string())])],
                }],
            }],
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_table_directive_serialization_roundtrip() {
        // Given
        use crate::table::TableCell;

        let directive = Directive::Table {
            title: Some("Fruit".to_string()),
            widths: Some(TableWidths::Explicit(vec![30, 70])),
            width: Some("100%".to_string()),
            align: Some(TableAlign::Center),
            classes: vec!["custom".to_string()],
            name: Some(TargetName::new("fruit-table")),
            header_rows: vec![TableRow {
                cells: vec![TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: vec![Node::Paragraph(vec![InlineNode::Text("Fruit".to_string())])],
                }],
            }],
            body_rows: vec![],
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }
}
