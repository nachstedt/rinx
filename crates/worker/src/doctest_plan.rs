//! The extracted, executable form of a document's doctest blocks.
//!
//! Like [`crate::domain_warnings`], the JSON wire format is deliberately owned
//! here, at the CLI/tooling boundary: the AST types stay plain parsed-document
//! values, and only the worker knows the on-disk shape the Python runner reads.
//!
//! # This file is the cache firewall
//!
//! A `.doctests.json` is a *projection* of the AST that keeps only what changes
//! how the code runs. Everything presentational is dropped on purpose —
//! `:hide:`, the trim tri-state, the language a block renders as, and source
//! line numbers. Editing prose, hiding a block, or flipping
//! `:trim-doctest-flags:` therefore leaves these bytes **identical**, Bazel's
//! action cache stops the cascade here, and the Python test does not re-run.
//!
//! Adding a presentation-only field to these types would silently break that
//! property, which is why `test_plan_ignores_presentation_only_differences`
//! asserts byte equality rather than merely structural equality.

use rusty_sphinx_ast::{self as ast, DocTestBlock, DocTestGroupSelector};
use serde::{Deserialize, Serialize};

/// One document's doctest groups, in the order they first appear.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocTestPlan {
    /// The document these tests came from, for failure messages.
    pub doc_path: String,
    pub groups: Vec<DocTestGroupPlan>,
}

/// One group: a single Python namespace shared by its setup, tests and cleanup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocTestGroupPlan {
    pub name: String,
    pub setup: Vec<DocTestSnippet>,
    pub cases: Vec<DocTestCase>,
    pub cleanup: Vec<DocTestSnippet>,
}

/// The conditions under which a block runs.
///
/// Both are evaluated by the runner, never here: `pyversion` is compared
/// against the running interpreter, and `skipif` is arbitrary Python.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunConditions {
    /// A PEP 440 specifier set, e.g. `">=3.5"`.
    pub pyversion: Option<String>,
    /// A Python expression; the block is skipped when it is true.
    pub skipif: Option<String>,
}

/// A `doctest` comparison flag, in the spelling
/// `doctest.OPTIONFLAGS_BY_NAME` uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocTestFlagSpec {
    pub name: String,
    /// `true` for `+FLAG`, `false` for `-FLAG`.
    pub enabled: bool,
}

/// A setup or cleanup block: code with no expected output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocTestSnippet {
    pub source: String,
    pub conditions: RunConditions,
}

/// The expected output of a [`DocTestCase::CodeWithOutput`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedOutput {
    pub text: String,
    pub conditions: RunConditions,
    pub flags: Vec<DocTestFlagSpec>,
}

/// One runnable unit within a group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocTestCase {
    /// A `.. doctest::` block, parsed by `doctest.DocTestParser` as-is.
    Interactive {
        source: String,
        conditions: RunConditions,
        flags: Vec<DocTestFlagSpec>,
    },
    /// A `.. testcode::` and the `.. testoutput::` answering it.
    ///
    /// The pairing is *structural* here rather than a rule the runner has to
    /// re-derive: an output block is attached during extraction or reported,
    /// so the runner cannot mispair them.
    CodeWithOutput {
        code: DocTestSnippet,
        /// `None` when the code is expected to print nothing.
        expected: Option<ExpectedOutput>,
    },
}

/// Builds the plan for `doc`.
///
/// # Errors
///
/// Returns every problem found, joined — an output block with no code block to
/// answer, or a second output block for code that already has one. Sphinx drops
/// both silently; reporting them means an author hears about a test that would
/// otherwise never run.
pub fn build_doctest_plan(doc: &ast::Document) -> Result<DocTestPlan, String> {
    let blocks = collect_doctest_blocks(doc);

    let mut groups: Vec<DocTestGroupPlan> = Vec::new();
    let mut all_group_blocks: Vec<&DocTestBlock> = Vec::new();
    let mut problems: Vec<String> = Vec::new();

    // Pass 1 — named groups, created in first-appearance order.
    for block in &blocks {
        if selects_all_groups(block) {
            all_group_blocks.push(block);
            continue;
        }
        for selector in block.groups().as_slice() {
            let Some(group_name) = selector.named() else {
                continue;
            };
            let index = group_index(&mut groups, group_name.as_str());
            if let Err(problem) = add_block(&mut groups[index], block) {
                problems.push(problem);
            }
        }
    }

    // Pass 2 — `*` blocks reach only the groups that already exist, so a
    // document containing nothing but `*` blocks runs nothing at all. This
    // ordering also means a `*` setup lands after any group's own setup.
    for block in all_group_blocks {
        for group in &mut groups {
            if let Err(problem) = add_block(group, block) {
                problems.push(problem);
            }
        }
    }

    if problems.is_empty() {
        Ok(DocTestPlan {
            doc_path: doc.path.clone(),
            groups,
        })
    } else {
        Err(problems.join("\n"))
    }
}

