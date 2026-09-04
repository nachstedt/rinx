# Development Guidelines

What this repository has learned about how it wants to be built. Add a new
entry under the heading it belongs to, as a single short sentence.

## About these guidelines

- Guidelines are beliefs, preferences and style choices, not factual statements about tools or technologies.

## Naming

- Name types after what they do, not what they are used for.
- Function names should describe what the function concretely does, not just reflect its abstract role (e.g., prefer `build_project_index()` over `analyze_many()`).

## Types and invariants

- Enforce invariants at the type level using opaque types with smart constructors ("parse, don't validate").
- In a build system, deserialization should validate invariants strictly and return errors on violations.
- Use enums for fields with a fixed set of predefined values to ensure type safety.
- When a family of related kinds needs different options per kind, give each kind its own explicit variant/type with only its own fields, rather than one shared struct/variant carrying the union of every kind's options (most of them meaningless on most instances).
- When several values play the same role, model them as one uniform collection rather than splitting the first out into its own field; if one of them is special, express that as a named accessor on the collection, not as a structural asymmetry.
- Give two constructs one shared type when they are structurally identical and carry no kind-specific data; split them into separate types only when they differ in the fields they need.
- Model a family of valueless flag options as one set of an enum rather than as several `bool` fields, so asking for one cannot be confused with asking for its neighbour.
- Prefer a type that makes a meaningless value unrepresentable (`NonZeroUsize` for a depth) and resolve the ambiguity once, where the source text is read.

## Module and file organization

- Prefer co-locating unit tests natively using inline `#[cfg(test)]` modules over splitting them into separate files with `include!` macros to satisfy arbitrary line-limit constraints.
- When two roles/directives share the same sub-syntax (e.g. the `Display text <target>` explicit-title form), extract one shared helper and reuse it rather than re-implementing the same parsing logic per role.
- Keep logic in the pipeline phase (and crate) that actually performs it; do not move it next to the data type it reads just because that type is defined elsewhere.
- Split any file that grows past roughly 800 lines, along construct or responsibility boundaries rather than at an arbitrary midpoint.
- Express a parent/child module relationship with a same-named subdirectory (`inline/reference.rs`), never with a filename prefix (`inline_reference.rs`).
- A module file that owns a same-named directory should be a pure forwarder: a module doc comment, `mod` declarations, and re-exports, with no logic or tests of its own.
- Place a module under whichever dispatcher actually calls it, which means grepping for its real callers rather than assuming from its name.
- Suspect any file named after a construct that also holds general-purpose helpers, and split the helpers out under a name describing what they do.
- Keep a helper flat at the crate's `src/` root only when it is genuinely reached from several sibling trees; nesting it under one construct's directory would misstate the dependency.
- Give shared logic its own crate when the phases needing it are forbidden to depend on each other, rather than reintroducing a dependency an earlier split was made to remove.
- Do not separate a type from the only code that constructs and reads it; modules that merely thread a value through their signatures are not users of it.
- Treat two functions with identical bodies under different names as one function, and delete the duplicate rather than relocating both.
- Prefer a plain private function in a parent module over a `pub(super)` one when only that module's own descendants call it.
- Give an item re-exported by a forwarder a visibility at least as wide as the re-export itself, which in practice means `pub(crate)` rather than `pub(super)`.
- Suffix a module name with an underscore when the natural name is a reserved word (`std_`, `mod_`, `type_`, `struct_`).
- Keep an implementation whole and split only its `#[cfg(test)]` content into topic-named sibling files when the implementation is one cohesive unit.
- A file that owns a test-only directory keeps no inline `#[cfg(test)]` module; all of its tests live in the topic-named siblings.
- Drop a module-name component the enclosing crate or directory already states (`scope/python.rs`, not `scope/python_scope.rs`).
- Give fixtures shared by two sibling test modules their own `#[cfg(test)]` support module rather than copying them into both.

## Refactoring safely

- Never mechanically re-indent code while moving it, since blanket whitespace edits silently rewrite string literals; move the text verbatim and let `cargo fmt` re-indent.
- Treat a pure reorganization as behaviour-preserving and prove it, by comparing test counts before and after and diffing the moved files' string literals against the previous revision.
- Check that every relocated test still carries its `#[test]` attribute, since a dropped attribute turns a test into dead code that clippy reports only as an unused function.

## Parsing and AST design

- When adding a construct whose delimiter characters can collide with an existing construct's, fix the ambiguity at the root by tightening the existing detector to match the spec, rather than relying on parser dispatch order.
- When constructs depend on exact character alignment (e.g. grid tables), validate the alignment strictly and reject on mismatch instead of silently padding lines to paper over it.
- When a prefix/sigil is markup rather than part of a name, strip it during parsing and record its meaning as typed intent on the AST node, rather than leaving it in the string for a later phase to re-interpret.
- Two directives that differ only in how their source spells out the same data should share one AST node, with a small enum recording which one wrote it.
- Keep parse-time configuration in one context object threaded through the parsers, rather than adding a parameter per setting.
- A pure phase should take an injected trait object for anything it cannot do itself (like reading a file) rather than acquiring the capability directly.
- Convert an embedded notation (LaTeX, CSV) with an established library during the build rather than shipping a client-side script, so the output stays self-contained and malformed input becomes a build diagnostic instead of a silent failure in the reader's browser.
- Store an embedded notation in the AST exactly as written and convert it only while rendering, so the choice of backend never reaches a serialized `.ast` file.
- Leave a pattern unexpanded in the AST and the index when several later phases must expand it against different sets, so no phase has to reimplement the matcher against its own narrower set.
- Prefer storing a graph plus each node's own data over a pre-flattened tree, since a flattened tree cannot be merged per document and loses the per-directive options that produced it.

