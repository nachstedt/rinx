use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Declares [`DiagnosticCode`] from a single table of `Variant => "dotted.id"`
/// pairs, deriving the enum, its [`DiagnosticCode::as_str`], its [`FromStr`]
/// and its [`DiagnosticCode::ALL`] listing from that one source.
///
/// Written as a macro rather than a hand-maintained `match` in each direction
/// because the two directions must stay exact inverses: an author writes an id
/// in a `.. noqa:` comment and the parser must map it back to the same variant
/// the diagnostic was raised with. Three parallel hand-written tables would let
/// a half-finished edit compile and silently break suppression for one code.
macro_rules! diagnostic_codes {
    ($( $(#[$meta:meta])* $variant:ident => $id:literal ),+ $(,)?) => {
        /// A stable identifier for one kind of diagnostic, used both to label a
        /// warning on the terminal and to name it in a `.. noqa:` comment.
        ///
        /// Ids are dotted and grouped by the construct they concern
        /// (`link.*`, `table.grid.*`, `doctest.*`, …). They are part of the
        /// documented surface the moment an author writes one into a document,
        /// so treat a rename as a breaking change.
        ///
        /// An enum rather than a bare string so that a typo in *our* code is a
        /// compile error, while a typo in an author's document is a reported
        /// [`Self::NoqaUnknownCode`] rather than a suppression that silently
        /// matches nothing.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum DiagnosticCode {
            // Serialized as the dotted id rather than the variant name, so the
            // `.ast`, the `--warnings-output` sidecar, the terminal and a
            // `.. noqa:` comment all spell a code exactly one way.
            $( $(#[$meta])* #[serde(rename = $id)] $variant ),+
        }

        impl DiagnosticCode {
            /// Every code, in declaration order. Lets the `.. noqa:` parser
            /// suggest near-misses and lets tests assert over the whole table.
            pub const ALL: &'static [Self] = &[ $( Self::$variant ),+ ];

            /// The dotted id this code is written as.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( Self::$variant => $id ),+
                }
            }
        }

        impl FromStr for DiagnosticCode {
            type Err = UnknownDiagnosticCode;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                match text {
                    $( $id => Ok(Self::$variant), )+
                    _ => Err(UnknownDiagnosticCode(text.to_string())),
                }
            }
        }
    };
}

