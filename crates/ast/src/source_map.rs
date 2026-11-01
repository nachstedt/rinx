//! Where a document's markup was written, as only the process that parsed it
//! knows.
//!
//! A [`Document`](crate::Document) is what every later build phase reads, and
//! all of it is written to the `.ast`: reading one back yields exactly what
//! was written. What only an editor asks of a parse — where each block runs,
//! which characters each name takes, which `.. include::` brought a file in —
//! lives here instead, beside the document rather than in it. A `SourceMap`
//! has no `Serialize` or `Deserialize` on purpose, so a build phase, which
//! holds nothing but what it read from a `.ast`, cannot have one: code that
//! needs it there does not compile.

use crate::block_id::BlockId;
use crate::include_site::IncludeSite;
use crate::named::NameId;
use crate::span::Span;
use crate::suppression::SuppressionSite;
use crate::syntax::SyntaxTable;

/// What the parser recorded about where a document's markup was written,
/// beside the [`Document`](crate::Document) it parsed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMap {
    /// The blocks an editor folds and the names it colours, as the parser
    /// read them.
    pub syntax: SyntaxTable,
    /// Every `.. include::` whose file was parsed in, in the order they were
    /// met — what an editor needs to point at the line that included a
    /// problem.
    pub include_sites: Vec<IncludeSite>,
    /// Exactly the characters of each name the document was written with, by
    /// the [`NameId`] its [`Named`](crate::Named) value carries.
    names: Vec<Span>,
    /// The lines of each block a definition was written as, by the
    /// [`BlockId`] its node carries.
    block_lines: Vec<Span>,
    /// Every block a `.. noqa:` written before it would cover, in the order
    /// the parser resolved them — an enclosing block after the blocks nested
    /// in it.
    suppression_sites: Vec<SuppressionSite>,
}

impl SourceMap {
    /// Records that a name was written as exactly the characters of `span`,
    /// yielding the id its value carries — the one way an id is made.
    ///
    /// # Panics
    ///
    /// When a document records more than `u32::MAX` names.
    pub fn record_name(&mut self, span: Span) -> NameId {
        let index = u32::try_from(self.names.len()).expect("fewer than 2^32 names");
        self.names.push(span);
        NameId::new(index)
    }

    /// Exactly the characters the name recorded under `id` was written as —
    /// `None` when `id` came from a different parse, whose map holds fewer
    /// names.
    #[must_use]
    pub fn name_span(&self, id: NameId) -> Option<Span> {
        self.names.get(id.index()).copied()
    }

    /// Records that a block was written over `lines`, yielding the id its node
    /// carries — the one way an id is made.
    ///
    /// # Panics
    ///
    /// When a document records more than `u32::MAX` blocks.
    pub fn record_block_lines(&mut self, lines: Span) -> BlockId {
        let index = u32::try_from(self.block_lines.len()).expect("fewer than 2^32 blocks");
        self.block_lines.push(lines);
        BlockId::new(index)
    }

    /// The lines the block recorded under `id` was written over — `None` when
    /// `id` came from a different parse, whose map holds fewer blocks.
    #[must_use]
    pub fn block_lines(&self, id: BlockId) -> Option<Span> {
        self.block_lines.get(id.index()).copied()
    }

    /// Records that a `.. noqa:` written before a block would cover `site`.
    pub fn record_suppression_site(&mut self, site: SuppressionSite) {
        self.suppression_sites.push(site);
    }

    /// Every block a `.. noqa:` written before it would cover.
    #[must_use]
    pub fn suppression_sites(&self) -> &[SuppressionSite] {
        &self.suppression_sites
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Position;
    use std::path::Path;

    #[test]
    fn test_a_default_source_map_records_nothing() {
        // Given / When
        let map = SourceMap::default();

        // Then
        assert_eq!(map.syntax, SyntaxTable::new());
        assert!(map.include_sites.is_empty());
        assert_eq!(map.name_span(NameId::new(0)), None);
        assert_eq!(map.block_lines(BlockId::new(0)), None);
        assert!(map.suppression_sites().is_empty());
    }

    #[test]
    fn test_recorded_suppression_sites_are_listed_in_order() {
        // Given
        let mut map = SourceMap::default();
        let site = |line| SuppressionSite {
            block: Span::new(Position::new(line, 1), Position::new(line, 9)),
            comment: None,
        };

        // When
        map.record_suppression_site(site(3));
        map.record_suppression_site(site(1));

        // Then
        assert_eq!(map.suppression_sites(), [site(3), site(1)]);
    }

    #[test]
    fn test_recorded_block_lines_are_found_by_their_id() {
        // Given
        let mut map = SourceMap::default();
        let title = Span::new(Position::new(1, 1), Position::new(1, 13));
        let marker = Span::new(Position::new(4, 1), Position::new(4, 21));

        // When
        let title_id = map.record_block_lines(title);
        let marker_id = map.record_block_lines(marker);

        // Then
        assert_ne!(title_id, marker_id);
        assert_eq!(map.block_lines(title_id), Some(title));
        assert_eq!(map.block_lines(marker_id), Some(marker));
    }

    #[test]
    fn test_names_and_blocks_are_counted_apart() {
        // Given
        let mut map = SourceMap::default();
        let line = Span::new(Position::new(2, 1), Position::new(2, 9));

        // When — a name recorded first does not offset the first block's id
        map.record_name(line);
        let block = map.record_block_lines(line);

        // Then
        assert_eq!(block, BlockId::new(0));
    }

    #[test]
    fn test_a_recorded_name_is_found_by_its_id() {
        // Given
        let mut map = SourceMap::default();
        let first = Span::new(Position::new(4, 5), Position::new(4, 12));
        let second = Span::new(Position::new(9, 8), Position::new(9, 13));

        // When
        let first_id = map.record_name(first);
        let second_id = map.record_name(second);

        // Then
        assert_ne!(first_id, second_id);
        assert_eq!(map.name_span(first_id), Some(first));
        assert_eq!(map.name_span(second_id), Some(second));
    }

    /// Every `.rs` file under `dir`, recursively.
    fn rust_files(dir: &Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).expect("reads the source directory") {
            let path = entry.expect("reads an entry").path();
            if path.is_dir() {
                files.extend(rust_files(&path));
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
        files
    }

    #[test]
    fn test_no_ast_field_is_left_out_of_the_ast_file() {
        // Given every source file of this crate. The attribute is spelled in
        // pieces so this file does not find itself.
        let needle = format!("serde({})", "skip");
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

        // When
        let skipping: Vec<String> = rust_files(&src)
            .into_iter()
            .filter(|path| {
                std::fs::read_to_string(path)
                    .expect("reads a source file")
                    .contains(&needle)
            })
            .map(|path| path.display().to_string())
            .collect();

        // Then — what only an editor reads belongs in `SourceMap`, so that a
        // document read back from a `.ast` is the document that was written
        assert_eq!(skipping, Vec::<String>::new());
    }
}
