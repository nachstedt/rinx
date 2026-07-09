# Rust Code Quality Review

A manual audit of `crates/` for unidiomatic, unsafe, or suboptimal Rust usage,
conducted 2026-07-09. `unsafe_code = "forbid"` is set workspace-wide (zero
`unsafe` blocks exist) and `cargo clippy --workspace --tests` with
`clippy::pedantic = "warn"` currently produces **zero warnings** — none of the
findings below are things clippy pedantic already catches. This is a living
document: update or remove entries as they're fixed, and add new ones the same
way `spec_gaps.md` is maintained.

Severity scale: **Critical** (panic/crash or injection on plausible input,
reachable from untrusted `.rst` content or an ordinary Bazel action) · **High**
(real bug or significant perf/maintainability cost) · **Medium** (real but
minor cost) · **Low** (nice-to-have polish) · **Nit** (purely cosmetic).

The anonymous-target/merge-retraction High findings below were independently
reproduced (either by compiling a minimal repro or by reading the exact code
path) before being recorded here.

## High

### 4. `ProjectIndex::merge` never retracts stale entries for a shrinking document
`crates/analyzer/src/lib.rs:61-79`. `targets`/`document_titles`/`domain_objects`
use `BTreeMap::extend`, and `glossary_terms` uses per-key `insert` — none of
these remove a key that existed in the stale `self` map but is absent from the
fresh `other` map. The only non-full-rebuild call site,
`process_preview` (`crates/worker/src/main.rs:109-127`, `index.merge(local_index)`
at line 125), merges a freshly re-analyzed single document straight into the
deserialized, potentially-stale global index with no retraction step first.

**Impact:** in live preview, deleting an explicit `.. _label:` target, an H1
heading, or a `.. py:function::` directive from the currently-edited document
leaves it resolvable/visible in the merged index after the debounced
re-render — distinct from (and not covered by) the already-documented "other
documents go stale until rebuild" limitation, since this is about the
*currently edited* document's own deletions not taking effect.

### 5. Duplicate-glossary-term diagnostic is computed but never surfaced (dead in production)
`ProjectIndex::merge` (`crates/analyzer/src/lib.rs:61-79`) returns
`Vec<String>` diagnostics for glossary terms defined in two places, but both
production call sites discard them: `build_project_index`
(`crates/analyzer/src/lib.rs:250`, the real `index` subcommand path) does
`let _ = index.merge(analyze(doc));`, and `process_preview`
(`crates/worker/src/main.rs:125`) does the same. Neither `cmd_index` nor
`process_preview` prints or otherwise surfaces these strings anywhere.
Contrast with broken-link warnings, which *are* surfaced via `eprintln!` at
`crates/worker/src/main.rs:335-337, 427-429`. Two teams (this project is
explicitly built around a multi-team docs split) defining the same `:term:` in
different files — a plausible copy-paste mistake — get silent last-writer-wins
behavior in both the CLI build and live preview, with the diagnostic machinery
fully implemented and unit-tested in isolation but effectively dead in the
binary.

**Fix:** print the returned diagnostics (e.g. alongside the existing broken-link
warnings) in `cmd_index`/`process_preview`.

### 6. Content nested inside a glossary definition is rendered but never indexed
`index_nodes` (`crates/analyzer/src/lib.rs:119-127`) never recurses into
`GlossaryEntry.definition`, unlike the renderer's `render_glossary`
(`crates/renderer/src/directives.rs:196`), which does
`render_nodes(html, &entry.definition, ctx)`. This directly contradicts
`index_nodes`'s own doc comment (lib.rs:102-108), which claims it mirrors
`render_nodes`'s recursion shape specifically so nested content "is still
indexed for cross-references to resolve, exactly like it's still rendered
with a working anchor."

**Impact:** a `Node::Target`, nested domain object, or list/table inside a
glossary definition gets a working HTML anchor but is absent from
`ProjectIndex`. With `--strict-links`/`strict_links = True` (see
`tests/test_strict_links.sh`), a legitimately-rendered target nested in a
glossary entry is reported as a broken cross-reference and fails the build.
Existing tests (`crates/analyzer/src/lib.rs:933-956, 958-974`) both use
`definition: vec![]`, so this gap has no regression coverage.

### 7. Anonymous targets nested in lists/tables are never resolved (false broken-link reports)
`collect_anonymous_targets` (`crates/renderer/src/lib.rs:98-119`) has arms
for admonition/version-change/see-also/domain-object/glossary bodies but none
for `Node::BulletList`, `Node::DefinitionList`, or `Node::Table` — it falls
through to a catch-all `_ => {}`. Confirmed by reading the match directly: any
`Node::AnonymousTarget` nested inside a list item, definition, or table cell
is silently skipped when building `anon_targets`.

