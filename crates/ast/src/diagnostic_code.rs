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
    /// A domain-object role that resolved only via an object-type alias — the
    /// definition's own type differs from the one the role asked for.
    LinkTypeMismatch => "link.type-mismatch",

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
    DirectiveToctreeUnknownOption => "directive.toctree-unknown-option",
    DirectiveContentsUnknownOption => "directive.contents-unknown-option",
    DirectiveSectnumUnknownOption => "directive.sectnum-unknown-option",
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
