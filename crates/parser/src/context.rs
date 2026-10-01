//! Parse-time configuration threaded through every block-level parser.
//!
//! Three things every nested parse needs to know: the domain a *bare* directive
//! or role resolves to, how to obtain the contents of a file a directive names
//! (`.. csv-table::`'s `:file:`, and the sources `.. include::` and
//! `.. literalinclude::` splice in), and where the lines it is walking sit in
//! the source. They travel together in a [`ParseCtx`] rather than as separate
//! parameters, so adding the next piece of parse-time configuration doesn't
//! touch two dozen signatures again.
//!
//! The loader is an injected trait object rather than a direct
//! `std::fs::read_to_string` call because this crate performs no I/O of its
//! own: it is also the live-preview path, where a document is parsed straight
//! from an editor buffer, and it is exercised by unit tests that must not
//! depend on the filesystem. `rinx` supplies the real
//! filesystem-backed loader.

use std::collections::BTreeMap;

use rinx_ast::{Domain, EntityId, FileId, Position, Span};
use rinx_entity::{EntitySchema, EntityType};

use crate::custom_roles::CustomRoles;
use crate::templating::TemplateMap;

/// A file read at parse time, and the identity the parser knows it by.
///
/// `id` is the loader's resolved, canonical name for the file — a path
/// relative to the source root — as opposed to `path` as the author wrote it,
/// which may be relative to whichever file the directive appeared in. Two
/// things need that canonical form: a nested `.. include::` resolves against
/// the file it is *written in*, and cycle detection compares files for
/// identity. Neither can work off the written text, since `../a/x.rst` and
/// `x.rst` may well be the same file.
pub struct LoadedFile {
    /// The resolved, source-root-relative path.
    pub id: String,
    /// The file's decoded contents.
    pub text: String,
}

/// Supplies the contents of a file named by a directive option or argument.
///
/// `path` is exactly the text the author wrote (e.g. `data/fruits.csv`);
/// resolving it is the implementation's job. `relative_to` is the [`id`] of
/// the file the directive was written in, or `None` for the document being
/// parsed — a nested include resolves against its own directory, as docutils
/// does. The error string is surfaced verbatim as a parse diagnostic, so it
/// should read as an explanation to the document's author.
///
/// [`id`]: LoadedFile::id
pub trait ParseFileLoader {
    /// # Errors
    ///
    /// Returns a human-readable explanation when the file cannot be read.
    fn load(&self, path: &str, relative_to: Option<&str>) -> Result<LoadedFile, String>;
}

/// The default loader: refuses every request.
///
/// Used wherever no filesystem context exists — [`crate::parse`], the legacy
/// `process_rst()` path, and every unit test — so that a file-reading option in
/// those contexts produces an honest diagnostic instead of silently reading
/// something relative to the process's working directory.
pub struct RejectParseFiles;

impl ParseFileLoader for RejectParseFiles {
    fn load(&self, path: &str, _relative_to: Option<&str>) -> Result<LoadedFile, String> {
        Err(format!(
            "cannot read '{path}': this parse was given no directory to resolve it against"
        ))
    }
}

/// Where the line slice a parser is currently walking sits in the original
/// document, so a local `(line, column)` can be translated back to a real
/// source position.
///
/// Both fields are the 1-based position, *in the original document*, of
/// `lines[0]`'s first character. A nested parse of a directive body or a list
/// item runs over a freshly built `Vec<&str>` whose indices start at zero
/// again, and several of those also strip a common indent — so a line offset
/// alone is not enough, and the column must be carried too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Origin {
    line: u32,
    column: u32,
    /// The column `lines[0]` starts at instead, when it differs from the
    /// lines below it — a directive whose content begins on its own marker
    /// line, after the `::`, while the rest is indented beneath it. See
    /// [`ParseCtx::hanging`].
    first_line_column: Option<u32>,
}

impl Origin {
    /// The start of a document or file: line 1, column 1.
    const START: Self = Self {
        line: 1,
        column: 1,
        first_line_column: None,
    };
}