**Impact:** a document with an anonymous hyperlink reference in a top-level
paragraph and its matching target nested one level inside a bullet list
renders the reference as a broken link (`class="broken-link"`) and reports a
spurious `BrokenLink`, even though the target genuinely exists — the same bug
class the existing regression test
`test_render_domain_object_resolves_nested_anonymous_hyperlink_in_body` was
added to catch for `DomainObject`, just not generalized to lists/tables. The
catch-all `_ => {}` arm means the compiler won't warn if a future node type
with a nested body is added and also missed here (unlike the exhaustive
matches in `render_nodes`/`render_directive`).

### 8. Duplicate `Node::Target` names and `DomainObject` keys are silently overwritten
`crates/analyzer/src/lib.rs:117` and `:130`: `index.targets.insert(...)` and
`index.domain_objects.insert(...)` are plain last-writer-wins inserts, both
within a single document's `analyze()` and across documents via `merge`'s
`extend` — with zero diagnostic, unlike glossary terms which at least compute
(if don't surface, see #5) a message. Sphinx normally warns on both "duplicate
label" and "duplicate object description." Two files each defining
`.. _overview:` or `.. py:function:: greet` (plausible copy-paste) silently
resolve cross-references to whichever doc happened to be processed last, with
no diagnostic generated at all.

### 9. `TargetName`'s normalization invariant isn't re-validated on deserialize
`crates/ast/src/target_name.rs:7-8`. Unlike its sibling `HashedContent`
(`crates/ast/src/hashed_content.rs`), which re-validates its invariant on
every `Deserialize` via `TryFrom`, `TargetName` derives `Deserialize` directly
on the tuple struct. Its doc comment states the type exists to enforce that
target names are case-collapsed/normalized, but that only happens via
`TargetName::new()` — a `TargetName` loaded from a `project.index` JSON file
(consumed by `render`/`preview`) can carry an unnormalized string, which would
then silently fail to `Eq`/`Hash`-match an otherwise-identical, properly
normalized `TargetName` for the same logical target. This is exactly the
failure mode the "parse, don't validate" pattern (documented in `CLAUDE.md`)
exists to prevent, and this type doesn't follow it.

### 10. `admonition_kind.rs` has zero test coverage
`crates/ast/src/admonition_kind.rs` — a 10-variant enum with hand-maintained
`as_str`/`FromStr`/`Display` — has no `#[cfg(test)]` module at all, in direct
contradiction of this repo's convention (CLAUDE.md: "all functions, including
private/helper ones, should have unit tests"). Every sibling kind enum
(`Domain`, `ObjectType`, `PyObjectType`) has at least round-trip tests; here a
typo in either match arm (e.g. `"important"` ↔ `Important`) would go
undetected.

## Medium

### 11. Unnecessary allocation in `DomainObjectBody` name extraction
`crates/ast/src/domain_object_body.rs:17-24, 92-99`. `extract_object_name`
always materializes an owned `String` via `.to_string()` even though the
slice it returns already borrows from `signature: &str` and could be `&str`
with no allocation; `DomainObjectBody::name()` compounds this with an extra
`.clone()` for the `PyModule`/`PyData` branches. Every caller immediately
re-borrows and discards the result as `&str`
(`crates/analyzer/src/lib.rs:129`, `crates/renderer/src/directives.rs:119-120`),
so an allocation happens per domain object, once per index build and once per
page render, for a value never kept as owned data. `signature_text()` two
lines below already shows the zero-copy alternative (`-> &str`, no clone) is
possible.

### 12. `TableCell::colspan`/`rowspan` invariant ("always >= 1") isn't enforced
`crates/ast/src/table.rs:12-15`. Plain public `usize` fields with a doc
comment claiming an invariant but no smart constructor, breaking with this
crate's own "parse, don't validate" convention used for `HashedContent`/
`TargetName`. A `colspan`/`rowspan` of `0` (from a future parser bug, or a
hand-crafted `.ast` JSON payload, since the fields are `pub` with plain
derived `Deserialize`) is structurally possible and would push any
divide-by-zero risk downstream instead of being made impossible here.

### 13. Several "kind" enums have untested `as_str`/`FromStr`
`crates/ast/src/version_change_kind.rs` only tests serialization round-trips,
not `as_str()`/`FromStr::from_str()` (including the invalid-string rejection
path) or `Display` directly. Same smaller gap in
`crates/ast/src/py_object_type.rs` and `crates/ast/src/c_object_type.rs`:
`as_str()` has no dedicated test in either file, and `PyObjectType::Module`/
`::Data` and `CObjectType::Function`'s string mappings are only ever exercised
indirectly (and only `Function`) via `object_type.rs`'s test.

### 14. Nav-subtree recursion has no memoization or depth guard
`crates/analyzer/src/lib.rs:198-238`, `build_nav_subtree`. A "diamond"
toctree graph (a document reachable from multiple ancestors, sharing further
descendants — confirmed intentionally supported by
`test_build_project_index_allows_shared_node_in_multiple_branches`,
lib.rs:818-865) is recomputed once per incoming path with no memoization, so
several stacked diamonds could blow up combinatorially. Both
`build_nav_subtree` and `index_nodes` are also unbounded plain recursion with
no depth guard — a pathologically deep toctree or nested list/table structure
could stack-overflow the analyzer. Likely low real-world likelihood for
hand-authored docs, but worth watching if large mirrored doc sets (the CPython
benchmark) produce deep/wide toctree graphs.

### 15. `process_preview` duplicates CSS relative-path logic instead of reusing the published helper
`crates/worker/src/main.rs:142-147` reimplements the CSS relative-path
calculation inline (`doc_path.matches('/').count()` + `"../".repeat(depth)`)
instead of calling `renderer::css_relative_path()`
(`crates/renderer/src/page.rs:12-19`), which `process_render` correctly uses
at line 188. Two independent implementations of the same logic can silently
drift — a future edge-case fix to `css_relative_path` won't propagate to
preview, producing preview-only CSS-link bugs that are hard to reproduce
because `render` behaves differently by then.

### 16. Single-value CLI flag parsing doesn't detect a missing value
`crates/worker/src/main.rs:34-44` (`flag_value`) never checks that the token
following a flag isn't itself another flag. If a flag's value is accidentally
omitted (e.g. a misconfigured Bazel rule emits `--output --config foo.toml`),
`flag_value` silently returns `"--config"` as `--output`'s value rather than
erroring — producing a confusing downstream file-I/O error instead of a clear
CLI diagnostic. `cmd_validate_images`'s `flag_value(args, "--output").ok()`
(line 373) compounds this: a dangling `--output` with no value is silently
treated as "no output requested." The multi-value variants (`flag_values`/
`flag_values_opt`, lines 53-80) already guard against this via
`take_while(|a| !a.starts_with("--"))` — the inconsistency between the
single- and multi-value parsers is itself a maintenance trap.

## Low

### 17. `HashedContent`'s validation error is a bare `String`
`crates/ast/src/hashed_content.rs:50-51`. Fine today (one failure mode: hash
mismatch), but callers can't match on failure kind without string inspection
if a second validation rule is ever added.

### 18. Page-title extraction duplicated between `process_render` and `process_preview`
`crates/worker/src/main.rs:129-140` vs. `176-186` — the "extract page title
from the first H1" block is duplicated verbatim (only the fallback value
differs). Should be one named helper per this repo's convention of naming and
independently testing concrete helper functions.