diagnostic_codes! {
    // --- Cross-references, reported by the renderer -------------------------
    /// A `:ref:` whose label is in no document's index.
    LinkBrokenRef => "link.broken-ref",
    /// A named hyperlink with no matching target.
    LinkBrokenHyperlink => "link.broken-hyperlink",
    /// An anonymous `__` reference with no anonymous target left to consume.
    LinkBrokenAnonymous => "link.broken-anonymous",
    /// A `:term:` naming no glossary entry.
    LinkBrokenTerm => "link.broken-term",
    /// An `:option:` naming no `.. option::` definition.
    LinkBrokenOption => "link.broken-option",
    /// An `:eq:` naming no labeled `.. math::`.
    LinkBrokenEquation => "link.broken-eq",
    /// A domain-object role (`:func:`, `:py:class:`, …) that resolved to nothing.
    LinkBrokenObject => "link.broken-object",
    /// A dot-prefixed domain-object role whose suffix search matched several
    /// objects, so it was deliberately left unresolved.
    LinkAmbiguousObject => "link.ambiguous-object",
    /// A `:doc:` naming no document of this site, nor one any inventory it
    /// may search lists.
    LinkBrokenDoc => "link.broken-doc",
    /// An `:any:` role whose target is no label, document, term, option,
    /// equation or domain object, here or in any inventory it may search.
    LinkBrokenAny => "link.broken-any",
    /// An `:any:` role whose target names several things at once, so it was
    /// deliberately left unresolved rather than linked to one of them.
    LinkAmbiguousAny => "link.ambiguous-any",
    /// A domain-object role that resolved only via an object-type alias — the
    /// definition's own type differs from the one the role asked for.
    LinkTypeMismatch => "link.type-mismatch",
    /// An `:external+name:` role naming an inventory the build never declared.
    LinkUnknownInventory => "link.unknown-inventory",
    /// A `:numref:` whose label is in no document's index, or labels
    /// something `numfig` never numbers — a paragraph, an uncaptioned code
    /// block. Sphinx calls both an "undefined label".
    LinkBrokenNumref => "link.broken-numref",

    // --- `:numref:` --------------------------------------------------------
    /// A `:numref:` to a figure, table or code block while `numfig` is off in
    /// `rinx.toml`, so nothing has a number to show.
    NumrefDisabled => "numref.disabled",
    /// A `:numref:` to an element that exists but was given no number: it has
    /// no caption, no toctree reaches its document, or its section is not
    /// numbered.
    NumrefUnnumbered => "numref.unnumbered",
    /// A `:numref:` whose title shows `{name}` for an element with no caption.
    NumrefNoCaption => "numref.no-caption",
    /// A `:numref:` title Sphinx could not apply: no `%s`, two of them, an
    /// unknown `{field}`, an unbalanced brace.
    NumrefInvalidFormat => "numref.invalid-format",
    /// An `:external:numref:` — an inventory holds no numbers.
    NumrefExternal => "numref.external",

    // --- Headings ----------------------------------------------------------
    /// A section adornment shorter than the title's display width, which
    /// docutils accepts and reports rather than rejecting.
    HeadingUnderlineTooShort => "heading.underline-too-short",

    // --- Transitions -------------------------------------------------------
    TransitionAtDocumentStart => "transition.at-document-start",
    TransitionAdjacent => "transition.adjacent",
    TransitionAtDocumentEnd => "transition.at-document-end",

    // --- Grid tables -------------------------------------------------------
    TableGridInconsistentIndent => "table.grid.inconsistent-indent",
    TableGridWidthMismatch => "table.grid.width-mismatch",
    TableGridUnterminated => "table.grid.unterminated",
    TableGridMultipleHeaderSeparators => "table.grid.multiple-header-separators",
    TableGridNoRows => "table.grid.no-rows",
    TableGridNoBodyRows => "table.grid.no-body-rows",
    TableGridNoColumns => "table.grid.no-columns",
    TableGridInconsistentColumnBoundary => "table.grid.inconsistent-column-boundary",

    // --- Simple tables -----------------------------------------------------
    TableSimpleUnderIndented => "table.simple.under-indented",
    TableSimpleBorderWidthMismatch => "table.simple.border-width-mismatch",
    TableSimpleNoBottomBorder => "table.simple.no-bottom-border",
    TableSimpleMultipleHeaderSeparators => "table.simple.multiple-header-separators",
    TableSimpleIncompleteColumnSpan => "table.simple.incomplete-column-span",
    TableSimpleTextInMargin => "table.simple.text-in-margin",
    TableSimpleUnalignedColumnSpan => "table.simple.unaligned-column-span",
    TableSimpleEmptyFirstCell => "table.simple.empty-first-cell",

    // --- Lists -------------------------------------------------------------
    ListBulletChanged => "list.bullet-changed",
    ListEnumeratedStartNotOne => "list.enumerated-start-not-one",
    ListEnumeratedNotRecognised => "list.enumerated-not-recognised",
    ListEnumeratedNoBlankLine => "list.enumerated-no-blank-line",

    // --- Block quotes --------------------------------------------------------
    /// A block quote ended because a less-indented line followed it directly,
    /// with no blank line in between.
    BlockQuoteUnindentNoBlankLine => "block-quote.unindent-no-blank-line",

    // --- Line blocks ---------------------------------------------------------
    /// A line block ended because a non-`|` line followed it directly, with
    /// no blank line in between.
    LineBlockEndsWithoutBlankLine => "line-block.ends-without-blank-line",

    // --- Directives, generally ---------------------------------------------
    /// A directive name this build does not recognize. Its body is never
    /// parsed, so everything inside it is lost to every later phase — which is
    /// why this is reported rather than left to the visible error block the
    /// renderer draws in its place.
    DirectiveUnknown => "directive.unknown",
    /// A `.. c:namespace-push::` with no scope argument. Refused rather than
    /// accepted as a no-op push, which a later `namespace-pop` would then
    /// unbalance.
    DirectiveNamespacePushArgumentMissing => "directive.namespace-push-argument-missing",
    DirectiveToctreeUnknownOption => "directive.toctree-unknown-option",
    DirectiveContentsUnknownOption => "directive.contents-unknown-option",
    DirectiveSectnumUnknownOption => "directive.sectnum-unknown-option",
    DirectiveDropdownUnknownOption => "directive.dropdown-unknown-option",
    DirectiveGridUnknownOption => "directive.grid-unknown-option",
    DirectiveGridItemUnknownOption => "directive.grid-item-unknown-option",
    DirectiveButtonLinkUnknownOption => "directive.button-link-unknown-option",
    DirectiveEntityTableUnknownOption => "directive.entity-table-unknown-option",
    DirectiveEntityFlowUnknownOption => "directive.entity-flow-unknown-option",
    DirectiveEntitySequenceUnknownOption => "directive.entity-sequence-unknown-option",
    DirectiveEntityPieUnknownOption => "directive.entity-pie-unknown-option",
    DirectiveEntityBarUnknownOption => "directive.entity-bar-unknown-option",
    DirectiveUnknownOption => "directive.unknown-option",
    DirectiveVersionArgumentMissing => "directive.version-argument-missing",
    DirectiveTitleArgumentMissing => "directive.title-argument-missing",

    // --- `list-table` / `csv-table` shared options -------------------------
    TableDataWidthsCountMismatch => "table.data.widths-count-mismatch",
    TableDataWidthsInvalid => "table.data.widths-invalid",
    TableDataAlignInvalid => "table.data.align-invalid",
    TableDataIntegerInvalid => "table.data.integer-invalid",
    TableDataRowCellCount => "table.data.row-cell-count",
    TableDataNotABulletList => "table.data.not-a-bullet-list",

    // --- `.. table::` directive ----------------------------------------------
    /// The directive's content is empty (after its option lines).
    TableDirectiveNoContent => "table.directive.no-content",
    /// The directive's content parses, but isn't a table.
    TableDirectiveNotATable => "table.directive.not-a-table",
    /// The directive's content is more than just the one wrapped table.
    TableDirectiveMultipleBlocks => "table.directive.multiple-blocks",

    // --- `csv-table` data --------------------------------------------------
    CsvUrlUnsupported => "csv.url-unsupported",
    CsvFileAndContent => "csv.file-and-content",
    CsvNoData => "csv.no-data",
    CsvFileUnreadable => "csv.file-unreadable",
    CsvEncodingUnsupported => "csv.encoding-unsupported",
    CsvMalformedHeader => "csv.malformed-header",
    CsvMalformedData => "csv.malformed-data",
    CsvHeaderRowsExceed => "csv.header-rows-exceed",
    CsvStubColumnsExceed => "csv.stub-columns-exceed",
    CsvDialectInvalidChar => "csv.dialect-invalid-char",
    CsvDialectNonAscii => "csv.dialect-non-ascii",

    // --- `sphinx.ext.doctest` directives -----------------------------------
    DoctestOptionNotSupported => "doctest.option-not-supported",
    DoctestUnknownOption => "doctest.unknown-option",
    DoctestPyVersionInvalid => "doctest.pyversion-invalid",
    DoctestFlagMissingSign => "doctest.flag-missing-sign",
    DoctestUnknownFlag => "doctest.unknown-flag",
    DoctestNoCode => "doctest.no-code",
    DoctestEmptyGroup => "doctest.empty-group",

    // --- `.. math::` and the math roles ------------------------------------
    /// A `:label:`/`:name:` option with no value, so nothing could reference
    /// the equation and it stays unnumbered.
    MathEmptyLabel => "math.empty-label",
    /// LaTeX the math renderer rejected, reported while rendering rather than
    /// while parsing — the parser deliberately never inspects the LaTeX, since
    /// only the renderer knows the math backend.
    MathInvalidLatex => "math.invalid-latex",

    // --- `.. code-block::` / `.. code::` / `.. highlight::` ----------------
    /// A language argument, or a `.. highlight::`, naming nothing at all.
    CodeBlockEmptyLanguage => "code-block.empty-language",
    /// A `:name:` option with no value, so nothing could `:ref:` the block.
    CodeBlockEmptyName => "code-block.empty-name",
    /// A `:lineno-start:`, `:dedent:`, `:number-lines:` or `:linenothreshold:`
    /// whose value is not a positive integer.
    CodeBlockInvalidInteger => "code-block.invalid-integer",
    /// An `:emphasize-lines:` that could not be read, or that names a line the
    /// block does not have.
    CodeBlockEmphasizeLinesInvalid => "code-block.emphasize-lines-invalid",
    /// A language with no grammar behind it, so the block was left
    /// unhighlighted. Reported while rendering rather than while parsing:
    /// only the renderer knows the set of languages the backend supports.
    CodeBlockUnknownLanguage => "code-block.unknown-language",
    /// A grammar that failed part-way through highlighting, so the block was
    /// left unhighlighted. Also render-time, and for the same reason.
    CodeBlockHighlightFailed => "code-block.highlight-failed",

    // --- `.. role::` --------------------------------------------------------
    /// An argument that is not `name(base)`, or a name no role could be
    /// written with, so the role was not defined.
    RoleInvalidArgument => "role.invalid-argument",
    /// A base role other than `code`, or none at all — the only kind of role
    /// this build can derive — so the role was not defined.
    RoleUnsupportedBase => "role.unsupported-base",
    /// A name this build already gives a role, which a custom role could
    /// never take over — the built-in one is matched first — so it was not
    /// defined.
    RoleBuiltinName => "role.builtin-name",
    /// A `:language:` option with no value; the role is defined unhighlighted.
    RoleEmptyLanguage => "role.empty-language",
    /// A `:class:` value that normalizes to no class name at all, so it was
    /// dropped.
    RoleInvalidClass => "role.invalid-class",

    // --- `:code:` and the roles derived from it -----------------------------
    /// A language with no grammar behind it, so the code was left
    /// unhighlighted. Reported while rendering, as for a code block.
    CodeRoleUnknownLanguage => "code-role.unknown-language",
    /// A grammar that failed part-way through highlighting, so the code was
    /// left unhighlighted.
    CodeRoleHighlightFailed => "code-role.highlight-failed",

    // --- `.. image::` / `.. figure::` --------------------------------------
    /// The directive has no argument, so there is nothing to show.
    ImageMissingUri => "image.missing-uri",
    /// A `:height:`, `:width:` or `:figwidth:` that is not a well-formed
    /// measurement, or that uses a unit CSS does not have.
    ImageInvalidLength => "image.invalid-length",
    /// A `:scale:` that is not a non-negative integer percentage.
    ImageInvalidScale => "image.invalid-scale",
    /// An `:align:` naming none of `left`/`center`/`right` — including the
    /// three vertical alignments, which docutils accepts only on an image
    /// inside a substitution definition.
    ImageInvalidAlign => "image.invalid-align",
    /// A `:loading:` naming none of `embed`/`link`/`lazy`.
    ImageInvalidLoading => "image.invalid-loading",
    /// A `:target:` or `:name:` written with no value, so it could neither
    /// link anywhere nor be linked to.
    ImageEmptyOptionValue => "image.empty-option-value",
    /// Content below an `.. image::`, which takes none. A `.. figure::` is
    /// the directive that has a body.
    ImageContentNotAllowed => "image.content-not-allowed",
    /// A `:scale:` with no `:width:` or `:height:` to apply it to. docutils
    /// would read the image file's own dimensions here; this build never opens
    /// the file while rendering, so the option is dropped rather than
    /// silently mis-sizing the image (see `docs/decisions/007-image-assets.md`).
    ImageScaleNoDimensions => "image.scale-no-dimensions",
    /// A `:loading: embed` on an external URL. Embedding one would mean
    /// fetching it during the build, which would stop the build being
    /// hermetic, so the image is linked instead.
    ImageEmbedExternal => "image.embed-external",
    /// A `:loading: embed` whose bytes never reached the renderer — in a Bazel
    /// build, an image missing from the library's `images` attribute. Reported
    /// while rendering, since only there is the asset sidecar known.
    ImageEmbedUnavailable => "image.embed-unavailable",

    // --- `:download:` -------------------------------------------------------
    /// A `:download:` naming a project file that never reached the site's
    /// `_downloads/` directory — in a Bazel build, a file missing from the
    /// library's `downloads` attribute. Fails the build rather than warning,
    /// as an undeclared image does: the page would otherwise ship a link to
    /// nothing. Reported by the site's asset validation, the one step that
    /// sees both the documents and the bundle.
    DownloadUndeclared => "download.undeclared",

    // --- `.. include::` ----------------------------------------------------
    /// The directive has no argument, so it names no file to splice in.
    IncludeMissingPath => "include.missing-path",
    /// The named file could not be read — in a Bazel build, usually one
    /// missing from the library's `parse_data` attribute.
    IncludeFileUnreadable => "include.file-unreadable",
    /// A file that includes itself, directly or through a chain of other
    /// files. The chain is named in the message, since the offending edit may
    /// be in any link of it.
    IncludeCycle => "include.cycle",
    /// Includes nested more deeply than the parser will follow. Distinct from
    /// [`Self::IncludeCycle`]: a very deep but finite chain is not a mistake in
    /// the same way, and the fix is different.
    IncludeDepthExceeded => "include.depth-exceeded",
    /// A `:start-after:`, `:end-before:`, `:start-at:` or `:end-at:` whose
    /// text appears nowhere in the file, so the selection could not be made.
    IncludeTextNotFound => "include.text-not-found",
    /// A `:start-line:`/`:end-line:`/`:lines:` naming a range the file does
    /// not have, or one that ends before it begins.
    IncludeInvalidLineRange => "include.invalid-line-range",
    /// Selection options that between them select nothing, so the directive
    /// would silently contribute an empty block.
    IncludeEmptySelection => "include.empty-selection",
    /// An `:encoding:` other than UTF-8 or a subset of it.
    IncludeEncodingUnsupported => "include.encoding-unsupported",
    /// A `:tab-width:` that is not an integer.
    IncludeInvalidTabWidth => "include.invalid-tab-width",
    /// An option this directive does not have.
    IncludeUnknownOption => "include.unknown-option",

    // --- `.. literalinclude::` ---------------------------------------------
    /// The `:diff:` file could not be read. Separate from
    /// [`Self::IncludeFileUnreadable`] so a document can suppress one without
    /// the other — they are different files and different mistakes.
    LiteralIncludeDiffUnreadable => "literalinclude.diff-unreadable",
    /// A `:lineno-match:` on a selection that is not one contiguous run of
    /// lines, so there is no single number the first line could carry.
    LiteralIncludeLinenoMatchUnusable => "literalinclude.lineno-match-unusable",
    /// A `:pyobject:`, which would need a Python parser to find the named
    /// class or function. Refused rather than ignored, so the page never
    /// silently shows the whole file where one function was meant.
    LiteralIncludePyObjectUnsupported => "literalinclude.pyobject-unsupported",

    // --- `.. toctree::` ----------------------------------------------------
    /// A `:maxdepth:` whose value is not a positive integer. Sphinx would
    /// treat the directive as having no depth limit at all, which silently
    /// renders a far larger tree than the author asked for.
    ToctreeMaxdepthInvalid => "toctree.maxdepth-invalid",
    /// A `:numbered:` whose value is present but is not a positive integer.
    ToctreeNumberedInvalid => "toctree.numbered-invalid",
    /// A `:name:` option with no value, so nothing could `:ref:` the toctree.
    ToctreeEmptyName => "toctree.empty-name",
    /// An entry naming a document the project does not contain. Detected while
    /// building the project index, since only there is the document list known.
    ToctreeMissingDocument => "toctree.missing-document",
    /// A `:glob:` pattern that matched no document.
    ToctreeGlobNoMatch => "toctree.glob-no-match",
    /// A document reached by two different toctrees, so it appears twice in the
    /// navigation and only the first position orders it.
    ToctreeDuplicateEntry => "toctree.duplicate-entry",
    /// A document no toctree reaches, so a reader can only arrive at it by
    /// following a cross-reference. Suppress with a leading `:orphan:` field.
    ToctreeOrphanDocument => "toctree.orphan-document",

    // --- `.. contents::` -----------------------------------------------------
    /// A `:depth:` whose value is not a positive integer. Sphinx would treat
    /// the directive as having no depth limit at all, which silently lists a
    /// far deeper tree than the author asked for.
    ContentsDepthInvalid => "contents.depth-invalid",
    /// A `:backlinks:` value other than `entry`, `top` or `none`.
    ContentsBacklinksInvalid => "contents.backlinks-invalid",
    /// A `:name:` option with no value, so nothing could `:ref:` the table of
    /// contents.
    ContentsEmptyName => "contents.empty-name",

    // --- `.. dropdown::` ----------------------------------------------------
    /// A `:color:` naming none of the eleven semantic colours. The summary is
    /// left unpainted rather than given a class no stylesheet defines.
    DropdownInvalidColor => "dropdown.invalid-color",
    /// An `:icon:` naming no octicon. Reported here, while parsing, because
    /// this is where the `:icon:` line can be pointed at — the name itself is
    /// checked against the icon set the renderer draws from.
    DropdownUnknownIcon => "dropdown.unknown-icon",
    /// A `:chevron:` other than `right-down` or `down-up`.
    DropdownInvalidChevron => "dropdown.invalid-chevron",
    /// An `:animate:` other than `fade-in` or `fade-in-slide-down`.
    DropdownInvalidAnimate => "dropdown.invalid-animate",
    /// A `:margin:` that is neither one nor four values, or that names a step
    /// off the 0–5 scale.
    DropdownInvalidMargin => "dropdown.invalid-margin",
    /// An option that needs a value but was written without one.
    DropdownEmptyOptionValue => "dropdown.empty-option-value",

    // --- `.. grid::` / `.. grid-item::` -------------------------------------
    //
    // One family for both directives, as `entity-table.*` is one family for
    // both its spellings: a code names the construct, and a grid and its
    // items are one construct written in two directives.
    /// A `.. grid::` argument or a `:columns:` that is neither one nor four
    /// values, or that names something other than `auto` or a column from 1
    /// to 12. The row or item is rendered without column classes rather than
    /// refused — see `grid.rs` for why a grid never dies of a bad option.
    GridInvalidColumns => "grid.invalid-columns",
    /// A `:gutter:` that is neither one nor four values, or that names a step
    /// off the 0–5 scale. `auto` is a step a column count allows and a gutter
    /// does not, and lands here.
    GridInvalidGutter => "grid.invalid-gutter",
    /// A `:margin:` or `:padding:` that is neither one nor four values, or
    /// that names a step off the scale.
    GridInvalidSpacing => "grid.invalid-spacing",
    /// A `:child-align:` other than `start`, `end`, `center`, `justify` or
    /// `spaced`.
    GridInvalidChildAlign => "grid.invalid-child-align",
    /// A `:child-direction:` other than `column` or `row`.
    GridInvalidChildDirection => "grid.invalid-child-direction",
    /// Content written directly inside a `.. grid::` that is not a
    /// `.. grid-item::`. Reported and *kept*, as sphinx-design keeps it: the
    /// layout is wrong, but dropping the content would be worse.
    GridUnexpectedChild => "grid.unexpected-child",
    /// A `.. grid-item::` written outside any `.. grid::`. Likewise reported
    /// and still rendered.
    GridItemOutsideGrid => "grid.item-outside-grid",
    /// An option that needs a value but was written without one.
    GridEmptyOptionValue => "grid.empty-option-value",

    // --- `.. button-link::` -------------------------------------------------
    //
    // Named for the directive rather than for buttons in general: sphinx-design
    // builds two of them from one base class, and `.. button-ref::` — which
    // this build does not yet have — is a second *construct*, not a second
    // spelling of this one. See `docs/decisions/018-button-link.md`.
    /// A `:color:` naming none of the eleven semantic colours. The button is
    /// left unpainted rather than given a class no stylesheet defines.
    ButtonLinkInvalidColor => "button-link.invalid-color",
    /// An `:align:` other than `left`, `right`, `center` or `justify`.
    ButtonLinkInvalidAlign => "button-link.invalid-align",
    /// A `.. button-link::` written with no URL to point at. The directive is
    /// drawn as an error block quoting its source, as an argument-less
    /// `.. image::` is.
    ButtonLinkMissingTarget => "button-link.missing-target",
    /// An option that needs a value but was written without one.
    ButtonLinkEmptyOptionValue => "button-link.empty-option-value",
    /// An option sphinx-design accepts here that this build refuses by name,
    /// rather than accepting and ignoring it.
    ButtonLinkUnsupportedOption => "button-link.unsupported-option",
    /// An `:outline:` with no `:color:` to outline. sphinx-design produces no
    /// class at all for that pair, so the option silently does nothing —
    /// reported for the reason `uml.unusable-scale` is.
    ButtonLinkUnusableOutline => "button-link.unusable-outline",
    /// A reference role written inside a button's label, which would nest one
    /// link inside another. The reference is rendered as its text alone.
    ButtonLinkNestedReference => "button-link.nested-reference",

    // --- `.. sectnum::` / `.. section-numbering::` --------------------------
    /// A `:depth:` whose value is not a positive integer. Treated as no depth
    /// limit, matching `.. contents::`'s own `:depth:`.
    SectnumDepthInvalid => "sectnum.depth-invalid",
    /// A `:start:` whose value is not a positive integer (`0` included — there
    /// is no sensible number to display at that value). The default of `1`
    /// is kept instead.
    SectnumStartInvalid => "sectnum.start-invalid",
    /// A `:prefix:` option with no value; docutils requires this option to
    /// carry one.
    SectnumEmptyPrefix => "sectnum.empty-prefix",
    /// A `:suffix:` option with no value; docutils requires this option to
    /// carry one.
    SectnumEmptySuffix => "sectnum.empty-suffix",
    /// An argument given to a directive docutils declares zero argument
    /// slots for. Real docutils reports this as a directive error rather
    /// than silently dropping it, so this build does too.
    SectnumUnexpectedArgument => "sectnum.unexpected-argument",

    // --- `.. index::` ------------------------------------------------------
    IndexInvalidPair => "index.invalid-pair",
    IndexInvalidTriple => "index.invalid-triple",
    IndexInvalidSee => "index.invalid-see",
    IndexInvalidSeeAlso => "index.invalid-seealso",

    // --- Signatures --------------------------------------------------------
    /// A `c:function`/`c:type` signature the C declaration parser could not
    /// read, so the name came from the fallback heuristic.
    CDeclUnparsed => "c-decl.unparsed",
    /// An `.. option::` spec that does not look like any recognised form.
    OptionMalformedSpec => "option.malformed-spec",

    // --- Substitution definitions (`.. |name| replace::` and friends) ------
    /// A `|name|` reference matching no substitution definition, in this
    /// document or case-insensitively.
    SubstitutionUndefined => "substitution.undefined",
    /// The same substitution name defined more than once. The first
    /// definition wins; the rest are reported and ignored.
    SubstitutionDuplicateDefinition => "substitution.duplicate-definition",
    /// A `replace` substitution whose content refers back to itself, directly
    /// or through a chain of other substitutions.
    SubstitutionCircularReference => "substitution.circular-reference",
    /// A `unicode` substitution's codepoint token was neither a decimal
    /// number, a recognized hexadecimal form, nor an XML character entity.
    SubstitutionInvalidUnicodeCode => "substitution.invalid-unicode-code",
    /// A `:name:` on an image inside a substitution definition. docutils
    /// refuses this: a substitution may be referenced more than once, but a
    /// name must be unique.
    SubstitutionImageNameNotAllowed => "substitution.image-name-not-allowed",

    // --- Entities (project-declared types), reported by parse and index ----
    /// An option on an entity directive that its type declares as neither an
    /// attribute nor a relation.
    EntityUnknownAttribute => "entity.unknown-attribute",
    /// An attribute value that does not fit its declared type — a word where
    /// an `int` was declared, a value outside an `enum`'s permitted set.
    EntityInvalidAttributeValue => "entity.invalid-attribute-value",
    /// A `required` attribute the entity did not give a value for.
    EntityMissingRequiredAttribute => "entity.missing-required-attribute",
    /// A directive argument a type takes none of, or one with more
    /// comma-separated parts than the type declares fields.
    EntityMalformedArgument => "entity.malformed-argument",
    /// An `:id:` that is not a legal entity id, or one that could not be
    /// derived because a source attribute had no value.
    EntityInvalidId => "entity.invalid-id",
    /// A legal id that does not match its type's declared `id.pattern`. Kept
    /// apart from `entity.invalid-id` because the entity keeps this id and
    /// links to it resolve: what is broken is a naming convention, which a
    /// migrating project may want to silence on its own.
    EntityIdPatternMismatch => "entity.id-pattern-mismatch",
    /// Two entities in the project claiming one id. Found by the index phase,
    /// which is the first to see every document.
    EntityDuplicateId => "entity.duplicate-id",
    /// A relation or role naming an entity that no document declares.
    EntityUnknownTarget => "entity.unknown-target",
    /// A relation target whose type is outside the relation's declared `to`.
    EntityDisallowedRelation => "entity.disallowed-relation",
    /// A `required` relation the entity named no target for.
    EntityMissingRequiredRelation => "entity.missing-required-relation",
    /// Several targets on a relation declared to take exactly one.
    EntityMultipleRelationTargets => "entity.multiple-relation-targets",
    /// A sub-directive inside an entity that its type does not declare as a
    /// section.
    EntityUnknownSection => "entity.unknown-section",
    /// A section directive written outside any entity, which would otherwise
    /// render as nothing at all.
    EntitySectionOutsideEntity => "entity.section-outside-entity",
    /// A section written more than once without being declared `multiple`.
    EntityDuplicateSection => "entity.duplicate-section",
    /// A `required` section the entity did not write.
    EntityMissingRequiredSection => "entity.missing-required-section",
    /// A role reference resolving to an entity whose type the role does not
    /// accept. Links anyway, as a domain-object type mismatch does.
    EntityRoleTypeMismatch => "entity.role-type-mismatch",
    /// A document parsed against a different schema than the one the index is
    /// being built with — a build misconfiguration that would otherwise
    /// produce quietly wrong output.
    EntitySchemaMismatch => "entity.schema-mismatch",

    // --- `.. entity-table::` / `.. needtable::` ----------------------------
    //
    // One family for both spellings of the directive: an author who wrote
    // `.. needtable::` still suppresses with `entity-table.*`, because a code
    // names the construct rather than the name it was written under.
    /// A `:filter:` this build's filter language cannot evaluate — either a
    /// syntax error, or a Python construct it deliberately does not support.
    /// The table still renders, listing everything, rather than vanishing.
    EntityTableInvalidFilter => "entity-table.invalid-filter",
    /// A `:filter:`, `:columns:` or `:sort:` naming a field no entity type
    /// declares and that is none of the built-in ones. Reported while parsing,
    /// where the schema is already in hand, rather than silently matching or
    /// showing nothing.
    EntityTableUnknownField => "entity-table.unknown-field",
    /// Both `:widths:` and its sphinx-needs spelling `:colwidths:` given at
    /// once. Neither is guessed at, since the two disagreeing is exactly the
    /// case where picking one silently renders the wrong table.
    EntityTableDuplicateWidths => "entity-table.duplicate-widths",
    /// A `:style:` this build has no rendering for — `datatables`, which is a
    /// JavaScript feature. A static table is rendered instead.
    EntityTableUnsupportedStyle => "entity-table.unsupported-style",
    /// A table whose filter matched no entity at all. Reported for the reason
    /// `.. literalinclude::` reports every way of selecting nothing: an empty
    /// table is far more often a mistaken filter than an intended statement.
    EntityTableEmptyResult => "entity-table.empty-result",

    // --- `.. entity-flow::` / `.. needflow::` ------------------------------
    //
    // One family for both spellings, as `entity-table.*` is one for its own
    // pair. Its own family rather than `uml.*`, even though a flowchart ends
    // as a compiled PlantUML picture like every diagram does: what can go
    // wrong here is a *question* about the entity graph, not a template, and
    // `uml.invalid-filter` on a directive holding no template names a
    // construct that is not there.
    /// A `:filter:` this build's filter language cannot evaluate. The
    /// flowchart still draws, showing everything, for the reason
    /// `entity-table.invalid-filter` still lists everything.
    EntityFlowInvalidFilter => "entity-flow.invalid-filter",
    /// A `:filter:` naming a field no entity type declares and that is none of
    /// the built-in ones. Reported while parsing, where the schema is in hand.
    EntityFlowUnknownField => "entity-flow.unknown-field",
    /// A `:relations:` entry naming a relation no entity type declares. The
    /// remaining entries are still drawn, since one misspelling should not
    /// cost every edge.
    EntityFlowUnknownRelation => "entity-flow.unknown-relation",
    /// A `:direction:` naming a layout `PlantUML` cannot draw. The default is
    /// used instead.
    EntityFlowInvalidDirection => "entity-flow.invalid-direction",
    /// An `:align:` that is not one of docutils' three.
    EntityFlowInvalidAlign => "entity-flow.invalid-align",
    /// A `:scale:` that is not a non-negative percentage.
    EntityFlowInvalidScale => "entity-flow.invalid-scale",
    /// A `:width:` that is not a length or a percentage.
    EntityFlowInvalidWidth => "entity-flow.invalid-width",
    /// A `:scale:` with no `:width:` to apply to. Reported for the reason
    /// `uml.unusable-scale` is: the compiled SVG is never opened while
    /// rendering, so it has no size of its own to scale.
    EntityFlowUnusableScale => "entity-flow.unusable-scale",
    /// An option whose value is required but was left empty.
    EntityFlowEmptyOptionValue => "entity-flow.empty-option-value",
    /// An option sphinx-needs' `needflow` has that this build does not
    /// implement. Reported by name rather than ignored, so an author who asked
    /// for a legend learns they did not get one.
    EntityFlowUnsupportedOption => "entity-flow.unsupported-option",
    /// A flowchart whose filter matched no entity at all, so there was nothing
    /// to draw. Reported for the reason `entity-table.empty-result` is, and it
    /// has to be: `PlantUML` rejects an empty diagram, so compiling one would
    /// fail the build with a syntax error naming a generated file.
    EntityFlowEmptyResult => "entity-flow.empty-result",
    /// A `:config:` naming a preamble the site config does not declare.
    EntityFlowUnknownConfig => "entity-flow.unknown-config",
    /// A flowchart written in a library that did not opt in to diagrams.
    /// Fails the parse, as `uml.diagrams-disabled` does and for the same
    /// reason: the picture would silently never be compiled.
    EntityFlowDiagramsDisabled => "entity-flow.diagrams-disabled",

    // --- `.. entity-sequence::` / `.. needsequence::` ----------------------
    //
    // One family for both spellings, and its own rather than `entity-flow.*`
    // for the reason that one is not `uml.*`: a code names the construct, and a
    // sequence diagram's question — a walk from start entities along message
    // relations — is not a flowchart's.
    /// A sequence diagram with no `:start:`, or one listing no entity. There is
    /// nowhere to begin the walk, so the directive degrades to an error block.
    EntitySequenceMissingStart => "entity-sequence.missing-start",
    /// A `:start:` entry that is not a well-formed entity id. The remaining
    /// entries are still walked.
    EntitySequenceInvalidStart => "entity-sequence.invalid-start",
    /// A sequence diagram with no `:relations:`/`:link_types:`. Mandatory,
    /// unlike sphinx-needs' default of `links`: neither that name nor "every
    /// relation the schema declares" describes which edges are messages.
    EntitySequenceMissingRelations => "entity-sequence.missing-relations",
    /// A `:relations:` entry naming a relation no entity type declares. The
    /// remaining entries are still walked.
    EntitySequenceUnknownRelation => "entity-sequence.unknown-relation",
    /// A `:filter:` this build's filter language cannot evaluate. The diagram
    /// still draws, keeping every receiver.
    EntitySequenceInvalidFilter => "entity-sequence.invalid-filter",
    /// A `:filter:` naming a field no entity type declares and that is none of
    /// the built-in ones.
    EntitySequenceUnknownField => "entity-sequence.unknown-field",
    /// A `:max-items:` that is not a non-negative whole number.
    EntitySequenceInvalidMaxItems => "entity-sequence.invalid-max-items",
    /// An `:align:` that is not one of docutils' three.
    EntitySequenceInvalidAlign => "entity-sequence.invalid-align",
    /// A `:scale:` that is not a non-negative percentage.
    EntitySequenceInvalidScale => "entity-sequence.invalid-scale",
    /// A `:width:` that is not a length or a percentage.
    EntitySequenceInvalidWidth => "entity-sequence.invalid-width",
    /// A `:scale:` with no `:width:` to apply to, for `uml.unusable-scale`'s
    /// reason.
    EntitySequenceUnusableScale => "entity-sequence.unusable-scale",
    /// An option whose value is required but was left empty.
    EntitySequenceEmptyOptionValue => "entity-sequence.empty-option-value",
    /// An option sphinx-needs' `needsequence` has that this build does not
    /// implement, reported by name rather than ignored.
    EntitySequenceUnsupportedOption => "entity-sequence.unsupported-option",
    /// A `:start:` entry naming no entity in the project. The other starts are
    /// still walked — sphinx-needs aborts the whole build here instead.
    EntitySequenceUnknownStart => "entity-sequence.unknown-start",
    /// A walk that found no message at all, so there was nothing to draw.
    /// `PlantUML` would otherwise be handed a picture of lone lifelines.
    EntitySequenceEmptyResult => "entity-sequence.empty-result",
    /// A `:config:` naming a preamble the site config does not declare.
    EntitySequenceUnknownConfig => "entity-sequence.unknown-config",
    /// A walk that found more messages than `:max-items:` allows, so the
    /// picture shows only the first ones. Reported as well as noted on the
    /// page, as sphinx-needs does, so whoever runs the build learns of it
    /// without reading every page; a deliberate cap is silenced with `.. noqa:`.
    EntitySequenceTruncated => "entity-sequence.truncated",
    /// A sequence diagram written in a library that did not opt in to
    /// diagrams, for `entity-flow.diagrams-disabled`'s reason.
    EntitySequenceDiagramsDisabled => "entity-sequence.diagrams-disabled",

    // --- `.. entity-pie::` / `.. needpie::` --------------------------------
    //
    // One family for both spellings, as the two families above are for their
    // own pairs, and its own family for the reason `entity-flow.*` is not
    // `uml.*`: a code names the construct. Nothing here is `uml.*` in any
    // case — a pie is drawn as SVG by the render action, never compiled, so
    // it has no `:config:`, no hash and no `diagrams-disabled`.
    /// A slice's filter, or the chart's own `:filter:`, that this build's
    /// filter language cannot evaluate. The slice still counts, selecting
    /// everything, for the reason `entity-table.invalid-filter` still lists
    /// everything: the diagnostic says what is wrong, and an empty wedge on
    /// top of it would hide what the author was reaching for.
    EntityPieInvalidFilter => "entity-pie.invalid-filter",
    /// A filter naming a field no entity type declares and that is none of the
    /// built-in ones. Reported while parsing, where the schema is in hand.
    EntityPieUnknownField => "entity-pie.unknown-field",
    /// A chart whose body holds no content line at all, so there is nothing to
    /// count and no chart to draw. Reported while parsing, unlike
    /// `entity-pie.empty-result`, because the body is this document's own text.
    EntityPieNoSlices => "entity-pie.no-slices",
    /// More or fewer `:labels:` than content lines. They pair by position, so
    /// a mismatch means at least one wedge is named wrongly or not at all —
    /// the wedges are still drawn, and the surplus labels dropped.
    EntityPieLabelCountMismatch => "entity-pie.label-count-mismatch",
    /// A `:colors:` entry that is not a colour this build can draw with. The
    /// built-in palette is used for that wedge instead.
    EntityPieInvalidColor => "entity-pie.invalid-color",
    /// An `:align:` that is not one of docutils' three.
    EntityPieInvalidAlign => "entity-pie.invalid-align",
    /// A `:scale:` that is not a non-negative percentage.
    EntityPieInvalidScale => "entity-pie.invalid-scale",
    /// A `:width:` that is not a length or a percentage.
    EntityPieInvalidWidth => "entity-pie.invalid-width",
    /// A `:scale:` with no `:width:` to apply to. The chart is drawn at a size
    /// this build chooses, so a bare percentage has nothing to scale.
    EntityPieUnusableScale => "entity-pie.unusable-scale",
    /// An option whose value is required but was left empty.
    EntityPieEmptyOptionValue => "entity-pie.empty-option-value",
    /// An option sphinx-needs' `needpie` has that this build does not
    /// implement. Reported by name rather than ignored, so an author who asked
    /// for exploded wedges learns they did not get them.
    EntityPieUnsupportedOption => "entity-pie.unsupported-option",
    /// A chart every one of whose wedges counted zero, so there is no chart to
    /// draw at all. Reported for the reason `entity-table.empty-result` is,
    /// and only the renderer can raise it: whether a filter selects anything
    /// depends on every document in the project.
    EntityPieEmptyResult => "entity-pie.empty-result",

    // --- `.. entity-bar::` / `.. needbar::` --------------------------------
    //
    // One family for both spellings, and its own rather than `entity-pie.*`'s
    // although most codes mirror it: a code names the construct. Like a pie,
    // a bar chart is drawn by the render action and never compiled, so there
    // is no `diagrams-disabled` here either.
    /// A cell's filter, or the chart's own `:filter:`, that this build's
    /// filter language cannot evaluate. The cell still counts, selecting
    /// everything, for `entity-pie.invalid-filter`'s reason.
    EntityBarInvalidFilter => "entity-bar.invalid-filter",
    /// A filter naming a field no entity type declares and that is none of the
    /// built-in ones. Reported while parsing, where the schema is in hand.
    EntityBarUnknownField => "entity-bar.unknown-field",
    /// A chart whose body holds no values at all once label rows and columns
    /// are taken out. Reported while parsing, because the body is this
    /// document's own text.
    EntityBarNoData => "entity-bar.no-data",
    /// A content line with a different number of cells from the first. The
    /// grid is widened to its longest row with zero cells, rather than
    /// refused as sphinx-needs does, so the data written is still drawn.
    EntityBarRaggedRow => "entity-bar.ragged-row",
    /// More or fewer `:xlabels:`/`:ylabels:` than the grid has columns/rows.
    /// They pair by position, so at least one bar is named wrongly; the bars
    /// are still drawn and the surplus labels dropped.
    EntityBarLabelCountMismatch => "entity-bar.label-count-mismatch",
    /// A `:colors:` or `:text_color:` entry that is not a colour this build
    /// can draw with.
    EntityBarInvalidColor => "entity-bar.invalid-color",
    /// A `:xlabels_rotation:`, `:ylabels_rotation:` or `:sum_rotation:` that
    /// is not a whole number of degrees. sphinx-needs silently ignores one;
    /// here the text is drawn unrotated and the line reported.
    EntityBarInvalidRotation => "entity-bar.invalid-rotation",
    /// An `:align:` that is not one of docutils' three.
    EntityBarInvalidAlign => "entity-bar.invalid-align",
    /// A `:scale:` that is not a non-negative percentage.
    EntityBarInvalidScale => "entity-bar.invalid-scale",
    /// A `:width:` that is not a length or a percentage.
    EntityBarInvalidWidth => "entity-bar.invalid-width",
    /// A `:scale:` with no `:width:` to apply to.
    EntityBarUnusableScale => "entity-bar.unusable-scale",
    /// An option whose value is required but was left empty.
    EntityBarEmptyOptionValue => "entity-bar.empty-option-value",
    /// An option sphinx-needs' `needbar` has that this build does not
    /// implement, reported by name with what to write instead.
    EntityBarUnsupportedOption => "entity-bar.unsupported-option",
    /// A chart every one of whose cells counted zero, so there are no bars to
    /// draw. Only the renderer can raise it, for `entity-pie.empty-result`'s
    /// reason.
    EntityBarEmptyResult => "entity-bar.empty-result",

    // --- `.. entity-update::` / `.. needextend::` --------------------------
    //
    // One family for both spellings, as `entity-table.*` is one for its own
    // pair — the argument's own name rather than `needextend.*`, since this
    // build owns the construct (see `docs/decisions/019-entity-update.md`).
    /// An argument that is neither a legal entity id nor a filter this build
    /// can evaluate — including an empty one.
    EntityUpdateInvalidArgument => "entity-update.invalid-argument",
    /// An option naming `id`, `type`, `type_name`, `docname` or `title` —
    /// identity fields this directive refuses to mutate.
    EntityUpdateProtectedField => "entity-update.protected-field",
    /// An option naming a *declared section*, not an attribute or relation.
    /// Sections are documents, deliberately excluded from the index this
    /// directive's effects live in, so they cannot be mutated this way.
    EntityUpdateSectionNotSupported => "entity-update.section-not-supported",
    /// An option naming a field no entity type declares at all.
    EntityUpdateUnknownField => "entity-update.unknown-field",
    /// A field this directive matched an entity for, but that entity's own
    /// declared type does not have.
    EntityUpdateFieldNotApplicable => "entity-update.field-not-applicable",
    /// A value that does not fit its matched field's declared type.
    EntityUpdateInvalidValue => "entity-update.invalid-value",
    /// A `+`/`-` operation on a field that is not list-valued.
    EntityUpdateListOperationOnScalar => "entity-update.list-operation-on-scalar",
    /// A `:strict:` value that is not a recognised boolean spelling.
    EntityUpdateInvalidStrict => "entity-update.invalid-strict",
    /// A `.. entity-update::`/`.. needextend::` whose target matched no
    /// entity at all, with `:strict:` at its default. Reported for the reason
    /// `entity-table.empty-result` is: a selection matching nothing is far
    /// likelier to be a mistake than an intention.
    EntityUpdateEmptyResult => "entity-update.empty-result",
    /// A `Set`/`Clear` that overwrote a *different* value a different
    /// directive had already established for the same field of the same
    /// entity. Reported once per involved directive, each under its own span
    /// in its own file, so either author can independently suppress their
    /// own copy — see `docs/decisions/019-entity-update.md`.
    EntityUpdateConflictingUpdate => "entity-update.conflicting-update",

    // --- `.. needimport::` -------------------------------------------------
    //
    // The one family named after another tool's spelling rather than after a
    // name of this build's own, because here the spelling *is* the construct:
    // `.. needimport::` exists to read sphinx-needs' `needs.json` and nothing
    // else, and the richer import this project may grow later should be free
    // to take the `entity-import` name and its own family with it. See
    // `docs/decisions/016-needimport.md`.
    /// A `.. needimport::` with no file to read.
    NeedImportMissingPath => "needimport.missing-path",
    /// An argument naming an `http`/`https` URL. Refused by name rather than
    /// fetched: a sandboxed build action may only read files declared before
    /// it runs, so a download would either fail or make the build
    /// unreproducible.
    NeedImportRemoteSource => "needimport.remote-source",
    /// An argument that is not a path to a `.json` file at all. sphinx-needs
    /// resolves a bare name through `needs_import_keys` in `conf.py` — a
    /// config this build has no equivalent of — so the name is refused by
    /// name rather than opened as a file and reported as missing.
    NeedImportUnsupportedImportKey => "needimport.unsupported-import-key",
    /// A `needs.json` that could not be read. Fails the build, the way every
    /// unreadable parse-time file does.
    NeedImportFileUnreadable => "needimport.file-unreadable",
    /// A file that is not the JSON this directive reads.
    NeedImportMalformedJson => "needimport.malformed-json",
    /// No single version block could be chosen — a `:version:` naming one the
    /// file does not hold, a dangling `current_version`, or several versions
    /// with nothing naming one.
    NeedImportUnknownVersion => "needimport.unknown-version",
    /// A need whose `type` names no declared entity type. The need is skipped,
    /// since there is no vocabulary to read its fields against.
    NeedImportUnknownType => "needimport.unknown-type",
    /// A need carrying a field that is neither one of sphinx-needs' own
    /// bookkeeping keys nor an attribute or relation the type declares.
    NeedImportUnknownField => "needimport.unknown-field",
    /// A field whose JSON value has no spelling an option could have been
    /// written with — an object, a null, or a list holding either.
    NeedImportInvalidValue => "needimport.invalid-value",
    /// A need whose `id` is missing or is not a legal entity id. Skipped: an
    /// entity with no id cannot be indexed or referred to.
    NeedImportInvalidId => "needimport.invalid-id",
    /// An `:ids:` entry naming a need the chosen version does not hold.
    NeedImportUnknownId => "needimport.unknown-id",
    /// A `:filter:` this build's filter language could not parse. Listed
    /// separately from the field check for the reason
    /// `entity-table.invalid-filter` is.
    NeedImportInvalidFilter => "needimport.invalid-filter",
    /// A `:filter:` naming a field no entity type declares and that is none of
    /// the built-ins.
    NeedImportUnknownFilterField => "needimport.unknown-filter-field",
    /// An option sphinx-needs accepts that this build must refuse — the
    /// presentation and templating family. Refused *by name*, with what to
    /// write instead, since the common failure here is an unsupported feature
    /// rather than a typo.
    NeedImportUnsupportedOption => "needimport.unsupported-option",
    /// An option `.. needimport::` does not accept at all.
    NeedImportUnknownOption => "needimport.unknown-option",
    /// A `:tags:` written for a type declaring no list attribute called
    /// `tags`. Nothing in this model privileges that name, so there is nowhere
    /// to put the values.
    NeedImportNoTagsAttribute => "needimport.no-tags-attribute",
    /// An import that contributed no entity at all, because `:ids:` or
    /// `:filter:` selected none. Reported for the reason
    /// `entity-table.empty-result` is: a selection matching nothing is far
    /// likelier to be a mistake than an intention.
    NeedImportEmptyResult => "needimport.empty-result",

    // --- `.. needservice::` ------------------------------------------------
    //
    // sphinx-needs' name alone, as `needimport.*` is: this build owns no
    // construct here, so the spelling is the construct.
    /// A `.. needservice::`, refused by name: it queries an external service
    /// while building, which a sandboxed action cannot do. The message points
    /// at a `needs.json` snapshot read by `.. needimport::` instead.
    NeedServiceUnsupported => "needservice.unsupported",

    // --- Diagram directives ------------------------------------------------
    //
    // One family for all six spellings — `.. plantuml::`/`.. uml::`,
    // `.. entity-diagram::`/`.. needuml::` and `.. entity-arch::`/`..
    // needarch::` — for the reason the `entity-table` family covers both of
    // its own: a code names the construct, and these six produce one node
    // whose failures are the same failures. `uml` rather than
    // `entity-diagram`, because the family has to name the plain PlantUML
    // spellings too, and those have no entity in them.
    /// An option no diagram directive accepts. The diagram still renders:
    /// refusing it over a misspelled option would silently drop the picture.
    UmlUnknownOption => "uml.unknown-option",
    /// An option whose value is required but was written empty, so the option
    /// was ignored.
    UmlEmptyOptionValue => "uml.empty-option-value",
    /// An `:align:` that is not one of the three horizontal placements. The
    /// diagram renders unaligned rather than not at all.
    UmlInvalidAlign => "uml.invalid-align",
    /// A `:scale:` that is not a non-negative percentage, by the same rule
    /// `.. image::` reads one.
    UmlInvalidScale => "uml.invalid-scale",
    /// A `:width:` that is not a length or a percentage.
    UmlInvalidWidth => "uml.invalid-width",
    /// A `:scale:` with no `:width:` for it to apply to. Reported for the same
    /// reason `.. image::` reports one: this build never opens the picture
    /// while rendering, so there is no natural size to scale, and dropping an
    /// author's explicit instruction in silence is what diagnostics exist to
    /// prevent.
    UmlUnusableScale => "uml.unusable-scale",
    /// An `:extra:` that is not a comma-separated list of `name: value` pairs.
    /// The pairs that did parse are still bound.
    UmlInvalidExtra => "uml.invalid-extra",
    /// An option that only means something for a templated diagram — `:key:`,
    /// `:extra:` — written on a plain `.. plantuml::`, where nothing would
    /// ever read it.
    UmlOptionNeedsTemplate => "uml.option-needs-template",
    /// A template that is not valid Jinja, or whose evaluation failed. The
    /// diagram is left out rather than compiled from half-expanded text, which
    /// would fail in `PlantUML` with a message about a construct the author
    /// never wrote.
    UmlTemplateError => "uml.template-error",
    /// A template asking for an entity no document declares. Reported while
    /// rendering, because only then is the whole project's entity graph known.
    UmlUnknownEntity => "uml.unknown-entity",
    /// A `filter()` this build's filter language cannot evaluate — the same
    /// language, and the same refusals, a listing directive's `:filter:` uses.
    UmlInvalidFilter => "uml.invalid-filter",
    /// An `.. entity-arch::` written outside any entity, where the `need` it
    /// exists to draw would be bound to nothing.
    UmlArchOutsideEntity => "uml.arch-outside-entity",
    /// A `uml()` import that reaches itself, directly or through a cycle.
    UmlRecursiveImport => "uml.recursive-import",
    /// A diagram whose expansion drew nothing — most often a `filter()` that
    /// matched no entity.
    ///
    /// Reported for the reason `entity-table.empty-result` is, and handled the
    /// same way: the page simply shows no picture. It has to be *reported*
    /// rather than compiled, because `PlantUML` refuses an empty diagram and
    /// would fail the whole build with a message about a generated file the
    /// author never wrote.
    UmlEmptyResult => "uml.empty-result",
    /// A diagram in a library that did not opt in with `diagrams = True`.
    ///
    /// Diagram compilation is opt-in per library so that a project drawing
    /// nothing pays nothing for it — Bazel cannot know which documents hold a
    /// diagram before reading them, so the build must be told. This is what
    /// keeps the opt-in honest: a forgotten attribute fails on the directive's
    /// own line rather than shipping a page with a broken picture.
    UmlDiagramsDisabled => "uml.diagrams-disabled",
    /// A `:save:`, which cannot be honoured: a sandboxed build action may only
    /// write files declared before it runs, and this path is written inside the
    /// document. The site's `diagram_sources` output group carries every
    /// diagram's expanded source instead.
    UmlSaveUnsupported => "uml.save-unsupported",
    /// A `:config:` naming a `PlantUML` preamble the site config does not
    /// declare. Refused rather than ignored: a diagram silently missing the
    /// styling its author asked for looks finished and is wrong.
    UmlUnknownConfig => "uml.unknown-config",

    // --- Jinja-templated sources -------------------------------------------
    /// A document or template that is not valid Jinja. Reported rather than
    /// rendered: the parse then runs on the *unrendered* text, so the page is
    /// still built, but every `{% %}` in it stays visible as the tell.
    JinjaSyntax => "jinja.syntax",
    /// A template reading a name bound by neither a `{% set %}` nor the
    /// library's `jinja_context`. Jinja2 would substitute the empty string;
    /// a page silently missing a value it asked for looks finished and is
    /// wrong.
    JinjaUndefinedValue => "jinja.undefined-value",
    /// A template that failed to render for a reason that is neither a syntax
    /// error nor an undefined value.
    JinjaRenderError => "jinja.render-error",
    /// An `{% include %}` naming a template the build did not declare. Carries
    /// the loader's own explanation, which names the `parse_data` attribute
    /// the file belongs in.
    JinjaTemplateNotFound => "jinja.template-not-found",
    /// A whitespace-control modifier (`{%-`, `-%}`), which this build refuses:
    /// tracking which line a rendered line came from means injecting markers
    /// that a trim would swallow, and in reStructuredText a silently changed
    /// indent changes what a block contains.
    JinjaWhitespaceControl => "jinja.whitespace-control",
    /// An `{% include %}` whose target is computed rather than written. Every
    /// file an action reads is declared before it runs, so a name that only
    /// exists mid-render could never be one of them.
    JinjaDynamicTemplateName => "jinja.dynamic-template-name",
    /// An `{% include %}` chain that reaches a template already being
    /// rendered. Reported with the whole chain, because the offending edit may
    /// be in any link of it.
    JinjaRecursiveInclude => "jinja.recursive-include",

    // --- `.. if-builder::` (sphinx-simplepdf) -------------------------------
    /// The directive has no argument, so it names no builder to compare
    /// against. Upstream gets docutils' generic "1 argument(s) required" here;
    /// a code of its own lets a document suppress this without suppressing
    /// every other malformed directive.
    IfBuilderMissingBuilder => "if-builder.missing-builder",
    /// An argument that names no builder this build knows — including a
    /// multi-word one, since the directive takes the rest of its line.
    /// sphinx-simplepdf excludes the body silently, which makes a typo
    /// indistinguishable from a deliberate exclusion and deletes content with
    /// nothing to grep for.
    IfBuilderUnknownBuilder => "if-builder.unknown-builder",
    /// A block whose builder *matches* but which has no content, so the
    /// directive contributes nothing where it was written to contribute
    /// something. A non-matching block is silent — emptiness only means a
    /// mistake on the branch that was selected.
    IfBuilderEmptyBody => "if-builder.empty-body",

    // --- The suppression mechanism itself ----------------------------------
    /// A `.. noqa:` comment naming an id that is not a diagnostic code. Never
    /// suppressible: a suppression that silences the report of its own typo
    /// would be unfixable.
    NoqaUnknownCode => "noqa.unknown-code",
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The error [`DiagnosticCode::from_str`] returns for an unrecognised id,
/// carrying the text that was written so a diagnostic can quote it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownDiagnosticCode(pub String);

impl fmt::Display for UnknownDiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown diagnostic code '{}'", self.0)
    }
}

