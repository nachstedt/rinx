//! Grouping and collection tests for [`super::build_doctest_plan`]: which
//! blocks land in which group, in what order, and which blocks are
//! collected at all. The run-conditions and problem-reporting cases live
//! in [`super::conditions_tests`].

use super::test_support::{code_pair, interactive_case, plan_of};

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
fn test_collects_a_bare_doctest_block_into_the_default_group() {
    // Given — no directive at all, just a `>>>` block.
    let rst = ">>> 1 + 1\n2\n";

    // When
    let plan = plan_of(rst);

    // Then
    assert_eq!(plan.groups.len(), 1);
    assert_eq!(plan.groups[0].name, "default");
    assert_eq!(plan.groups[0].cases.len(), 1);
}

#[test]
fn test_a_bare_block_carries_no_options() {
    // Given
    let rst = ">>> 1 + 1\n2\n";

    // When
    let plan = plan_of(rst);

    // Then
    let (source, conditions, flags) = interactive_case(&plan.groups[0].cases[0]);
    assert_eq!(source, ">>> 1 + 1\n2");
    assert_eq!(conditions.pyversion, None);
    assert_eq!(conditions.skipif, None);
    assert!(flags.is_empty());
}

#[test]
fn test_interleaves_bare_blocks_with_directives_in_document_order() {
    // Given — the shape that makes `re.rst` work: an import written as a
    // bare block, used later by a directive in the same group. Ordering is
    // the whole point, since a group is one namespace.
    let rst = concat!(
        "Intro:\n\n",
        "   >>> import re\n\n",
        ".. doctest::\n\n",
        "   >>> re.escape('a')\n   'a'\n",
    );

    // When
    let plan = plan_of(rst);

    // Then — one group, the import first.
    assert_eq!(plan.groups.len(), 1);
    assert_eq!(plan.groups[0].cases.len(), 2);
    let (source, _, _) = interactive_case(&plan.groups[0].cases[0]);
    assert_eq!(source, ">>> import re", "the import must run first");
}

#[test]
fn test_a_literal_block_is_not_collected() {
    // Given — a `::` block must never become executable.
    let rst = "Example::\n\n   >>> 1 + 1\n   2\n";

    // When
    let plan = plan_of(rst);

    // Then
    assert!(plan.groups.is_empty());
}

#[test]
fn test_a_bare_block_creates_a_group_for_a_star_setup_to_reach() {
    // Given — a document whose only named group comes from a bare block.
    // Sphinx distributes `*` blocks over groups that already exist, so the
    // setup now has somewhere to land where previously it had none.
    let rst = concat!(
        ".. testsetup:: *\n\n   import math\n\n",
        "Intro:\n\n",
        "   >>> int(math.pi)\n   3\n",
    );

    // When
    let plan = plan_of(rst);

    // Then
    assert_eq!(plan.groups.len(), 1);
    assert_eq!(plan.groups[0].setup.len(), 1);
    assert_eq!(plan.groups[0].cases.len(), 1);
}

#[test]
fn test_finds_bare_blocks_nested_inside_directives() {
    // Given
    let rst = ".. note::\n\n   Try it:\n\n   >>> 1 + 1\n   2\n";

    // When
    let plan = plan_of(rst);

    // Then
    assert_eq!(plan.groups.len(), 1);
    assert_eq!(plan.groups[0].cases.len(), 1);
}