/// A position, and the file it was measured in.
///
/// The two travel together because a [`TemplateMap`] answers both at once: the
/// line a rendered line came from and the template it was written in are one
/// lookup, and separating them would let a span carry one file's line number
/// under another file's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourcePoint {
    /// Where in that file the position is.
    pub(crate) position: Position,
    /// The file, or `None` for the document being parsed.
    file: Option<FileId>,
}

impl SourcePoint {
    /// The span from here to `end`, measured in this point's file.
    ///
    /// The only way a [`Span`] is built in this crate, so that a span cannot
    /// be measured in one file and named after another: every span-building
    /// helper — [`ParseCtx::line_span`], the simple-table context's, the
    /// inline source map's — ends here, and the file arrives already attached
    /// to the point [`ParseCtx::position`] resolved.
    #[must_use]
    pub(crate) const fn to(self, end: Self) -> Span {
        Span::new(self.position, end.position).with_file(self.file)
    }
}

/// Configuration for one parse, borrowed by every block-level parser.
pub struct ParseCtx<'a> {
    /// The domain a bare (unprefixed) directive or role resolves to.
    pub default_domain: Domain,
    /// How to read a file named by a directive.
    pub files: &'a dyn ParseFileLoader,
    /// The project's entity meta-model, which is what tells this parse that
    /// `.. req::` is a directive at all.
    ///
    /// Empty for a project that declares no entities, in which case every
    /// entity lookup misses and parsing behaves exactly as it did before the
    /// feature existed.
    pub schema: &'a EntitySchema,
    /// The document being parsed, as the project knows it.
    ///
    /// Needed only by generated entity ids, which hash it so that the first
    /// unnamed `.. req::` of two different documents cannot collide. Set by
    /// [`crate::parse_with_ctx`] from the path it is already given, so no
    /// caller has to remember to supply it twice.
    pub doc_path: &'a str,
    /// Whether the document's source is rendered as a Jinja template before
    /// it is parsed, and the names such a template may read.
    ///
    /// Off unless the library asked for it: a document is free to contain
    /// `{{` and `{%` as text, and most do not mean Jinja by them. See
    /// [`crate::templating`].
    pub jinja: Option<&'a [(String, String)]>,
    /// The entity type whose body is currently being parsed, if any.
    ///
    /// This is what makes a section sub-directive recognisable *only* inside
    /// the entity that declares it: `.. verification-criteria::` written at
    /// top level matches nothing and is diagnosed, rather than silently
    /// rendering as an unknown directive. Cleared again inside a section's own
    /// body, so sections cannot nest.
    pub enclosing_entity: Option<&'a EntityType>,
    /// The *id* of that same entity, when one is known.
    ///
    /// Kept beside the type rather than derived from it because they answer
    /// different questions and have different lifetimes: the type decides
    /// which section names are legal, and is cleared inside a section's body so
    /// sections cannot nest; the id decides what an `.. entity-arch::` draws,
    /// and must survive into a section body — a diagram written under
    /// `.. verification-criteria::` is still a diagram of the entity it sits
    /// in.
    pub enclosing_entity_id: Option<&'a EntityId>,
    /// Whether the lines being parsed are the body of a `.. grid::`.
    ///
    /// Only a `.. grid-item::` asks: sphinx-design warns when one is written
    /// with any other parent, and this is the parser's way of knowing. It is
    /// *not* cleared by [`Self::inside_entity`] and friends for the same
    /// reason [`Self::enclosing_entity_id`] is not — but it is cleared by an
    /// item's own body, since a grid-item does not make its content a row.
    pub in_grid_row: bool,
    /// Names a `.. needimport::` may write instead of a path, each already
    /// resolved to a source-root-relative file — the schema's `[import_keys]`
    /// table, which is sphinx-needs' `needs_import_keys`.
    ///
    /// Resolved before it gets here: the values are written relative to the
    /// schema file, and only the worker knows where that is. This crate reads
    /// a map it can hand straight to [`ParseFileLoader::load`] with no anchor.
    pub import_keys: &'a BTreeMap<String, String>,
    /// The roles the document has defined so far with `.. role::`.
    ///
    /// Unlike every other field this is *state*, not configuration: a
    /// `.. role::` fills it in and the inline scan of every later paragraph
    /// reads it, which is what makes a role apply from its definition onwards
    /// — see [`crate::custom_roles`]. Set by [`crate::parse_with_ctx`], which
    /// owns one table per document; a context built without it defines no
    /// roles.
    custom_roles: Option<&'a CustomRoles>,
    /// Where the current line slice came from, or `None` when it came from
    /// nowhere in the source — see [`Self::synthetic`].
    origin: Option<Origin>,
    /// The file the current lines were read from, or `None` for the document
    /// itself. Stamped onto every [`Span`] this context builds, which is what
    /// lets a diagnostic about included content name the file it is really in
    /// — see [`Span::file`].
    file: Option<FileId>,
    /// That same file as the loader's id, for resolving a nested include
    /// against the file it is written in.
    ///
    /// Kept beside [`Self::file`] rather than derived from it because the two
    /// answer different questions: an id addresses the *filesystem*, and a
    /// [`FileId`] addresses the document's own table. Only the loader can turn
    /// one into the other.
    current_file: Option<&'a str>,
    /// The ids of the files currently being included, outermost first — the
    /// chain an `.. include::` would extend. A file already in it would close
    /// a cycle, so this is what makes that detectable at all.
    include_stack: &'a [String],
    /// Where each line of the text being parsed came from, when a Jinja pass
    /// rewrote it — see [`crate::templating`].
    ///
    /// Present only on the document's own context. A Jinja pass moves lines
    /// around freely, so unlike [`Self::origin`] this mapping is not an offset
    /// and cannot compose; it is consulted once, at the end of
    /// [`Self::position`], after every nested offset has been added.
    template_map: Option<&'a TemplateMap>,
}

