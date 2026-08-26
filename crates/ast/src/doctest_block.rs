use serde::{Deserialize, Serialize};

use crate::doctest_flag::DocTestFlag;
use crate::doctest_group::DocTestGroupSelector;
use crate::hashed_content::HashedContent;
use crate::non_empty_vector::NonEmptyVector;
use crate::py_version_spec::PyVersionSpec;

/// Whether a block's rendered form should have its doctest flag comments
/// stripped.
///
/// Three states, not two: `:trim-doctest-flags:` and `:no-trim-doctest-flags:`
/// are *separate* options in Sphinx, and writing neither is distinct from
/// writing either — the unset case defers to the project-wide
/// `trim_doctest_flags` setting (which defaults to trimming).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DocTestTrim {
    /// `:trim-doctest-flags:` — always strip.
    Trim,
    /// `:no-trim-doctest-flags:` — never strip.
    NoTrim,
    /// Neither option given; fall back to the project default.
    Unset,
}

impl DocTestTrim {
    /// Resolves the tri-state against the project-wide default.
    #[must_use]
    pub const fn resolve(self, project_default: bool) -> bool {
        match self {
            Self::Trim => true,
            Self::NoTrim => false,
            Self::Unset => project_default,
        }
    }
}

/// One block from the `sphinx.ext.doctest` directive family.
///
/// # Why five variants rather than one struct
///
/// The five directives do not share an option set: `testsetup`/`testcleanup`
/// accept only `:skipif:` (they are never rendered, so `:hide:` would be
/// meaningless, and they are never compared against expected output, so
/// `:options:` would be too), while `testcode` has no `:options:` because it
/// carries no expected output of its own. Folding them into one struct would
/// give every block a union of fields that are meaningless on most of them.
///
/// # Content is stored raw, exactly once
///
/// [`HashedContent`] holds the block body **verbatim**, with only the common
/// leading indentation removed. Sphinx keeps a raw test source and separately
/// derives a display form by stripping `<BLANKLINE>` markers and
/// `# doctest: +FLAG` comments; storing that derived form here instead would
/// lose the source the runner needs. The renderer derives the display form and
/// the extractor derives the test form, each from this one field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocTestBlock {
    /// `.. doctest::` — interactive `>>>` examples with inline expected output.
    Interactive {
        groups: NonEmptyVector<DocTestGroupSelector>,
        content: HashedContent,
        hide: bool,
        flags: Vec<DocTestFlag>,
        pyversion: Option<PyVersionSpec>,
        skipif: Option<String>,
        trim: DocTestTrim,
    },
    /// `.. testcode::` — plain Python source whose output, if any, is stated by
    /// a following [`Self::Output`] block in the same group.
    Code {
        groups: NonEmptyVector<DocTestGroupSelector>,
        content: HashedContent,
        hide: bool,
        pyversion: Option<PyVersionSpec>,
        skipif: Option<String>,
        trim: DocTestTrim,
    },
    /// `.. testoutput::` — the expected output of the preceding
    /// [`Self::Code`] block in the same group.
    Output {
        groups: NonEmptyVector<DocTestGroupSelector>,
        content: HashedContent,
        hide: bool,
        flags: Vec<DocTestFlag>,
        pyversion: Option<PyVersionSpec>,
        skipif: Option<String>,
        trim: DocTestTrim,
    },
    /// `.. testsetup::` — code run before its group's tests. Never rendered.
    Setup {
        groups: NonEmptyVector<DocTestGroupSelector>,
        content: HashedContent,
        skipif: Option<String>,
    },
    /// `.. testcleanup::` — code run after its group's tests. Never rendered.
    Cleanup {
        groups: NonEmptyVector<DocTestGroupSelector>,
        content: HashedContent,
        skipif: Option<String>,
    },
}