/// Collects every doctest block in `doc`, however deeply nested, in document
/// order.
fn collect_doctest_blocks(doc: &ast::Document) -> Vec<&DocTestBlock> {
    let mut blocks = Vec::new();
    ast::walk_nodes(&doc.nodes, &mut |node| {
        if let ast::Node::Directive(ast::Directive::DocTest(block)) = node {
            blocks.push(block);
        }
    });
    blocks
}

/// Whether any of the block's selectors is `*`.
///
/// Sphinx checks membership and then skips the block's named groups entirely,
/// so `.. testsetup:: *, foo` goes to every group rather than to `foo` twice.
fn selects_all_groups(block: &DocTestBlock) -> bool {
    block
        .groups()
        .as_slice()
        .iter()
        .any(|selector| matches!(selector, DocTestGroupSelector::AllGroups))
}

/// Returns the index of the group named `name`, appending it if new.
fn group_index(groups: &mut Vec<DocTestGroupPlan>, name: &str) -> usize {
    if let Some(index) = groups.iter().position(|group| group.name == name) {
        return index;
    }
    groups.push(DocTestGroupPlan {
        name: name.to_string(),
        setup: Vec::new(),
        cases: Vec::new(),
        cleanup: Vec::new(),
    });
    groups.len() - 1
}

/// Adds one block to `group`, mirroring Sphinx's `TestGroup.add_code`.
fn add_block(group: &mut DocTestGroupPlan, block: &DocTestBlock) -> Result<(), String> {
    match block {
        DocTestBlock::Setup { .. } => group.setup.push(snippet_of(block)),
        DocTestBlock::Cleanup { .. } => group.cleanup.push(snippet_of(block)),
        DocTestBlock::Interactive { flags, .. } => group.cases.push(DocTestCase::Interactive {
            source: block.content().body().to_string(),
            conditions: conditions_of(block),
            flags: flag_specs(flags),
        }),
        DocTestBlock::Code { .. } => group.cases.push(DocTestCase::CodeWithOutput {
            code: snippet_of(block),
            expected: None,
        }),
        DocTestBlock::Output { flags, .. } => return attach_output(group, block, flags),
    }
    Ok(())
}

/// Attaches an output block to the last code block *in this group*.
///
/// The pairing is per-group, not document-adjacent: blocks of other groups may
/// sit between a `testcode` and its `testoutput`, and reading the preceding
/// sibling instead would mispair them in any document using two groups.
fn attach_output(
    group: &mut DocTestGroupPlan,
    block: &DocTestBlock,
    flags: &[ast::DocTestFlag],
) -> Result<(), String> {
    let expected = ExpectedOutput {
        text: block.content().body().to_string(),
        conditions: conditions_of(block),
        flags: flag_specs(flags),
    };

    match group.cases.last_mut() {
        Some(DocTestCase::CodeWithOutput { expected: slot, .. }) if slot.is_none() => {
            *slot = Some(expected);
            Ok(())
        }
        Some(DocTestCase::CodeWithOutput { .. }) => Err(format!(
            "group '{}': a testoutput block already answers this testcode; \
             the second one would never run",
            group.name
        )),
        Some(DocTestCase::Interactive { .. }) => Err(format!(
            "group '{}': a testoutput block follows a doctest block, which \
             states its own output; it would never run",
            group.name
        )),
        None => Err(format!(
            "group '{}': a testoutput block has no preceding testcode to \
             answer; it would never run",
            group.name
        )),
    }
}

/// The source and run conditions of any block.
fn snippet_of(block: &DocTestBlock) -> DocTestSnippet {
    DocTestSnippet {
        source: block.content().body().to_string(),
        conditions: conditions_of(block),
    }
}