/// The map a context built without a schema points at.
///
/// A borrowed field needs something to borrow, and allocating an empty map per
/// context would be waste — the same reason [`EntitySchema::empty_ref`] exists.
fn empty_import_keys() -> &'static BTreeMap<String, String> {
    static EMPTY: std::sync::OnceLock<BTreeMap<String, String>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(BTreeMap::new)
}

impl<'a> ParseCtx<'a> {
    /// A context that resolves bare constructs in `default_domain` and has no
    /// filesystem access.
    #[must_use]
    pub fn with_domain(default_domain: Domain) -> Self {
        Self::new(default_domain, &RejectParseFiles)
    }

    /// A context that resolves bare constructs in `default_domain` and reads
    /// files through `files`.
    #[must_use]
    pub fn new(default_domain: Domain, files: &'a dyn ParseFileLoader) -> Self {
        Self {
            default_domain,
            files,
            schema: EntitySchema::empty_ref(),
            doc_path: "",
            jinja: None,
            enclosing_entity: None,
            enclosing_entity_id: None,
            in_grid_row: false,
            import_keys: empty_import_keys(),
            custom_roles: None,
            origin: Some(Origin::START),
            file: None,
            current_file: None,
            include_stack: &[],
            template_map: None,
        }
    }

    /// The same context, parsing the document at `doc_path`.
    #[must_use]
    pub(crate) fn for_document(&self, doc_path: &'a str) -> Self {
        Self { doc_path, ..*self }
    }

    /// The same context, recording and reading the document's `.. role::`
    /// definitions in `roles`.
    ///
    /// Borrows for a possibly shorter lifetime than `'a` because the caller
    /// owns the table for the length of one parse, as [`Self::included`]'s
    /// caller owns the include stack.
    #[must_use]
    pub(crate) fn with_custom_roles<'b>(&'b self, roles: &'b CustomRoles) -> ParseCtx<'b> {
        ParseCtx {
            custom_roles: Some(roles),
            ..*self
        }
    }