impl DocTestBlock {
    /// The directive name this block was written as.
    #[must_use]
    pub const fn directive_name(&self) -> &'static str {
        match self {
            Self::Interactive { .. } => "doctest",
            Self::Code { .. } => "testcode",
            Self::Output { .. } => "testoutput",
            Self::Setup { .. } => "testsetup",
            Self::Cleanup { .. } => "testcleanup",
        }
    }

    /// The groups this block belongs to.
    #[must_use]
    pub const fn groups(&self) -> &NonEmptyVector<DocTestGroupSelector> {
        match self {
            Self::Interactive { groups, .. }
            | Self::Code { groups, .. }
            | Self::Output { groups, .. }
            | Self::Setup { groups, .. }
            | Self::Cleanup { groups, .. } => groups,
        }
    }

    /// The block's verbatim body.
    #[must_use]
    pub const fn content(&self) -> &HashedContent {
        match self {
            Self::Interactive { content, .. }
            | Self::Code { content, .. }
            | Self::Output { content, .. }
            | Self::Setup { content, .. }
            | Self::Cleanup { content, .. } => content,
        }
    }

    /// The `:skipif:` expression, if given.
    #[must_use]
    pub const fn skipif(&self) -> Option<&String> {
        match self {
            Self::Interactive { skipif, .. }
            | Self::Code { skipif, .. }
            | Self::Output { skipif, .. }
            | Self::Setup { skipif, .. }
            | Self::Cleanup { skipif, .. } => skipif.as_ref(),
        }
    }

    /// Whether this block produces any HTML.
    ///
    /// `testsetup`/`testcleanup` are *always* invisible — that is a property of
    /// the directive, not an option — so they answer `false` without consulting
    /// a `hide` field they do not have.
    #[must_use]
    pub const fn is_rendered(&self) -> bool {
        match self {
            Self::Interactive { hide, .. }
            | Self::Code { hide, .. }
            | Self::Output { hide, .. } => !*hide,
            Self::Setup { .. } | Self::Cleanup { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctest_flag::DocTestFlagName;
    use crate::doctest_group::DocTestGroup;

    /// A single-selector group list naming `name`.
    fn groups_named(name: &str) -> NonEmptyVector<DocTestGroupSelector> {
        NonEmptyVector::single(DocTestGroupSelector::new(name))
    }

    /// A minimal `.. doctest::` block.
    fn interactive(hide: bool) -> DocTestBlock {
        DocTestBlock::Interactive {
            groups: groups_named(""),
            content: HashedContent::new(">>> 1 + 1\n2".to_string()),
            hide,
            flags: Vec::new(),
            pyversion: None,
            skipif: None,
            trim: DocTestTrim::Unset,
        }
    }

    /// A minimal `.. testsetup::` block.
    fn setup(skipif: Option<String>) -> DocTestBlock {
        DocTestBlock::Setup {
            groups: groups_named("*"),
            content: HashedContent::new("import os".to_string()),
            skipif,
        }
    }

    #[test]
    fn test_directive_name_reports_each_variants_spelling() {
        // Given
        let cases = [
            (interactive(false), "doctest"),
            (
                DocTestBlock::Code {
                    groups: groups_named(""),
                    content: HashedContent::new("print(1)".to_string()),
                    hide: false,
                    pyversion: None,
                    skipif: None,
                    trim: DocTestTrim::Unset,
                },
                "testcode",
            ),
            (
                DocTestBlock::Output {
                    groups: groups_named(""),
                    content: HashedContent::new("1".to_string()),
                    hide: false,
                    flags: Vec::new(),
                    pyversion: None,
                    skipif: None,
                    trim: DocTestTrim::Unset,
                },
                "testoutput",
            ),
            (setup(None), "testsetup"),
            (
                DocTestBlock::Cleanup {
                    groups: groups_named(""),
                    content: HashedContent::new("pass".to_string()),
                    skipif: None,
                },
                "testcleanup",
            ),
        ];

        for (block, expected) in cases {
            // When
            let name = block.directive_name();

            // Then
            assert_eq!(name, expected);
        }
    }

    #[test]
    fn test_groups_returns_the_selector_list() {
        // Given
        let block = interactive(false);

        // When
        let groups = block.groups();

        // Then
        assert_eq!(
            groups.as_slice(),
            &[DocTestGroupSelector::Named(DocTestGroup::default())]
        );
    }

    #[test]
    fn test_groups_returns_all_groups_for_a_star_argument() {
        // Given
        let block = setup(None);

        // When
        let groups = block.groups();

        // Then
        assert_eq!(groups.as_slice(), &[DocTestGroupSelector::AllGroups]);
    }

    #[test]
    fn test_content_returns_the_verbatim_body() {
        // Given
        let block = interactive(false);

        // When
        let content = block.content();

        // Then
        assert_eq!(content.body(), ">>> 1 + 1\n2");
    }

    #[test]
    fn test_skipif_returns_the_expression_when_given() {
        // Given
        let block = setup(Some("sys.platform == 'win32'".to_string()));

        // When
        let skipif = block.skipif();

        // Then
        assert_eq!(skipif.map(String::as_str), Some("sys.platform == 'win32'"));
    }

    #[test]
    fn test_skipif_returns_none_when_absent() {
        // Given
        let block = setup(None);

        // When
        let skipif = block.skipif();

        // Then
        assert_eq!(skipif, None);
    }

    #[test]
    fn test_is_rendered_is_true_for_a_visible_block() {
        // Given
        let block = interactive(false);

        // When / Then
        assert!(block.is_rendered());
    }

    #[test]
    fn test_is_rendered_is_false_for_a_hidden_block() {
        // Given
        let block = interactive(true);

        // When / Then
        assert!(!block.is_rendered());
    }

    #[test]
    fn test_is_rendered_is_false_for_setup_even_though_it_has_no_hide_option() {
        // Given — invisibility is a property of the directive here.
        let block = setup(None);

        // When / Then
        assert!(!block.is_rendered());
    }

    #[test]
    fn test_trim_resolve_honours_an_explicit_trim() {
        // Given / When / Then — the project default is overridden either way.
        assert!(DocTestTrim::Trim.resolve(false));
        assert!(!DocTestTrim::NoTrim.resolve(true));
    }

    #[test]
    fn test_trim_resolve_defers_to_the_project_default_when_unset() {
        // Given / When / Then
        assert!(DocTestTrim::Unset.resolve(true));
        assert!(!DocTestTrim::Unset.resolve(false));
    }

    #[test]
    fn test_serialization_roundtrip_for_an_interactive_block() {
        // Given — every optional field populated.
        let block = DocTestBlock::Interactive {
            groups: NonEmptyVector::new(
                DocTestGroupSelector::new("parsing"),
                vec![DocTestGroupSelector::AllGroups],
            ),
            content: HashedContent::new(">>> 1 + 1\n2".to_string()),
            hide: true,
            flags: vec![
                DocTestFlag::enable(DocTestFlagName::Ellipsis),
                DocTestFlag::disable(DocTestFlagName::NormalizeWhitespace),
            ],
            pyversion: Some(PyVersionSpec::parse(">= 3.5").expect("should parse")),
            skipif: Some("True".to_string()),
            trim: DocTestTrim::NoTrim,
        };

        // When
        let json = serde_json::to_string(&block).expect("Failed to serialize");
        let deserialized: DocTestBlock =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(block, deserialized);
    }

    #[test]
    fn test_serialization_roundtrip_for_a_setup_block() {
        // Given
        let block = setup(Some("sys.platform == 'win32'".to_string()));

        // When
        let json = serde_json::to_string(&block).expect("Failed to serialize");
        let deserialized: DocTestBlock =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(block, deserialized);
    }

    #[test]
    fn test_deserialization_rejects_content_whose_hash_does_not_match() {
        // Given — the HashedContent invariant must survive nesting in a block.
        let json = r#"{"Setup":{"groups":[{"Named":"default"}],"content":{"hash":"deadbeef","body":"import os"},"skipif":null}}"#;

        // When
        let result: Result<DocTestBlock, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialization_rejects_an_empty_group_list() {
        // Given — NonEmptyVector's invariant, reached through a block.
        let json =
            r#"{"Setup":{"groups":[],"content":{"hash":"x","body":"import os"},"skipif":null}}"#;

        // When
        let result: Result<DocTestBlock, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }
}
