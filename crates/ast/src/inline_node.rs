use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::asset_uri::AssetUri;
use crate::code_language::ResolvedLanguage;
use crate::docutils_pep_number::DocutilsPepNumber;
use crate::docutils_rfc_number::DocutilsRfcNumber;
use crate::image::ImageOptions;
use crate::index_entry::{IndexEntry, InvalidIndexEntry};
use crate::inventory_selector::InventorySelector;
use crate::number_format::NumberFormat;
use crate::registry_target::{Registry, RegistryTarget};
use crate::script_position::ScriptPosition;

/// Why the parser refused a role — see [`InlineNode::RefusedRole`].
///
/// One variant per role that can be refused, each carrying what its own
/// diagnostic and its own lowering need: the roles share the reporting pass,
/// not what they are shown as afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleRefusal {
    /// A `:numref:`, lowered to an unlinked [`InlineNode::NumberReference`].
    NumberReference(NumberReferenceRefusal),
    /// A `:pep:`, `:rfc:`, `:cve:` or `:cwe:` whose target `registry` cannot
    /// link, lowered to the role's source text as Sphinx's `problematic` node
    /// shows it. `target` is the target as written, from which the
    /// diagnostic re-derives its reason.
    RegistryTarget { registry: Registry, target: String },
    /// A `:pep-reference:` whose target is not a number from 0 to 9999,
    /// lowered to the role's source text as docutils' `problematic` node
    /// shows it. `target` is the target as written.
    DocutilsPepNumber { target: String },
    /// An `:rfc-reference:` whose target is not a number of at least 1,
    /// lowered to the role's source text as docutils' `problematic` node
    /// shows it. `target` is the target as written.
    DocutilsRfcNumber { target: String },
    /// An `:index:` whose entry its type cannot split, lowered to an
    /// [`InlineNode::IndexReference`] showing `title` and making no entry —
    /// Sphinx warns and still shows the text.
    IndexEntry {
        title: String,
        entry: InvalidIndexEntry,
    },
    /// Interpreted text with a role written both before and after it, lowered
    /// to its source text as docutils' `problematic` node shows it.
    MultipleRoles,
    /// Interpreted text with a role and a hyperlink reference's `_` or `__`,
    /// lowered to its source text as docutils' `problematic` node shows it.
    RoleAndReference,
}