/// Extracts the conditions under which `block` runs, rendering `pyversion`
/// back to its canonical specifier spelling for the runner.
fn conditions_of(block: &DocTestBlock) -> RunConditions {
    let pyversion = match block {
        DocTestBlock::Interactive { pyversion, .. }
        | DocTestBlock::Code { pyversion, .. }
        | DocTestBlock::Output { pyversion, .. } => pyversion.as_ref().map(ToString::to_string),
        DocTestBlock::Setup { .. } | DocTestBlock::Cleanup { .. } => None,
    };

    RunConditions {
        pyversion,
        skipif: block.skipif().cloned(),
    }
}

/// Renders parsed flags back to the spellings `doctest` knows.
fn flag_specs(flags: &[ast::DocTestFlag]) -> Vec<DocTestFlagSpec> {
    flags
        .iter()
        .map(|flag| DocTestFlagSpec {
            name: flag.name.as_doctest_name().to_string(),
            enabled: flag.enabled,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_parser::parse;

    /// Parses `rst` and builds its plan, expecting success.
    fn plan_of(rst: &str) -> DocTestPlan {
        build_doctest_plan(&parse("test.rst", rst)).expect("plan should build")
    }

    /// Parses `rst` and builds its plan, expecting failure.
    fn problems_of(rst: &str) -> String {
        build_doctest_plan(&parse("test.rst", rst)).expect_err("plan should fail")
    }

    /// Unwraps a code/output pair, failing the test on any other case.
    fn code_pair(case: &DocTestCase) -> (&DocTestSnippet, Option<&ExpectedOutput>) {
        match case {
            DocTestCase::CodeWithOutput { code, expected } => (code, expected.as_ref()),
            DocTestCase::Interactive { .. } => {
                panic!("expected a code/output pair, got an interactive case")
            }
        }
    }

    /// Unwraps an interactive case's flags, failing the test on any other case.
    fn interactive_flags(case: &DocTestCase) -> &[DocTestFlagSpec] {
        match case {
            DocTestCase::Interactive { flags, .. } => flags,
            DocTestCase::CodeWithOutput { .. } => {
                panic!("expected an interactive case, got a code/output pair")
            }
        }
    }

    #[test]
    fn test_builds_an_empty_plan_for_a_document_without_doctests() {
        // Given
        let rst = "Title\n=====\n\nJust prose.";

        // When
        let plan = plan_of(rst);

        // Then
        assert!(plan.groups.is_empty());
        assert_eq!(plan.doc_path, "test.rst");
    }

    #[test]
    fn test_puts_an_unnamed_block_in_the_default_group() {
        // Given
        let rst = ".. doctest::\n\n   >>> 1 + 1\n   2";

        // When
        let plan = plan_of(rst);

        // Then
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].name, "default");
        assert_eq!(plan.groups[0].cases.len(), 1);
    }

    #[test]
    fn test_pairs_testcode_with_the_testoutput_that_follows_it() {
        // Given
        let rst = ".. testcode::\n\n   print(42)\n\n.. testoutput::\n\n   42";

        // When
        let plan = plan_of(rst);

        // Then
        let (code, expected) = code_pair(&plan.groups[0].cases[0]);
        assert_eq!(code.source, "print(42)");
        assert_eq!(expected.expect("paired").text, "42");
    }

    #[test]
    fn test_leaves_expected_empty_for_testcode_that_prints_nothing() {
        // Given
        let rst = ".. testcode::\n\n   x = 1";

        // When
        let plan = plan_of(rst);

        // Then
        let (_, expected) = code_pair(&plan.groups[0].cases[0]);
        assert!(expected.is_none());
    }

    #[test]
    fn test_pairs_per_group_across_interleaved_blocks() {
        // Given — the two groups interleave, so document adjacency would
        // pair `a`'s code with `b`'s output.
        let rst = concat!(
            ".. testcode:: a\n\n   print('a')\n\n",
            ".. testcode:: b\n\n   print('b')\n\n",
            ".. testoutput:: a\n\n   a\n\n",
            ".. testoutput:: b\n\n   b\n",
        );

        // When
        let plan = plan_of(rst);

        // Then — each output landed in its own group.
        for group in &plan.groups {
            let (code, expected) = code_pair(&group.cases[0]);
            assert_eq!(code.source, format!("print('{}')", group.name));
            assert_eq!(expected.expect("paired").text, group.name);
        }
    }

    #[test]
    fn test_keeps_groups_in_first_appearance_order() {
        // Given — deterministic ordering keeps the JSON byte-stable.
        let rst = concat!(
            ".. testcode:: zebra\n\n   pass\n\n",
            ".. testcode:: alpha\n\n   pass\n",
        );

        // When
        let plan = plan_of(rst);

        // Then
        let names: Vec<&str> = plan.groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, vec!["zebra", "alpha"]);
    }

    #[test]
    fn test_joins_a_block_to_every_group_it_names() {
        // Given
        let rst = concat!(
            ".. testcode:: one\n\n   pass\n\n",
            ".. testcode:: two\n\n   pass\n\n",
            ".. testsetup:: one, two\n\n   import os\n",
        );

        // When
        let plan = plan_of(rst);

        // Then
        assert_eq!(plan.groups.len(), 2);
        for group in &plan.groups {
            assert_eq!(group.setup.len(), 1);
        }
    }

    #[test]
    fn test_distributes_a_star_block_to_every_existing_group() {
        // Given
        let rst = concat!(
            ".. testsetup:: *\n\n   import math\n\n",
            ".. testcode:: one\n\n   pass\n\n",
            ".. testcode:: two\n\n   pass\n",
        );

        // When
        let plan = plan_of(rst);

        // Then
        assert_eq!(plan.groups.len(), 2);
        for group in &plan.groups {
            assert_eq!(group.setup[0].source, "import math");
        }
    }

    #[test]
    fn test_drops_star_blocks_when_no_group_exists() {
        // Given — Sphinx returns early when no group was created, so this
        // setup code never runs. Reproducing that matters: doing otherwise
        // would execute code Sphinx never executes.
        let rst = ".. testsetup:: *\n\n   import math\n";

        // When
        let plan = plan_of(rst);

        // Then
        assert!(plan.groups.is_empty());
    }

    #[test]
    fn test_a_star_block_does_not_also_join_its_named_groups_twice() {
        // Given — Sphinx checks for `*` and skips the named groups entirely.
        let rst = concat!(
            ".. testcode:: one\n\n   pass\n\n",
            ".. testsetup:: *, one\n\n   import os\n",
        );

        // When
        let plan = plan_of(rst);

        // Then
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].setup.len(), 1);
    }

    #[test]
    fn test_orders_star_setup_after_a_groups_own_setup() {
        // Given — `*` blocks are distributed in a second pass.
        let rst = concat!(
            ".. testsetup:: one\n\n   named = 1\n\n",
            ".. testsetup:: *\n\n   starred = 2\n\n",
            ".. testcode:: one\n\n   pass\n",
        );

        // When
        let plan = plan_of(rst);

        // Then
        let sources: Vec<&str> = plan.groups[0]
            .setup
            .iter()
            .map(|s| s.source.as_str())
            .collect();
        assert_eq!(sources, vec!["named = 1", "starred = 2"]);
    }

    #[test]
    fn test_collects_cleanup_blocks() {
        // Given
        let rst = concat!(
            ".. testcode:: one\n\n   pass\n\n",
            ".. testcleanup:: one\n\n   del x\n",
        );

        // When
        let plan = plan_of(rst);

        // Then
        assert_eq!(plan.groups[0].cleanup[0].source, "del x");
    }

    #[test]
    fn test_finds_doctests_nested_inside_other_directives() {
        // Given — the top-level-only scan that shipped for diagrams would
        // have missed this entirely.
        let rst = ".. note::\n\n   .. testcode::\n\n      print(1)\n";

        // When
        let plan = plan_of(rst);

        // Then
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].cases.len(), 1);
    }

    #[test]
    fn test_carries_run_conditions_through_unevaluated() {
        // Given
        let rst = concat!(
            ".. testcode::\n",
            "   :pyversion: >= 3.5\n",
            "   :skipif: sys.platform == 'win32'\n",
            "\n   print(1)\n",
        );

        // When
        let plan = plan_of(rst);

        // Then — rendered canonically, decided by the runner.
        let (code, _) = code_pair(&plan.groups[0].cases[0]);
        assert_eq!(code.conditions.pyversion.as_deref(), Some(">=3.5"));
        assert_eq!(
            code.conditions.skipif.as_deref(),
            Some("sys.platform == 'win32'")
        );
    }

    #[test]
    fn test_carries_comparison_flags_in_doctest_spelling() {
        // Given
        let rst = concat!(
            ".. doctest::\n",
            "   :options: +ELLIPSIS, -NORMALIZE_WHITESPACE\n",
            "\n   >>> f()\n",
        );

        // When
        let plan = plan_of(rst);

        // Then
        assert_eq!(
            interactive_flags(&plan.groups[0].cases[0]),
            &[
                DocTestFlagSpec {
                    name: "ELLIPSIS".to_string(),
                    enabled: true
                },
                DocTestFlagSpec {
                    name: "NORMALIZE_WHITESPACE".to_string(),
                    enabled: false
                },
            ]
        );
    }

    #[test]
    fn test_reports_a_testoutput_with_no_preceding_testcode() {
        // Given — Sphinx drops this silently; the author never learns their
        // expected output is not checked.
        let rst = ".. testoutput::\n\n   42\n";

        // When
        let problems = problems_of(rst);

        // Then
        assert!(problems.contains("no preceding testcode"));
    }

    #[test]
    fn test_reports_a_second_testoutput_for_the_same_testcode() {
        // Given
        let rst = concat!(
            ".. testcode::\n\n   print(1)\n\n",
            ".. testoutput::\n\n   1\n\n",
            ".. testoutput::\n\n   2\n",
        );

        // When
        let problems = problems_of(rst);

        // Then
        assert!(problems.contains("already answers this testcode"));
    }

    #[test]
    fn test_reports_a_testoutput_following_a_doctest_block() {
        // Given — a doctest states its own output, so this can never run.
        let rst = concat!(
            ".. doctest::\n\n   >>> 1\n   1\n\n",
            ".. testoutput::\n\n   1\n",
        );

        // When
        let problems = problems_of(rst);

        // Then
        assert!(problems.contains("states its own output"));
    }

    #[test]
    fn test_reports_every_problem_at_once() {
        // Given — two independent mistakes in two groups.
        let rst = concat!(
            ".. testoutput:: a\n\n   1\n\n",
            ".. testoutput:: b\n\n   2\n",
        );

        // When
        let problems = problems_of(rst);

        // Then — both reported, so one build shows the author everything.
        assert_eq!(problems.lines().count(), 2);
    }

    #[test]
    fn test_plan_ignores_presentation_only_differences() {
        // Given — two documents whose *tests* are identical but whose prose,
        // `:hide:` and trim options differ.
        let plain = concat!(
            "Title\n=====\n\nSome prose.\n\n",
            ".. testcode::\n\n   print(1)\n\n",
            ".. testoutput::\n\n   1\n",
        );
        let decorated = concat!(
            "Title\n=====\n\nCompletely different prose, and more of it.\n\n",
            ".. testcode::\n   :hide:\n\n   print(1)\n\n",
            ".. testoutput::\n   :no-trim-doctest-flags:\n\n   1\n",
        );

        // When — serialized exactly as the extract step writes them.
        let left = serde_json::to_string(&plan_of(plain)).expect("serialize");
        let right = serde_json::to_string(&plan_of(decorated)).expect("serialize");

        // Then — byte-identical, so Bazel's action cache stops the cascade
        // here and the Python test does not re-run. This is the property the
        // whole design rests on; asserting structural equality would let a
        // presentation-only field creep into the wire format unnoticed.
        assert_eq!(left, right);
    }

    #[test]
    fn test_plan_changes_when_the_test_code_changes() {
        // Given — the other half of the firewall: a real change must not be
        // cached away.
        let before = ".. testcode::\n\n   print(1)\n";
        let after = ".. testcode::\n\n   print(2)\n";

        // When
        let left = serde_json::to_string(&plan_of(before)).expect("serialize");
        let right = serde_json::to_string(&plan_of(after)).expect("serialize");

        // Then
        assert_ne!(left, right);
    }

    #[test]
    fn test_plan_serialization_roundtrip() {
        // Given
        let plan = plan_of(".. testcode:: g\n\n   print(1)\n\n.. testoutput:: g\n\n   1\n");

        // When
        let json = serde_json::to_string(&plan).expect("serialize");
        let restored: DocTestPlan = serde_json::from_str(&json).expect("deserialize");

        // Then
        assert_eq!(plan, restored);
    }
}
