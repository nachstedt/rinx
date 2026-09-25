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

use rinx_ast::{self as ast, DocTestBlock, DocTestGroupSelector};
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
    for source in &blocks {
        let block = match source {
            // A bare `>>>` block joins the `default` group with no options,
            // interleaved with the directives in document order. Ordering is
            // what makes the shared namespace work: an `import` written as a
            // bare block has to run before a directive that relies on it.
            PlanSource::BareBlock(content) => {
                let index = group_index(&mut groups, ast::DocTestGroup::DEFAULT_NAME);
                groups[index].cases.push(DocTestCase::Interactive {
                    source: content.body().to_string(),
                    conditions: RunConditions {
                        pyversion: None,
                        skipif: None,
                    },
                    flags: Vec::new(),
                });
                continue;
            }
            PlanSource::Directive(block) => *block,
        };

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

/// One testable thing found in a document, in the two forms it can take.
///
/// Kept as a borrowed enum rather than normalising bare blocks into synthetic
/// [`DocTestBlock`] values: fabricating AST nodes that no parser produced would
/// make the plan harder to trace back to its source, and the two forms differ
/// in what they can carry anyway.
enum PlanSource<'a> {
    /// One of the five `sphinx.ext.doctest` directives.
    Directive(&'a DocTestBlock),
    /// A docutils `>>>` block: always the `default` group, never any options.
    BareBlock(&'a ast::HashedContent),
}

/// Collects every testable block in `doc`, however deeply nested, in document
/// order.
///
/// Document order matters more here than it looks: a group is one Python
/// namespace, so a bare `>>> import re` must reach the plan before the
/// `.. doctest::` that calls `re.split`. Interleaving the two forms in one pass
/// is what preserves that.
fn collect_doctest_blocks(doc: &ast::Document) -> Vec<PlanSource<'_>> {
    let mut blocks = Vec::new();
    ast::walk_nodes(&doc.nodes, &mut |node| match node {
        ast::Node::Directive(ast::Directive::DocTest(block)) => {
            blocks.push(PlanSource::Directive(block));
        }
        ast::Node::DoctestBlock(content) => blocks.push(PlanSource::BareBlock(content)),
        _ => {}
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

/// Tests, split by topic rather than kept inline: [`grouping_tests`] for
/// which blocks land in which group, [`conditions_tests`] for run
/// conditions, problem reporting and the presentation firewall, over the
/// shared fixtures in [`test_support`].
#[cfg(test)]
mod conditions_tests;
#[cfg(test)]
mod grouping_tests;
#[cfg(test)]
mod test_support;