## Porting from a reference implementation

- When emulating a reference implementation, deviate deliberately where it is silently permissive: keep the type/kind check on every lookup path even if the original skips it, so an author's mistake is reported instead of resolved to something plausible.
- When a reference implementation's subsystems implement the same rule with different strictness, reproduce each subsystem's rule rather than unifying them behind the more lenient one.
- When porting logic from a reference implementation, reproduce its observable outcome rather than its literal operation, since your own normalizing types can silently turn its miss into a hit.
- When the reference implementation's output discards information the source expressed, render that information faithfully instead of reproducing the loss, and say so in a comment so nobody "fixes" it back.
- Where a third-party library cannot reproduce the reference implementation's behaviour exactly, pick the narrower behaviour, diagnose what you refuse, and comment why so nobody widens it by accident.
- When the reference implementation defines a rule over full Unicode character classes, port those classes rather than an ASCII approximation that happens to satisfy the current tests.
- Transcribe a reference implementation's pre-generated tables rather than re-deriving them, so the two cannot drift apart as either side's inputs change; fetch the actual source rather than reconstructing a table from memory.

## Diagnostics

- Documentation builds should fail loudly if content invariants (like missing diagram images) are violated.
- Keep the resilient parser and the strict build separate: the parser degrades bad input and records what went wrong, and the build step decides whether that is fatal.
- When a lookup is genuinely ambiguous, do not pick a winner — leave it unresolved and emit a warning that names every candidate, so the diagnostic tells the author what to disambiguate between.
- When porting a reference implementation, port its full diagnostic set, and additionally invent diagnostics of your own wherever it silently degrades valid-looking input into something else.
- When two phases must derive the same identifier, give them one function to call rather than two implementations to keep in step, and key it on something that cannot collide (a position) rather than on content that can.
- Before keeping an invented diagnostic, measure its false-positive rate over the benchmark corpus; a heuristic that stays silent across real documents is safe to keep unnarrowed.
- Every diagnostic carries a source position and a stable code; the position goes in a span and the code in an enum, never formatted into the message text.
- Report a position as a range rather than a point, even while only its start is printed, because the end is free wherever the start is and retrofitting it later re-touches every reporting site.
- Name a file a reader can actually open in a diagnostic — the source path, not an internal logical path that merely looks like one.
- Count columns in characters, not bytes, and convert to a protocol's encoding at that protocol's boundary rather than in the middle.
- Any code that dedents or re-slices lines before parsing them must rebase the position context by the same amount, so a helper that trims should return how much it trimmed rather than discarding it.
- Report no position rather than a wrong one for content that corresponds to no source line.
- Author-facing suppression belongs in the document, spelled as a construct the reference implementation already ignores, so the file stays buildable by both tools.
- Apply suppression at the reporting boundary, never where a problem is detected: the detecting phase must record everything, and only the reporting phase sees every phase's output.
- A suppression must lift the consequence as well as the message — the strict-build failure and the machine-readable sidecar too — or it only hides the problem.
- Diagnose a suppression that names an unknown code, since one that silently matches nothing is the same silent degradation the diagnostics exist to catch.
- Generate an enum's string mapping and its parse-back from one table when both directions must be exact inverses.

## Testing

- All functions — including private and helper functions — should be thoroughly unit tested.
- Tests should always follow the Given-When-Then pattern.
- Always create unit tests for new functions introduced during refactoring.
- Validate real-world/benchmark inputs against a parser before considering a feature done — a hand-built test suite can miss patterns (like a border line with a partial `+` set) that only show up in authentic external documents.
- When deleting tests whose subject moved to another phase, check the destination already covers those cases and add an integration test for the seam, rather than letting the coverage go with the code.
- Do not widen a public API to serve a test; keep test-only helpers and fixtures inside the `#[cfg(test)]` module that needs them.
- When a table of related cases is maintained by hand, test its structural invariants (reflexivity, symmetry) across all entries rather than only asserting the individual rows, so a half-finished edit fails.

## Configuration and CLI

- Prefer a single configuration file over accumulating individual CLI flags; adding future fields requires no interface changes.
- Configuration files should contain metadata and settings, not file paths; sandboxed build systems relocate files, breaking embedded paths.
- Relative file paths work correctly for offline viewing; do not use inlining as a workaround for path computation.

## Build, examples and documentation

- Always add examples for all variants of a new feature to the example project.
- When adding new `.rst` files to the example project, always declare them in `srcs` in the corresponding `BUILD.bazel` file.
- Derive a target's inputs from the target it depends on rather than repeating them in the build file; where the build system cannot express that, choose the coarser granularity over a duplicated naming convention.
- Give data a build target reads its own attribute, separate from the attribute that declares dependencies on other targets.
- A test that proves a build declaration is load-bearing must also force the affected action to re-run, since Bazel does not invalidate an action when an input is merely removed.
- Resolve a tool from `PATH` in a test script rather than hardcoding its install location.
- In spec_gaps.md, mark a row 🔶 partial rather than ✅ whenever its own Notes say a specific argument/option/variant of that feature is unsupported, even though the feature's core case works.

## Workflow and commits

- After every code modification session, run `cargo clippy --tests` and resolve all warnings before finishing.
- After every code modification session, run `cargo fmt`.
- Do not commit changes unless explicitly requested by the user.
- Give every commit a conventional-commit subject line, e.g. `feat: support simple tables`.
- Always follow that subject with a body summarizing at a high level what the commit does and the notable decisions behind it; a bare subject line is never enough.
- After every code modification session, verify `bazel build //examples:site` succeeds.
- Treat clippy warnings as refactoring opportunities; do not suppress them with `#[allow(...)]` attributes.