### 19. Manual CLI argument parsing instead of `clap`
`crates/worker/src/main.rs:34-80`. Hand-rolled flag parsing means no
`--help`, no typed values, and no unknown-flag detection (e.g.
`parse --input a.rst --output b.ast --bogus xyz` silently ignores `--bogus`).
With 6 subcommands and ~20 flags already, this is a growing amount of
hand-maintained parsing/edge-case test surface a derive macro would give for
free.

### 20. Duplicated domain-object-role handler boilerplate
`crates/parser/src/inline.rs:145-210` — `handle_func_match`,
`handle_mod_match`, `handle_data_match` each repeat the same "resolve
optional `domain` capture, fall back to `default_domain`, build
`DomainObjectReference`" shape. A single parameterized helper would remove
~40 lines of near-identical code.

### 21. Glossary sort recomputes `.to_lowercase()` per comparison
`crates/parser/src/glossary.rs:116-128`. `entries.sort_by` allocates a fresh
lowercased `String` on both sides for every comparison instead of computing
each entry's sort key once (`sort_by_key` with a precomputed key /
Schwartzian transform). Low-impact given typical glossary sizes, but an easy,
idiomatic fix.

### 22. Inconsistent escaping context for enum-derived strings written into HTML attributes
`crates/renderer/src/directives.rs:28-29` (`encode_text`, a text-context
escaper, used inside `class="admonition {kind_escaped}"`) and `:78`
(`writeln!(html, "<div class=\"{kind_str}\">")`, no escaping at all) put
values into an attribute using text-context escaping or none, versus the rest
of the crate's careful attribute-vs-text split (`inline.rs`'s
`encode_double_quoted_attribute` for `href`/`class` vs. `encode_text` for
element content). Currently harmless because the source strings are fixed
Rust enum values, not user data — but it'd become exploitable the moment
either enum grows a user-suppliable "kind."