/// Why the parser refused a `:numref:` — see [`RoleRefusal::NumberReference`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberReferenceRefusal {
    /// The explicit title is not a format Sphinx could apply.
    InvalidTitle,
    /// An `:external:` prefix: an inventory holds no numbers.
    External,
}
use crate::object_type::ObjectType;
use crate::span::Span;
use crate::target_search_order::TargetSearchOrder;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlineNode {
    Text(String),
    /// An inline cross-reference produced by the `:ref:` role, linking to a
    /// labeled location elsewhere in the site.
    ///
    /// `display` is the explicit title of the angle-bracket form, and `None`
    /// when the author wrote the bare label. The two are kept apart rather
    /// than defaulting `display` to `target` while parsing, because the text a
    /// bare `:ref:` shows is the *section title* the label points at — which
    /// only the project index knows, so only the renderer can supply it.
    Reference {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<String>,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
        /// Which inventories the target may come from — see
        /// [`InventorySelector`]. Left out of a `.ast` file when ordinary.
        #[serde(default, skip_serializing_if = "InventorySelector::is_any")]
        inventory: InventorySelector,
    },
    Hyperlink {
        text: String,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    AnonymousReference {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    AnonymousHyperlink {
        text: String,
        target: String,
    },
    Emphasis(String),
    Strong(String),
    Literal(String),
    Program(String),
    /// Text set below or above the line, from `:sub:`/`:subscript:`,
    /// `:sup:`/`:superscript:` or a role derived from one with `.. role::`.
    ///
    /// The text is plain, as docutils' generic roles give it: never parsed
    /// for nested markup and never split into a title. `classes` are a
    /// derived role's, already normalized; the built-in roles carry none.
    Script {
        position: ScriptPosition,
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        classes: Vec<String>,
    },
    /// The title of a work, from `:title-reference:`/`:title:`/`:t:` or from
    /// interpreted text written without a role while the default role is
    /// docutils' own — which is what a bare `` `text` `` means unless a
    /// `.. default-role::` or the library's `default_role` says otherwise.
    ///
    /// Plain text, as docutils' generic roles give it: never parsed for
    /// nested markup and never split into a title.
    TitleReference(String),
    /// An inline cross-reference produced by the term role, linking to a glossary entry.
    ///
    /// The `display` field is the visible link text and `term` is the glossary key.
    /// They differ when the role is written with an explicit display-text override,
    /// i.e. the angle-bracket form where the text before the angle bracket is shown
    /// and the text inside the angle brackets is looked up in the glossary index.
    TermReference {
        display: String,
        term: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
        /// Which inventories the target may come from — see
        /// [`InventorySelector`]. Left out of a `.ast` file when ordinary.
        #[serde(default, skip_serializing_if = "InventorySelector::is_any")]
        inventory: InventorySelector,
    },
    /// An inline cross-reference produced by a domain role (e.g. `:func:`,
    /// `:py:func:`, `:c:func:`), linking to a `Directive::DomainObject`.
    ///
    /// `object_type` is always concrete by the time this node exists — the
    /// parser resolves a bare (unprefixed) role via the file's default
    /// domain immediately, mirroring how `Directive::DomainObject` is
    /// resolved.
    ///
    /// `name` and `display` differ when the role target uses a `~` prefix
    /// (e.g. `~pkg.mod.func`): `name` is the full name used to resolve the
    /// cross-reference, `display` is the shortened text shown to the reader
    /// (just the last dotted component). A `!` prefix instead sets `link`
    /// to `false`, suppressing the hyperlink entirely (the target is never
    /// looked up, so a missing target produces no broken-link warning).
    ///
    /// A leading `.` prefix is likewise consumed by the parser: it never
    /// survives into `name` or `display` (it is markup, not part of any
    /// object's name), and is recorded as `search_order` instead.
    ///
    /// A *trailing* `()` — written so the reference reads as a call at the
    /// point of use (`` :c:func:`Py_TYPE()` ``) — is markup too, and splits
    /// the other way round from `~`: it is stripped from `name`, because no
    /// declaration ever registers a name with parens in it, but kept in
    /// `display`, because that is what the reader is meant to see. Real
    /// Sphinx arrives at the same split by stripping the parens at resolution
    /// time and never showing them to the title; doing it while parsing keeps
    /// `name` a plain name at every later phase.
    DomainObjectReference {
        object_type: ObjectType,
        name: String,
        display: String,
        link: bool,
        /// Which resolution order the target asked for. `#[serde(default)]`
        /// keeps `.ast` files written before this field existed loadable —
        /// they predate leading-dot support, so the default (no dot) is the
        /// faithful reading.
        #[serde(default)]
        search_order: TargetSearchOrder,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
        /// Which inventories the target may come from — see
        /// [`InventorySelector`]. Left out of a `.ast` file when ordinary.
        #[serde(default, skip_serializing_if = "InventorySelector::is_any")]
        inventory: InventorySelector,
    },
    /// An inline cross-reference produced by the `:option:` role, linking to
    /// a `.. option::`/`.. cmdoption::` definition.
    ///
    /// Unlike [`Self::DomainObjectReference`], this carries no `object_type`
    /// (always `std:cmdoption`) and no `search_order` — `:option:`'s
    /// resolution is a distinct ambient-program/global-fallback/embedded-
    /// program search (see `rinx_renderer::resolution::option`), not
    /// the dot-prefixed most/least-qualified search [`TargetSearchOrder`]
    /// models. `target` is carried through close to verbatim: only the
    /// explicit-title split (this role's `` `display <target>` `` syntax)
    /// happens at parse time, because the rest of the algorithm depends on
    /// the *merged* project index, not on anything knowable from one file's
    /// parse.
    OptionReference {
        display: String,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
        /// Which inventories the target may come from — see
        /// [`InventorySelector`]. Left out of a `.ast` file when ordinary.
        #[serde(default, skip_serializing_if = "InventorySelector::is_any")]
        inventory: InventorySelector,
    },
    /// A cross-reference produced by the `:any:` role, which names a target
    /// without saying what kind of thing it is.
    ///
    /// Kept as the question rather than lowered to one of the specific
    /// reference variants while parsing, because what `target` names — a
    /// label, a document, a glossary term, an option, an equation or a domain
    /// object — is a fact about the whole project, which only the merged
    /// index knows. The renderer searches every kind and draws the one hit
    /// exactly as its own role would.
    ///
    /// `display` is the explicit title of the angle-bracket form and `None`
    /// for a bare target, as on [`Self::Reference`]: a label hit shows its
    /// section title, which is not known here. `target` is kept as written —
    /// unlike on [`Self::DomainObjectReference`], a trailing `()` is not
    /// stripped, since it is markup only for the domain-object kinds and the
    /// renderer strips it for those alone. A `!` prefix is markup for every
    /// kind, so it is consumed here and recorded as `link: false`: the target
    /// is then never looked up and shown as a literal.
    AnyReference {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<String>,
        target: String,
        link: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
        /// Which inventories the target may come from — see
        /// [`InventorySelector`]. Left out of a `.ast` file when ordinary.
        #[serde(default, skip_serializing_if = "InventorySelector::is_any")]
        inventory: InventorySelector,
    },
    /// A whole-document cross-reference produced by the `:doc:` (or
    /// `:std:doc:`) role.
    ///
    /// `target` is the document name as written — relative to the
    /// referencing document, or to the source root with a leading `/` — and
    /// is resolved only while rendering, because whether that document exists
    /// is a fact about the whole project. `display` is the explicit title of
    /// the angle-bracket form and `None` for a bare target, as on
    /// [`Self::Reference`]: a bare `:doc:` shows the target document's title,
    /// which only the project index knows. A `!` prefix is consumed here and
    /// recorded as `link: false`, as on [`Self::AnyReference`].
    DocReference {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<String>,
        target: String,
        link: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
        /// Which inventories the target may come from — see
        /// [`InventorySelector`]. Left out of a `.ast` file when ordinary.
        #[serde(default, skip_serializing_if = "InventorySelector::is_any")]
        inventory: InventorySelector,
    },
    /// A reference showing the *number* of a figure, table, code block or
    /// section, produced by the `:numref:` (or `:std:numref:`) role.
    ///
    /// `target` is the label as written and resolved only while rendering:
    /// which element it labels, and which number the whole project's toctree
    /// gave that element, only the project index knows. `title` is the
    /// explicit title of the angle-bracket form, already parsed into the
    /// [`NumberFormat`] it is applied as — so a title Sphinx could not apply
    /// was reported here, at the role, and never reaches this variant. `None`
    /// for a bare target, which shows the site's `numfig_format` for the kind
    /// of thing it points at.
    ///
    /// `link: false` is the `!` form, and also what a refused title becomes:
    /// either way the reference is never looked up and `target` holds the text
    /// shown instead — everything after the `!`, or the title as written.
    NumberReference {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<NumberFormat>,
        target: String,
        link: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// A role the parser refused, before the whole-document pass
    /// (`rinx_parser`'s `report_refused_roles`) reports it at its span and
    /// lowers it to what the [`RoleRefusal`] says it is shown as.
    ///
    /// An intermediate node, as [`Self::SubstitutionReference`] is: the inline
    /// scan has nowhere to report a diagnostic, and a refusal must still reach
    /// the author at the role. `text` is what the lowered node shows.
    RefusedRole {
        text: String,
        refusal: RoleRefusal,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// A link to a document of a numbered registry outside the site,
    /// produced by the `:pep:`, `:rfc:`, `:cve:` and `:cwe:` roles — the
    /// four are one construct, differing only in their [`RegistryTarget`].
    ///
    /// Sphinx's roles each yield three nodes — a general-index entry, the
    /// anchor that entry links to, and the link — and this one node carries
    /// all three, so nothing can separate the anchor from the entry pointing
    /// at it. `index_id` is that anchor, minted per document once parsing
    /// ends (`rinx_parser`'s `assign_registry_index_ids`), in the same
    /// sequence as `.. index::` directives' ids; empty until then.
    ///
    /// Only the document's page is stored, not its URL: a PEP's or an RFC's
    /// index is the site's `pep_base_url` or `rfc_base_url`, applied while
    /// rendering so changing it re-parses nothing. `display` is the explicit
    /// title of the angle-bracket form; without one the link shows
    /// [`RegistryTarget::display_text`].
    RegistryReference {
        target: RegistryTarget,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<String>,
        index_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// A link to a Python Enhancement Proposal, produced by docutils' own
    /// `:pep-reference:` role, which Sphinx leaves in place beside its `:pep:`.
    ///
    /// A sibling of [`Self::RegistryReference`] rather than a flag on it, because
    /// it has none of what that node carries beyond the number: no index
    /// entry and so no anchor, no explicit title, no fragment. It shows
    /// `PEP ` and the number as written. As for `:pep:`, only the page is
    /// stored and the site's `pep_base_url` is applied while rendering.
    DocutilsPepReference {
        number: DocutilsPepNumber,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// A link to an RFC, produced by docutils' own `:rfc-reference:` role,
    /// which Sphinx leaves in place beside its `:rfc:`.
    ///
    /// A sibling of [`Self::RegistryReference`] for the reason
    /// [`Self::DocutilsPepReference`] is: no index entry, no anchor, no
    /// explicit title. It shows `RFC ` and the number as `int()` reads it,
    /// never the section. The site's `rfc_base_url` is applied while
    /// rendering.
    DocutilsRfcReference {
        number: DocutilsRfcNumber,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// The `:index:` role: general-index entries, the anchor they link to,
    /// and the text written in the paragraph — the three nodes Sphinx's
    /// `IndexRole` yields, held together for the reason
    /// [`Self::RegistryReference`] holds its three.
    ///
    /// `title` is plain text, never markup, and already final: the explicit
    /// title, or the target with a leading `!` main marker removed. `entries`
    /// are parsed in the `.. index::` grammar; `index_id` is minted per
    /// document once parsing ends, in the sequence the registry roles' anchors
    /// share, and is empty until then.
    IndexReference {
        title: String,
        entries: Vec<IndexEntry>,
        index_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// A link to a file the site serves for download, produced by the
    /// `:download:` (or `:std:download:`) role.
    ///
    /// Unlike every other reference role, `target` names a *file*, not
    /// something the project index knows: it is an [`AssetUri`], split into
    /// an external URL or a project file the moment it is parsed, exactly as
    /// an image's argument is, so the Bazel validator and the renderer
    /// resolve it with the one [`AssetUri::resolve`]. `display` is the
    /// explicit title of the angle-bracket form and `None` for a bare target,
    /// which shows the target as written. A `!` prefix is consumed here and
    /// recorded as `link: false`: nothing is linked or copied, and `target`
    /// then holds the whole text after the `!`.
    DownloadReference {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<String>,
        target: AssetUri,
        link: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// A reference to a project-declared entity, from a role the schema
    /// names — ``:req:`REQ_001` ``, ``:need:`REQ_001` ``, or the built-in
    /// ``:entity:`REQ_001` ``.
    ///
    /// The `role` is kept rather than resolved to a set of acceptable types,
    /// because the types it accepts are a *schema* fact and this node has to
    /// survive into a `.ast` file that outlives the process which parsed it.
    /// Resolution, and the type check the role exists for, happen where the
    /// merged project index is available.
    EntityReference {
        role: String,
        target: String,
        display: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// Inline math produced by the `:math:` role, holding LaTeX verbatim.
    ///
    /// A verbatim context like [`Self::Literal`]: the backslashes are the
    /// content, so this is one of the two variants whose escape markers turn
    /// back into backslashes rather than being dropped (see
    /// `rinx_parser`'s `unescape_node`).
    ///
    /// Carries a span despite not being a cross-reference — unlike every other
    /// self-contained variant, its content can be rejected at render time, and
    /// the resulting diagnostic needs a position.
    Math {
        latex: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// Inline code produced by the `:code:` role, or by a custom role a
    /// `.. role:: name(code)` derives from it.
    ///
    /// Not a [`Self::Literal`]: an inline literal has no language and no
    /// classes, where this carries the ones its role was defined with. A plain
    /// `:code:` has [`ResolvedLanguage::None`] and no classes, and is drawn
    /// unhighlighted. Unlike a literal, its backslashes are escapes — Sphinx's
    /// `code_role` receives interpreted text, so `\*` shows `*`.
    ///
    /// Carries a span for the same reason [`Self::Math`] does: a language
    /// with no grammar behind it is only found while rendering.
    Code {
        text: String,
        /// Always written out: [`ResolvedLanguage`]'s own default is Sphinx's
        /// `highlight_language` (`default`), which is not what an omitted
        /// language means here.
        language: ResolvedLanguage,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        classes: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// An inline cross-reference produced by the `:eq:` role, linking to a
    /// labeled `.. math::` and displaying that equation's number.
    ///
    /// Carries no `display` field, unlike every other cross-reference role:
    /// `:eq:` has no explicit-title form, because the visible text is the
    /// equation number, which is not known until the project index exists.
    EquationReference {
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// A `|name|` substitution reference, before the whole-document
    /// resolution pass (`rinx_parser`'s `resolve_substitutions`)
    /// replaces it with its definition's resolved content.
    ///
    /// An intermediate node rather than something a later phase ever sees:
    /// resolution is intra-document and runs at the end of `parse()`, so a
    /// well-formed `Document` never carries one of these by the time it is
    /// returned. An unresolvable name still degrades to this being replaced
    /// with a literal `Text("|name|")` (plus a diagnostic) rather than
    /// staying — see [`crate::DiagnosticCode::SubstitutionUndefined`].
    SubstitutionReference {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// What a `.. |name| image::` substitution reference resolves to: an
    /// image inline in running text rather than a block of its own.
    ///
    /// Boxed for the same reason [`crate::Directive::Image`] is — an
    /// [`ImageOptions`] is large, and an enum costs its largest variant on
    /// every node.
    InlineImage(Box<ImageOptions>),
}

impl InlineNode {
    /// Where this node's markup was written, for the variants that can
    /// produce a diagnostic; `None` for every other variant.
    ///
    /// Only the cross-reference roles, [`Self::Math`] and [`Self::Code`] carry
    /// a position, because only they can fail at render time — the roles by
    /// not resolving, `Math` by holding LaTeX the math backend rejects, `Code`
    /// by naming a language no grammar highlights. Giving every variant
    /// one would double the size of a parsed document to record something
    /// nothing reads.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        match self {
            Self::Reference { span, .. }
            | Self::Hyperlink { span, .. }
            | Self::AnonymousReference { span, .. }
            | Self::TermReference { span, .. }
            | Self::DomainObjectReference { span, .. }
            | Self::OptionReference { span, .. }
            | Self::AnyReference { span, .. }
            | Self::DocReference { span, .. }
            | Self::DownloadReference { span, .. }
            | Self::NumberReference { span, .. }
            | Self::RefusedRole { span, .. }
            | Self::RegistryReference { span, .. }
            | Self::IndexReference { span, .. }
            | Self::DocutilsPepReference { span, .. }
            | Self::DocutilsRfcReference { span, .. }
            | Self::EntityReference { span, .. }
            | Self::Math { span, .. }
            | Self::Code { span, .. }
            | Self::EquationReference { span, .. }
            | Self::SubstitutionReference { span, .. } => *span,
            _ => None,
        }
    }

    /// Records where this node's markup was written, for the variants that
    /// carry a position; every other variant is returned unchanged.
    ///
    /// Exists so the inline scan can attach positions in exactly one place —
    /// it is the only code that knows a match's offsets — instead of every
    /// role parser having to thread them through its own construction.
    #[must_use]
    pub fn with_span(mut self, at: Option<Span>) -> Self {
        match &mut self {
            Self::Reference { span, .. }
            | Self::Hyperlink { span, .. }
            | Self::AnonymousReference { span, .. }
            | Self::TermReference { span, .. }
            | Self::DomainObjectReference { span, .. }
            | Self::OptionReference { span, .. }
            | Self::AnyReference { span, .. }
            | Self::DocReference { span, .. }
            | Self::DownloadReference { span, .. }
            | Self::NumberReference { span, .. }
            | Self::RefusedRole { span, .. }
            | Self::RegistryReference { span, .. }
            | Self::IndexReference { span, .. }
            | Self::DocutilsPepReference { span, .. }
            | Self::DocutilsRfcReference { span, .. }
            | Self::EntityReference { span, .. }
            | Self::Math { span, .. }
            | Self::Code { span, .. }
            | Self::EquationReference { span, .. }
            | Self::SubstitutionReference { span, .. } => *span = at,
            _ => {}
        }
        self
    }

    /// Whether this node renders as a hyperlink — an `<a>` element.
    ///
    /// Exists for the one construct that cannot contain one: a
    /// [`crate::ButtonLink`]'s label is itself inside an `<a>`, and nesting
    /// two would be invalid HTML. The parser reports such a label and the
    /// renderer flattens it, and both ask this rather than keeping a list of
    /// variants each — two lists that a new linking role would silently leave
    /// disagreeing.
    ///
    /// A [`Self::DomainObjectReference`] written with a `!` prefix has
    /// `link: false` and is deliberately *not* a link: its target is never
    /// looked up, so it nests nothing.
    #[must_use]
    pub const fn renders_as_link(&self) -> bool {
        match self {
            Self::Reference { .. }
            | Self::Hyperlink { .. }
            | Self::AnonymousReference { .. }
            | Self::AnonymousHyperlink { .. }
            | Self::TermReference { .. }
            | Self::OptionReference { .. }
            | Self::EntityReference { .. }
            | Self::EquationReference { .. }
            | Self::RegistryReference { .. }
            | Self::DocutilsPepReference { .. }
            | Self::DocutilsRfcReference { .. } => true,
            Self::DomainObjectReference { link, .. }
            | Self::AnyReference { link, .. }
            | Self::DocReference { link, .. }
            | Self::DownloadReference { link, .. }
            | Self::NumberReference { link, .. } => *link,
            _ => false,
        }
    }
}

/// Flattens a sequence of inline nodes down to the plain text a reader would
/// see, discarding all markup/links. Used where a data field requires a
/// plain `String` — e.g. a nav-sidebar label or an HTML `<title>` — rather
/// than for rendering visible HTML body content (which uses `InlineNode`
/// directly so links/emphasis are preserved).
#[must_use]
pub fn inline_plain_text(nodes: &[InlineNode]) -> String {
    nodes
        .iter()
        .map(|node| -> Cow<'_, str> {
            Cow::Borrowed(match node {
                InlineNode::Text(text)
                | InlineNode::Emphasis(text)
                | InlineNode::Strong(text)
                | InlineNode::Literal(text)
                | InlineNode::Program(text)
                | InlineNode::Script { text, .. }
                | InlineNode::TitleReference(text)
                | InlineNode::Code { text, .. }
                | InlineNode::AnonymousReference { text, .. }
                | InlineNode::RefusedRole { text, .. }
                | InlineNode::IndexReference { title: text, .. }
                | InlineNode::Hyperlink { text, .. }
                | InlineNode::AnonymousHyperlink { text, .. } => text.as_str(),
                // Without the index, a bare label's section title is unknown, so
                // the label itself is the best plain text there is.
                InlineNode::Reference {
                    display, target, ..
                }
                | InlineNode::AnyReference {
                    display, target, ..
                }
                | InlineNode::DocReference {
                    display, target, ..
                } => display.as_deref().unwrap_or(target),
                InlineNode::DownloadReference {
                    display, target, ..
                } => display.as_deref().unwrap_or(target.as_written()),
                InlineNode::TermReference { display, .. }
                | InlineNode::DomainObjectReference { display, .. }
                | InlineNode::OptionReference { display, .. }
                | InlineNode::EntityReference { display, .. }
                // What the link shows: the title, or Sphinx's `PEP <target>`.
                | InlineNode::RegistryReference {
                    display: Some(display),
                    ..
                } => display.as_str(),
                InlineNode::RegistryReference { target, .. } => {
                    return Cow::Owned(target.display_text());
                }
                InlineNode::DocutilsPepReference { number, .. } => {
                    return Cow::Owned(format!("PEP {}", number.as_written()));
                }
                InlineNode::DocutilsRfcReference { number, .. } => {
                    return Cow::Owned(number.display_text());
                }
                // The LaTeX source is the only plain text an equation has: its
                // rendered form is markup, and its `:eq:` number isn't known
                // without the project index this function deliberately doesn't take.
                InlineNode::Math { latex, .. } => latex.as_str(),
                InlineNode::EquationReference { label, .. } => label.as_str(),
                // Without the index there is no number, so the title as written —
                // or the label — is the best plain text there is.
                InlineNode::NumberReference { title, target, .. } => {
                    title.as_ref().map_or(target.as_str(), NumberFormat::as_str)
                }
                // Never reaches a caller of this function in a well-formed
                // document — resolved away by the end of parsing — but degrades
                // to the written name rather than vanishing if one somehow does.
                InlineNode::SubstitutionReference { name, .. } => name.as_str(),
                // An image has no text of its own beyond its `:alt:`, which
                // docutils falls back to the URI for; this function takes no
                // resolver and cannot know the URI's resolved form, so an unset
                // `:alt:` contributes nothing rather than a wrong guess.
                InlineNode::InlineImage(options) => options.alt.as_deref().unwrap_or(""),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_type::PyObjectType;

    #[test]
    fn test_renders_as_link_is_true_for_every_reference_role() {
        // Given — one node per variant the renderer sends to an `<a>`
        let links = vec![
            InlineNode::Reference {
                display: Some("d".to_string()),
                target: "t".to_string(),
                span: None,
                inventory: crate::InventorySelector::Any,
            },
            InlineNode::Hyperlink {
                text: "t".to_string(),
                target: "u".to_string(),
                span: None,
            },
            InlineNode::AnonymousReference {
                text: "t".to_string(),
                span: None,
            },
            InlineNode::AnonymousHyperlink {
                text: "t".to_string(),
                target: "u".to_string(),
            },
            InlineNode::TermReference {
                display: "d".to_string(),
                term: "t".to_string(),
                span: None,
                inventory: crate::InventorySelector::Any,
            },
            InlineNode::OptionReference {
                display: "d".to_string(),
                target: "t".to_string(),
                span: None,
                inventory: crate::InventorySelector::Any,
            },
            InlineNode::EntityReference {
                role: "req".to_string(),
                target: "REQ_1".to_string(),
                display: "REQ_1".to_string(),
                span: None,
            },
        ];

        // When / Then
        for node in links {
            assert!(
                node.renders_as_link(),
                "{node:?} should be reported as a link"
            );
        }
    }

    #[test]
    fn test_renders_as_link_is_false_for_markup_that_is_not_a_link() {
        // Given
        let plain = vec![
            InlineNode::Text("t".to_string()),
            InlineNode::Emphasis("t".to_string()),
            InlineNode::Strong("t".to_string()),
            InlineNode::Literal("t".to_string()),
            InlineNode::Program("t".to_string()),
            InlineNode::Script {
                position: ScriptPosition::Subscript,
                text: "t".to_string(),
                classes: Vec::new(),
            },
            InlineNode::TitleReference("t".to_string()),
        ];

        // When / Then
        for node in plain {
            assert!(!node.renders_as_link(), "{node:?} should not be a link");
        }
    }

    #[test]
    fn test_a_suppressed_domain_object_reference_is_not_a_link() {
        // Given — the `!` prefix form, whose target is never looked up
        let suppressed = InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Function),
            name: "pkg.f".to_string(),
            display: "pkg.f".to_string(),
            link: false,
            search_order: TargetSearchOrder::default(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };
        let linked = InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Function),
            name: "pkg.f".to_string(),
            display: "pkg.f".to_string(),
            link: true,
            search_order: TargetSearchOrder::default(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When / Then
        assert!(!suppressed.renders_as_link());
        assert!(linked.renders_as_link());
    }

    #[test]
    fn test_a_suppressed_any_reference_is_not_a_link() {
        // Given — `:any:` with and without the `!` prefix
        let any = |link| InlineNode::AnyReference {
            display: None,
            target: "t".to_string(),
            link,
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When / Then
        assert!(!any(false).renders_as_link());
        assert!(any(true).renders_as_link());
    }

    #[test]
    fn test_any_reference_carries_its_span() {
        // Given
        let at = Span::new(
            crate::span::Position::new(3, 1),
            crate::span::Position::new(3, 12),
        );
        let node = InlineNode::AnyReference {
            display: None,
            target: "t".to_string(),
            link: true,
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let placed = node.with_span(Some(at));

        // Then
        assert_eq!(placed.span(), Some(at));
    }

    #[test]
    fn test_any_reference_without_explicit_title_roundtrips_without_display() {
        // Given — a bare `:any:`, whose link text only the index can supply
        let node = InlineNode::AnyReference {
            display: None,
            target: "pkg.run".to_string(),
            link: true,
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert!(!json.contains("display"));
        assert!(!json.contains("inventory"));
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_inline_plain_text_uses_the_title_or_target_of_an_any_reference() {
        // Given
        let titled = InlineNode::AnyReference {
            display: Some("Go".to_string()),
            target: "t".to_string(),
            link: true,
            span: None,
            inventory: crate::InventorySelector::Any,
        };
        let bare = InlineNode::AnyReference {
            display: None,
            target: "t".to_string(),
            link: true,
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let text = inline_plain_text(&[titled, bare]);

        // Then
        assert_eq!(text, "Got");
    }

    fn doc_reference(display: Option<&str>, link: bool) -> InlineNode {
        InlineNode::DocReference {
            display: display.map(str::to_string),
            target: "guide/intro".to_string(),
            link,
            span: None,
            inventory: crate::InventorySelector::Any,
        }
    }

    #[test]
    fn test_a_suppressed_doc_reference_is_not_a_link() {
        // Given — `:doc:` with and without the `!` prefix
        let suppressed = doc_reference(None, false);
        let linked = doc_reference(None, true);

        // When / Then
        assert!(!suppressed.renders_as_link());
        assert!(linked.renders_as_link());
    }

    #[test]
    fn test_doc_reference_carries_its_span() {
        // Given
        let at = Span::new(
            crate::span::Position::new(2, 4),
            crate::span::Position::new(2, 20),
        );

        // When
        let placed = doc_reference(None, true).with_span(Some(at));

        // Then
        assert_eq!(placed.span(), Some(at));
    }

    #[test]
    fn test_doc_reference_without_explicit_title_roundtrips_without_display() {
        // Given — a bare `:doc:`, whose link text only the index can supply
        let node = doc_reference(None, true);

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert!(!json.contains("display"));
        assert!(!json.contains("inventory"));
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_inline_plain_text_uses_the_title_or_target_of_a_doc_reference() {
        // Given
        let titled = doc_reference(Some("Intro"), true);
        let bare = doc_reference(None, true);

        // When
        let text = inline_plain_text(&[titled, bare]);

        // Then
        assert_eq!(text, "Introguide/intro");
    }

    fn download_reference(display: Option<&str>, link: bool) -> InlineNode {
        InlineNode::DownloadReference {
            display: display.map(str::to_string),
            target: AssetUri::new("data/sample.csv"),
            link,
            span: None,
        }
    }

    #[test]
    fn test_a_suppressed_download_reference_is_not_a_link() {
        // Given — `:download:` with and without the `!` prefix
        let suppressed = download_reference(None, false);
        let linked = download_reference(None, true);

        // When / Then
        assert!(!suppressed.renders_as_link());
        assert!(linked.renders_as_link());
    }

    #[test]
    fn test_download_reference_carries_its_span() {
        // Given
        let at = Span::new(
            crate::span::Position::new(3, 1),
            crate::span::Position::new(3, 30),
        );

        // When
        let placed = download_reference(None, true).with_span(Some(at));

        // Then
        assert_eq!(placed.span(), Some(at));
    }

    #[test]
    fn test_download_reference_without_explicit_title_roundtrips_without_display() {
        // Given
        let node = download_reference(None, true);

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert!(!json.contains("display"));
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_inline_plain_text_uses_the_title_or_written_target_of_a_download_reference() {
        // Given
        let titled = download_reference(Some("the data"), true);
        let bare = download_reference(None, true);

        // When
        let text = inline_plain_text(&[titled, bare]);

        // Then
        assert_eq!(text, "the datadata/sample.csv");
    }

    fn number_reference(title: Option<&str>, link: bool) -> InlineNode {
        InlineNode::NumberReference {
            title: title.map(|text| NumberFormat::parse(text).expect("a valid format")),
            target: "fig-root".to_string(),
            link,
            span: None,
        }
    }

    #[test]
    fn test_a_suppressed_number_reference_is_not_a_link() {
        // Given — `:numref:` with and without the `!` prefix
        let suppressed = number_reference(None, false);
        let linked = number_reference(None, true);

        // When / Then
        assert!(!suppressed.renders_as_link());
        assert!(linked.renders_as_link());
    }

    #[test]
    fn test_number_reference_carries_its_span() {
        // Given
        let at = Span::new(
            crate::span::Position::new(4, 2),
            crate::span::Position::new(4, 22),
        );

        // When
        let placed = number_reference(None, true).with_span(Some(at));

        // Then
        assert_eq!(placed.span(), Some(at));
    }

    #[test]
    fn test_number_reference_roundtrips_its_title_as_written() {
        // Given
        let titled = number_reference(Some("Figure {number}"), true);
        let bare = number_reference(None, true);

        // When
        let titled_json = serde_json::to_string(&titled).expect("serializes");
        let bare_json = serde_json::to_string(&bare).expect("serializes");

        // Then
        assert!(
            titled_json.contains("\"title\":\"Figure {number}\""),
            "{titled_json}"
        );
        assert!(!bare_json.contains("title"));
        assert_eq!(
            serde_json::from_str::<InlineNode>(&titled_json).expect("deserializes"),
            titled
        );
    }

    #[test]
    fn test_inline_plain_text_uses_the_title_or_label_of_a_number_reference() {
        // Given
        let titled = number_reference(Some("Fig. %s"), true);
        let bare = number_reference(None, true);

        // When
        let text = inline_plain_text(&[titled, bare]);

        // Then
        assert_eq!(text, "Fig. %sfig-root");
    }

    #[test]
    fn test_a_refused_role_carries_its_span_and_shows_its_text() {
        // Given
        let at = Span::new(
            crate::span::Position::new(5, 1),
            crate::span::Position::new(5, 30),
        );
        let refused = InlineNode::RefusedRole {
            text: "see this".to_string(),
            refusal: RoleRefusal::NumberReference(NumberReferenceRefusal::InvalidTitle),
            span: None,
        };

        // When
        let placed = refused.with_span(Some(at));

        // Then
        assert_eq!(placed.span(), Some(at));
        assert!(!placed.renders_as_link());
        assert_eq!(inline_plain_text(&[placed]), "see this");
    }

    fn pep(target: &str, display: Option<&str>) -> InlineNode {
        InlineNode::RegistryReference {
            target: RegistryTarget::parse(Registry::Pep, target).unwrap(),
            display: display.map(str::to_string),
            index_id: "index-0".to_string(),
            span: None,
        }
    }

    #[test]
    fn test_a_pep_reference_carries_its_span_and_is_a_link() {
        // Given
        let at = Span::new(
            crate::span::Position::new(2, 1),
            crate::span::Position::new(2, 9),
        );

        // When
        let placed = pep("8", None).with_span(Some(at));

        // Then
        assert_eq!(placed.span(), Some(at));
        assert!(placed.renders_as_link());
    }

    #[test]
    fn test_inline_plain_text_shows_a_pep_reference_as_its_link_text() {
        // Given / When / Then
        assert_eq!(inline_plain_text(&[pep("8#x", None)]), "PEP 8#x");
        assert_eq!(
            inline_plain_text(&[pep("8", Some("Style guide"))]),
            "Style guide"
        );
    }

    #[test]
    fn test_inline_plain_text_shows_an_rfc_section_in_words() {
        // Given
        let node = InlineNode::RegistryReference {
            target: RegistryTarget::parse(Registry::Rfc, "2324#section-2").unwrap(),
            display: None,
            index_id: "index-0".to_string(),
            span: None,
        };

        // When / Then
        assert_eq!(inline_plain_text(&[node]), "RFC 2324 Section 2");
    }

    #[test]
    fn test_a_pep_reference_round_trips_through_json() {
        // Given
        let node = pep("0008#naming", Some("naming"));

        // When
        let json = serde_json::to_string(&node).unwrap();

        // Then
        assert_eq!(serde_json::from_str::<InlineNode>(&json).unwrap(), node);
    }

    fn docutils_pep(number: &str) -> InlineNode {
        InlineNode::DocutilsPepReference {
            number: DocutilsPepNumber::parse(number).unwrap(),
            span: None,
        }
    }

    #[test]
    fn test_a_docutils_pep_reference_carries_its_span_and_is_a_link() {
        // Given
        let at = Span::new(
            crate::span::Position::new(3, 1),
            crate::span::Position::new(3, 20),
        );

        // When
        let placed = docutils_pep("8").with_span(Some(at));

        // Then
        assert_eq!(placed.span(), Some(at));
        assert!(placed.renders_as_link());
    }

    #[test]
    fn test_inline_plain_text_shows_a_docutils_pep_reference_as_written() {
        // Given / When / Then
        assert_eq!(inline_plain_text(&[docutils_pep("08")]), "PEP 08");
    }

    #[test]
    fn test_a_docutils_pep_reference_round_trips_through_json() {
        // Given
        let node = docutils_pep("0008");

        // When
        let json = serde_json::to_string(&node).unwrap();

        // Then
        assert_eq!(serde_json::from_str::<InlineNode>(&json).unwrap(), node);
    }

    fn docutils_rfc(number: &str) -> InlineNode {
        InlineNode::DocutilsRfcReference {
            number: DocutilsRfcNumber::parse(number).unwrap(),
            span: None,
        }
    }

    #[test]
    fn test_a_docutils_rfc_reference_carries_its_span_and_is_a_link() {
        // Given
        let at = Span::new(
            crate::span::Position::new(3, 1),
            crate::span::Position::new(3, 20),
        );

        // When
        let placed = docutils_rfc("2822").with_span(Some(at));

        // Then
        assert_eq!(placed.span(), Some(at));
        assert!(placed.renders_as_link());
    }

    #[test]
    fn test_inline_plain_text_shows_a_docutils_rfc_reference_normalized() {
        // Given / When / Then
        assert_eq!(inline_plain_text(&[docutils_rfc("02822#s")]), "RFC 2822");
    }

    #[test]
    fn test_a_docutils_rfc_reference_round_trips_through_json() {
        // Given
        let node = docutils_rfc("2822#section-3");

        // When
        let json = serde_json::to_string(&node).unwrap();

        // Then
        assert_eq!(serde_json::from_str::<InlineNode>(&json).unwrap(), node);
    }

    #[test]
    fn test_code_carries_its_span_and_shows_its_text() {
        // Given
        let at = Span::new(
            crate::span::Position::new(1, 3),
            crate::span::Position::new(1, 20),
        );
        let node = InlineNode::Code {
            text: "x = 1".to_string(),
            language: ResolvedLanguage::None,
            classes: Vec::new(),
            span: None,
        };

        // When
        let placed = node.with_span(Some(at));

        // Then
        assert_eq!(placed.span(), Some(at));
        assert!(!placed.renders_as_link());
        assert_eq!(inline_plain_text(&[placed]), "x = 1");
    }

    #[test]
    fn test_code_roundtrips_its_language_and_classes() {
        // Given
        let node = InlineNode::Code {
            text: "print()".to_string(),
            language: ResolvedLanguage::parse("python").unwrap(),
            classes: vec!["extra".to_string()],
            span: None,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_plain_code_roundtrips_without_classes() {
        // Given
        let node = InlineNode::Code {
            text: "x".to_string(),
            language: ResolvedLanguage::None,
            classes: Vec::new(),
            span: None,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");

        // Then — no classes key, and the unhighlighted language survives
        assert!(!json.contains("classes"));
        assert_eq!(
            serde_json::from_str::<InlineNode>(&json).expect("Failed to deserialize"),
            node
        );
    }

    #[test]
    fn test_inline_node_program_serialization_roundtrip() {
        // Given
        let node = InlineNode::Program("curl".to_string());

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_option_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::OptionReference {
            display: "-O <dis --show-offsets>".to_string(),
            target: "dis --show-offsets".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::Reference {
            display: Some("GenericAlias".to_string()),
            target: "types-genericalias".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_reference_without_explicit_title_roundtrips_without_display() {
        // Given — a bare `:ref:`, whose title only the index can supply
        let node = InlineNode::Reference {
            display: None,
            target: "home-index".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert!(!json.contains("display"));
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_term_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::TermReference {
            display: "the environment".to_string(),
            term: "environment".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_term_reference_display_equals_term_when_no_alias() {
        // Given
        let term_text = "environment";

        // When
        let node = InlineNode::TermReference {
            display: term_text.to_string(),
            term: term_text.to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // Then
        if let InlineNode::TermReference { display, term, .. } = node {
            assert_eq!(display, term);
        } else {
            panic!("Expected TermReference");
        }
    }

    #[test]
    fn test_domain_object_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Function),
            name: "foo".to_string(),
            display: "foo".to_string(),
            link: true,
            search_order: TargetSearchOrder::MostQualifiedFirst,
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_domain_object_reference_deserializes_without_search_order_field() {
        // Given — an `.ast` file written before leading-dot support existed.
        let json = r#"{"DomainObjectReference":{"object_type":"py:function","name":"foo","display":"foo","link":true}}"#;

        // When
        let deserialized: InlineNode = serde_json::from_str(json).expect("Failed to deserialize");

        // Then — it reads as an unprefixed target, not a parse failure.
        assert_eq!(
            deserialized,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: crate::InventorySelector::Any,
            }
        );
    }

    #[test]
    fn test_inline_plain_text_concatenates_plain_text_nodes() {
        // Given
        let nodes = vec![
            InlineNode::Text("Hello ".to_string()),
            InlineNode::Strong("world".to_string()),
        ];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn test_inline_plain_text_keeps_a_subscript_as_its_text() {
        // Given — `H\ :sub:`2`\ O`
        let nodes = vec![
            InlineNode::Text("H".to_string()),
            InlineNode::Script {
                position: ScriptPosition::Subscript,
                text: "2".to_string(),
                classes: Vec::new(),
            },
            InlineNode::Text("O".to_string()),
        ];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "H2O");
    }

    #[test]
    fn test_inline_plain_text_keeps_a_title_reference_as_its_text() {
        // Given — `` `Dune` ``
        let nodes = vec![
            InlineNode::Text("Read ".to_string()),
            InlineNode::TitleReference("Dune".to_string()),
        ];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "Read Dune");
    }

    #[test]
    fn test_inline_plain_text_uses_display_for_reference() {
        // Given — an explicit-title `:ref:`
        let nodes = vec![InlineNode::Reference {
            display: Some("GenericAlias".to_string()),
            target: "types-genericalias".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        }];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "GenericAlias");
    }

    #[test]
    fn test_inline_plain_text_falls_back_to_the_label_for_a_bare_reference() {
        // Given — a bare `:ref:`; the section title is not known without an index
        let nodes = vec![InlineNode::Reference {
            display: None,
            target: "types-genericalias".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        }];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "types-genericalias");
    }

    #[test]
    fn test_inline_plain_text_uses_shortened_display_for_domain_object_reference() {
        // Given — a `~`-shortened domain-object reference
        let nodes = vec![InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Module),
            name: "pkg.submodule".to_string(),
            display: "submodule".to_string(),
            link: true,
            search_order: TargetSearchOrder::LeastQualifiedFirst,
            span: None,
            inventory: crate::InventorySelector::Any,
        }];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "submodule");
    }

    #[test]
    fn test_inline_plain_text_uses_display_for_term_reference() {
        // Given
        let nodes = vec![InlineNode::TermReference {
            display: "the env".to_string(),
            term: "environment".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        }];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "the env");
    }

    #[test]
    fn test_inline_plain_text_returns_empty_string_for_empty_input() {
        assert_eq!(inline_plain_text(&[]), "");
    }
}