impl std::error::Error for UnknownDiagnosticCode {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_every_code_round_trips_through_its_id() {
        // Given every declared code
        for code in DiagnosticCode::ALL {
            // When it is written out and read back
            let parsed = DiagnosticCode::from_str(code.as_str());

            // Then — the two directions are exact inverses.
            assert_eq!(parsed.as_ref(), Ok(code), "{code} did not round-trip");
        }
    }

    #[test]
    fn test_every_id_is_unique() {
        // Given
        let ids: HashSet<&str> = DiagnosticCode::ALL.iter().map(|c| c.as_str()).collect();

        // When / Then — a duplicated id would make `from_str` unable to pick
        // the right variant, silently suppressing the wrong diagnostic.
        assert_eq!(ids.len(), DiagnosticCode::ALL.len());
    }

    #[test]
    fn test_every_id_is_dotted_and_lowercase() {
        // Given / When / Then — the documented shape: `group.kebab-case-name`.
        for code in DiagnosticCode::ALL {
            let id = code.as_str();
            assert!(id.contains('.'), "{id} is not namespaced");
            assert_eq!(id, id.to_lowercase(), "{id} is not lowercase");
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || matches!(c, '.' | '-' | '0'..='9')),
                "{id} contains an unexpected character",
            );
        }
    }

    #[test]
    fn test_display_matches_as_str() {
        // Given
        let code = DiagnosticCode::LinkBrokenRef;

        // When / Then
        assert_eq!(code.to_string(), code.as_str());
        assert_eq!(code.to_string(), "link.broken-ref");
    }

    #[test]
    fn test_from_str_rejects_an_unknown_id() {
        // Given a plausible typo of a real code
        let result = DiagnosticCode::from_str("link.brokenref");

        // When / Then — the written text comes back for quoting.
        assert_eq!(
            result,
            Err(UnknownDiagnosticCode("link.brokenref".to_string()))
        );
    }

    #[test]
    fn test_unknown_code_error_quotes_the_written_text() {
        // Given
        let error = UnknownDiagnosticCode("nope".to_string());

        // When / Then
        assert_eq!(error.to_string(), "unknown diagnostic code 'nope'");
    }

    #[test]
    fn test_code_serialization_roundtrip() {
        // Given
        let code = DiagnosticCode::TableGridNoColumns;

        // When
        let json = serde_json::to_string(&code).expect("Failed to serialize");
        let deserialized: DiagnosticCode =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then — the wire form is the documented dotted id, the same string an
        // author writes in a `.. noqa:`, not the Rust variant name
        assert_eq!(json, "\"table.grid.no-columns\"");
        assert_eq!(code, deserialized);
    }
}