### 23. Avoidable per-heading string allocation
`crates/renderer/src/lib.rs:165`: `let tag = format!("h{}", (*level).clamp(1, 6));`
allocates a `String` per heading purely to interpolate into `<{tag}>`/`</{tag}>`.
Can be `write!(html, "<h{level}>")`/`write!(html, "</h{level}>")` directly with
the clamped value, no allocation — worth doing given the CPython-docs
benchmark target has many headings.

### 24. Unnecessary owned-`String` copy in `render_admonition`
`crates/renderer/src/directives.rs:18-26` always materializes an owned
`String` for `title_text` (`String::from(title)`) even when `title: Option<&str>`
already borrows the text; a `Cow<str>` would avoid the copy when a custom
title is present.

### 25. Missing escaping regression test for `render_page`; `collect_anonymous_targets` untested directly
`crates/renderer/src/page.rs`'s test module has no analogue of `lib.rs`'s
`test_render_escapes_html_special_characters` covering `page_title`/
`project`/nav titles — almost certainly why finding #1 went unnoticed.
Separately, `collect_anonymous_targets` (a private helper, which per CLAUDE.md
should have its own unit test) is only exercised indirectly through
full-document `render()` tests, and is exactly the function with the bug in
finding #7.

### 26. Role-dispatch `.captures(m_str).unwrap()` calls document their safety invariant inconsistently
`crates/parser/src/inline.rs:146, 167, 193, 221, 225, 232, 250, 265, 273, 285`
— ten call sites re-match `m_str` against the same regex that already found
it in `parse_inline_text`'s `kind`-tagged dispatch (`"func"` →
`FUNC_ROLE_REGEX`, `"ref"` → `REF_REGEX`, etc.), then bare-`.unwrap()` the
resulting `Option<Captures>`. This can't panic today — the `kind` tag is
always paired with the regex that produced `m_str` — but that's an implicit
invariant enforced by convention across ~10 separate branches, not by the
type system, and none of the ten sites say so. Contrast with
`handle_func_match` two lines below one of them (line 154 in the same file),
which documents an analogous guaranteed-safe unwrap explicitly: `.expect("every
domain defines a 'func' role")`. A future contributor adding an eleventh
role/regex pair and mismatching the `kind` string would get a bare "unwrap on
a `None` value" panic with no hint that the real bug is a tag/regex mismatch.

**Fix:** replace the ten bare `.unwrap()`s with `.expect("kind dispatch
guarantees this regex already matched m_str")` (or equivalent), matching the
self-documenting convention already used at line 154.

## Nit

### 27. `merge`'s glossary loop does a `get` then an unconditional `insert`
`crates/analyzer/src/lib.rs:67-77` — two map traversals where `Entry` would
do one. Minor constant-factor cost on the per-keystroke preview hot path; not
caught by clippy's `map_entry` lint because the `insert` isn't guarded by the
same `if`/`else` shape the lint pattern-matches on.

### 28. `normalize_path` tested twice
`crates/analyzer/src/utils.rs:19-34` and again through the `pub use` re-export
in `crates/analyzer/src/lib.rs`'s test module (lib.rs:351-424) — near-duplicate
cases in both places. Harmless, but consolidating would reduce maintenance
surface without losing coverage.

### 29. `flag_values`/`flag_values_opt` are near-duplicate implementations
`crates/worker/src/main.rs:53-80` — `flag_values_opt` could trivially be
`flag_values(...).unwrap_or_default()` instead of a second hand-written copy.

### 30. `integration_test.rs` doesn't follow the repo's Given-When-Then convention
`crates/worker/tests/integration_test.rs` has no `// Given`/`// When`/
`// Then` comments, unlike every unit-test module under `crates/*/src/`.
Additionally, `test_parser_step` and `test_renderer_step` call
`rusty_sphinx_parser`/`rusty_sphinx_renderer` directly and never touch
`rusty_sphinx_worker`'s own public API (`process_rst`, `validator::validate_toctree`)
— they're spot-checks of upstream crates that already carry their own unit
tests, not integration tests of this crate. Only `test_e2e_translation`/
`test_e2e_multi_level_headings` actually exercise the crate under test.

## What's already solid

Worth naming so it doesn't get re-litigated: `unsafe_code = "forbid"` is
respected everywhere; `validator.rs` is small, fully unit-tested, and has no
panic paths; the `process_*`/`cmd_*` split described in `CLAUDE.md` is
followed consistently across all six worker subcommands, with good
`.context()` error messages on every I/O boundary; no PlantUML/subprocess
command-injection surface exists in the worker crate; `ProjectIndex`'s fields
are all `BTreeMap`, so no `HashMap`-iteration-order nondeterminism leaks into
Bazel-cached output; and all `panic!`/`unreachable!()` hits found via grep
across the parser crate are test-module assertion helpers, not
production-reachable panics.