    /// The roles the document has defined so far, or `None` when this parse
    /// keeps no table.
    #[must_use]
    pub(crate) const fn custom_roles(&self) -> Option<&'a CustomRoles> {
        self.custom_roles
    }

    /// The same context, parsing against `schema`.
    #[must_use]
    pub fn with_schema(self, schema: &'a EntitySchema) -> Self {
        Self { schema, ..self }
    }

    /// The same context, with the schema's `[import_keys]` resolved for a
    /// `.. needimport::` to look an alias up in.
    ///
    /// Separate from [`Self::with_schema`] even though the table is declared
    /// in the schema file, because what belongs here is the *resolved* map and
    /// the schema holds the values as written — see
    /// `commands::entity_schema::resolve_import_keys`.
    #[must_use]
    pub fn with_import_keys(self, import_keys: &'a BTreeMap<String, String>) -> Self {
        Self {
            import_keys,
            ..self
        }
    }

    /// The same context, rendering each document's source as a Jinja template
    /// first, with `context` bound for it to read.
    #[must_use]
    pub fn with_jinja(self, context: &'a [(String, String)]) -> Self {
        Self {
            jinja: Some(context),
            ..self
        }
    }

    /// The context for parsing text a Jinja pass produced, whose lines `map`
    /// traces back to the files they were written in.
    ///
    /// Borrows for a possibly shorter lifetime than `'a` because the map is
    /// built during the parse it configures, exactly as
    /// [`included`](Self::included)'s stack is.
    #[must_use]
    pub(crate) fn templated<'b>(&'b self, map: &'b TemplateMap) -> ParseCtx<'b> {
        ParseCtx {
            template_map: Some(map),
            ..*self
        }
    }

    /// The context for parsing the body of an entity of `entity_type`.
    #[must_use]
    pub(crate) fn inside_entity(&self, entity_type: &'a EntityType) -> Self {
        Self {
            enclosing_entity: Some(entity_type),
            ..*self
        }
    }

    /// The same context, recording *which* entity is being parsed.
    ///
    /// Separate from [`Self::inside_entity`] because the id is only known
    /// after the entity's options have been read, while the type is known from
    /// its directive name — and because, unlike the type, the id survives into
    /// a section body.
    #[must_use]
    pub(crate) fn inside_entity_id(&self, id: &'a EntityId) -> Self {
        Self {
            enclosing_entity_id: Some(id),
            ..*self
        }
    }

    /// The context for parsing the body of a `.. grid::`, in which a
    /// `.. grid-item::` is the expected child.
    #[must_use]
    pub(crate) fn inside_grid_row(&self) -> Self {
        Self {
            in_grid_row: true,
            ..*self
        }
    }

    /// The context for parsing the body of a `.. grid-item::`, which is
    /// content rather than a row — an item written inside an item is as
    /// misplaced as one written at top level.
    #[must_use]
    pub(crate) fn outside_grid_row(&self) -> Self {
        Self {
            in_grid_row: false,
            ..*self
        }
    }

    /// The context for parsing a section's own body.
    ///
    /// Clears the enclosing entity *type*, so a section sub-directive written
    /// inside a section is not recognised — sections are one level deep by
    /// design, and the alternative is a nesting whose rendering has no meaning.
    ///
    /// The id is deliberately kept: a section's body is still the entity's
    /// content, so an `.. entity-arch::` written there draws the entity it
    /// sits in rather than reporting that it sits in none.
    #[must_use]
    pub(crate) fn outside_entity(&self) -> Self {
        Self {
            enclosing_entity: None,
            ..*self
        }
    }

    /// The context for a nested parse over a line slice that begins
    /// `line_offset` lines below this one's first line, and whose lines have
    /// had `column_offset` leading characters stripped.
    ///
    /// Offsets compose, so nesting a body inside a body inside a list item
    /// still lands on the right source position.
    #[must_use]
    pub(crate) fn nested(&self, line_offset: usize, column_offset: usize) -> Self {
        Self {
            origin: self.origin.map(|origin| {
                let column_offset = u32::try_from(column_offset).unwrap_or(0);
                Origin {
                    line: origin.line + u32::try_from(line_offset).unwrap_or(0),
                    column: origin.column + column_offset,
                    // Only the slice's first line hangs, so a slice starting
                    // on any later line has none.
                    first_line_column: origin
                        .first_line_column
                        .filter(|_| line_offset == 0)
                        .map(|column| column + column_offset),
                }
            }),
            ..*self
        }
    }

    /// The context for a nested parse over a line slice that begins
    /// `line_offset` lines below this one's first line, whose first line
    /// starts `first_line_column` characters in, and whose remaining lines
    /// have had `column_offset` leading characters stripped.
    ///
    /// The shape of a directive whose content starts on its marker line:
    /// `.. seealso:: text` puts the first content line after the `::`, while
    /// any continuation sits at the body's indent. A single column cannot
    /// place both, and getting the first wrong would misplace every
    /// diagnostic about a role written there.
    #[must_use]
    pub(crate) fn hanging(
        &self,
        line_offset: usize,
        first_line_column: usize,
        column_offset: usize,
    ) -> Self {
        let nested = self.nested(line_offset, column_offset);
        Self {
            origin: nested.origin.map(|origin| Origin {
                first_line_column: self.origin.map(|outer| {
                    // Measured from wherever the outer slice's line
                    // `line_offset` starts, which is itself hanging when that
                    // is its first line.
                    let base = outer
                        .first_line_column
                        .filter(|_| line_offset == 0)
                        .unwrap_or(outer.column);
                    base + u32::try_from(first_line_column).unwrap_or(0)
                }),
                ..origin
            }),
            ..nested
        }
    }

    /// The context for a nested parse over lines that exist nowhere in the
    /// source — today only the rows `.. csv-table::` generates from CSV data.
    ///
    /// Everything parsed under it reports positionless diagnostics, which is
    /// honest: pointing at a line of the `.rst` that does not contain the
    /// offending text would be worse than pointing nowhere.
    #[must_use]
    pub(crate) fn synthetic(&self) -> Self {
        Self {
            origin: None,
            ..*self
        }
    }

    /// The context for parsing the text of an included file: positions start
    /// again at line 1 column 1, and every span built under it is attributed
    /// to `file`.
    ///
    /// `stack` must be this context's [`include_stack`](Self::include_stack)
    /// extended with `id`; the caller owns it, which is why the returned
    /// context borrows for a possibly shorter lifetime than `'a`.
    #[must_use]
    pub(crate) fn included<'b>(
        &'b self,
        file: FileId,
        id: &'b str,
        stack: &'b [String],
    ) -> ParseCtx<'b> {
        ParseCtx {
            default_domain: self.default_domain,
            files: self.files,
            schema: self.schema,
            doc_path: self.doc_path,
            jinja: self.jinja,
            enclosing_entity: self.enclosing_entity,
            enclosing_entity_id: self.enclosing_entity_id,
            in_grid_row: self.in_grid_row,
            import_keys: self.import_keys,
            custom_roles: self.custom_roles,
            origin: Some(Origin::START),
            file: Some(file),
            current_file: Some(id),
            include_stack: stack,
            // An `.. include::` fragment is not Jinja-rendered: Sphinx's
            // `source-read` fires for documents, not for transcluded text. Its
            // positions therefore restart at line 1 of its own file.
            template_map: None,
        }
    }

    /// The loader id of the file being parsed, or `None` for the document —
    /// what a directive passes as the loader's `relative_to`.
    #[must_use]
    pub(crate) const fn current_file(&self) -> Option<&'a str> {
        self.current_file
    }

    /// The chain of files currently being included, outermost first.
    #[must_use]
    pub(crate) const fn include_stack(&self) -> &'a [String] {
        self.include_stack
    }

    /// The source position of `local_column` on `local_line`, both 0-based
    /// indices into the slice being parsed, and the file it is measured in.
    #[must_use]
    pub(crate) fn position(&self, local_line: usize, local_column: usize) -> Option<SourcePoint> {
        let origin = self.origin?;
        let line = origin.line + u32::try_from(local_line).unwrap_or(0);
        let first_column = origin
            .first_line_column
            .filter(|_| local_line == 0)
            .unwrap_or(origin.column);
        let column = first_column + u32::try_from(local_column).unwrap_or(0);
        let Some(map) = self.template_map else {
            return Some(SourcePoint {
                position: Position::new(line, column),
                file: self.file,
            });
        };
        let written = map.written_at(line);
        Some(SourcePoint {
            position: Position::new(written.line, column),
            file: written.file,
        })
    }

    /// A span covering the whole of `local_line`, whose content is `text`.
    /// The common shape for a block-level diagnostic: it can name the
    /// offending line, but no column within it means anything.
    #[must_use]
    pub(crate) fn line_span(&self, local_line: usize, text: &str) -> Option<Span> {
        let start = self.position(local_line, 0)?;
        let end = self.position(local_line, text.chars().count())?;
        Some(start.to(end))
    }

    /// A span covering `first` through `last` inclusive (0-based indices),
    /// for a diagnostic about a multi-line construct as a whole. `last_text`
    /// is the content of the last line.
    #[must_use]
    pub(crate) fn lines_span(&self, first: usize, last: usize, last_text: &str) -> Option<Span> {
        let start = self.position(first, 0)?;
        let end = self.position(last, last_text.chars().count())?;
        Some(start.to(end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_domain_keeps_the_requested_domain() {
        // Given / When
        let ctx = ParseCtx::with_domain(Domain::C);

        // Then
        assert_eq!(ctx.default_domain, Domain::C);
    }

    #[test]
    fn test_reject_parse_files_names_the_path_it_refused() {
        // Given
        let loader = RejectParseFiles;

        // When
        let result = loader.load("data/fruits.csv", None);

        // Then
        let message = result
            .err()
            .expect("RejectParseFiles must refuse every path");
        assert!(message.contains("data/fruits.csv"), "{message}");
    }

    #[test]
    fn test_hanging_places_the_first_line_at_its_own_column() {
        // Given a slice starting on line 3 whose first line begins 13
        // characters in and whose other lines had 3 characters stripped
        let ctx = ParseCtx::with_domain(Domain::Py).hanging(2, 13, 3);

        // When
        let first = ctx.position(0, 0).expect("positioned");
        let second = ctx.position(1, 0).expect("positioned");

        // Then
        assert_eq!(first.position, Position::new(3, 14));
        assert_eq!(second.position, Position::new(4, 4));
    }

    #[test]
    fn test_nested_on_the_first_line_keeps_the_hanging_column() {
        // Given
        let ctx = ParseCtx::with_domain(Domain::Py).hanging(0, 10, 3);

        // When
        let nested = ctx.nested(0, 2);

        // Then
        let point = nested.position(0, 0).expect("positioned");
        assert_eq!(point.position, Position::new(1, 13));
        let below = nested.position(1, 0).expect("positioned");
        assert_eq!(below.position, Position::new(2, 6));
    }

    #[test]
    fn test_nested_below_the_first_line_drops_the_hanging_column() {
        // Given
        let ctx = ParseCtx::with_domain(Domain::Py).hanging(0, 10, 3);

        // When
        let nested = ctx.nested(1, 2);

        // Then
        let point = nested.position(0, 0).expect("positioned");
        assert_eq!(point.position, Position::new(2, 6));
    }

    #[test]
    fn test_hanging_inside_a_hanging_first_line_measures_from_it() {
        // Given a hanging slice, and a hanging slice starting on its first line
        let outer = ParseCtx::with_domain(Domain::Py).hanging(0, 10, 3);

        // When
        let inner = outer.hanging(0, 4, 1);

        // Then
        let point = inner.position(0, 0).expect("positioned");
        assert_eq!(point.position, Position::new(1, 15));
        let below = inner.position(1, 0).expect("positioned");
        assert_eq!(below.position, Position::new(2, 5));
    }

    #[test]
    fn test_hanging_of_a_synthetic_context_stays_positionless() {
        // Given
        let ctx = ParseCtx::with_domain(Domain::Py).synthetic();

        // When
        let hanging = ctx.hanging(1, 5, 3);

        // Then
        assert_eq!(hanging.position(0, 0), None);
    }

    #[test]
    fn test_with_domain_uses_the_rejecting_loader() {
        // Given
        let ctx = ParseCtx::with_domain(Domain::Py);

        // When
        let result = ctx.files.load("anything.csv", None);

        // Then
        assert!(result.is_err());
    }
}
