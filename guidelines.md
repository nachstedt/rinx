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
- When a value is transformed once and then must never be re-transformed, give the result its own type that cannot express the untransformed state, so forgetting the step is a compile error.
- Keep the open-ended case of an enum as a validated newtype rather than enumerating a third party's whole vocabulary, which would import their knowledge into your own root crate and freeze it into serialized data.
- Reserve special values of a field as their own variants, so a name that happens to collide with one cannot be mistaken for it.
- Box a large enum variant's payload into its own struct, since an enum costs its largest variant everywhere it is stored.
- When a construct's vocabulary is declared by the user, validate it once at the boundary and let every later phase read data already checked, rather than carrying a dynamically-typed value that each use re-inspects.
- Separate a *value* from a *document* at the type level: something parsed into a node tree and something stored as a typed scalar diverge in every later phase, so one field kind with a "parsed" flag would carry the union of both meanings everywhere.

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
- Split a meta-model from the instances it describes along the dependency arrow: what is written in a source file and survives into a build artefact belongs to the artefact's own crate.
- Ask another crate for a fact it owns through an injected trait rather than keeping a second copy of it, since the copy drifts exactly when the check matters most.
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
- Two directives asking one question with different presentations stay separate nodes; put the generality in the layer that answers the question, not in one directive with a mode switch whose options are each valid in only one mode.
- Keep parse-time configuration in one context object threaded through the parsers, rather than adding a parameter per setting.
- A pure phase should take an injected trait object for anything it cannot do itself (like reading a file) rather than acquiring the capability directly.
- Convert an embedded notation (LaTeX, CSV) with an established library during the build rather than shipping a client-side script, so the output stays self-contained and malformed input becomes a build diagnostic instead of a silent failure in the reader's browser.
- Let the build system's constraints decide a third-party library, not the library's feature list: a dependency needing a foreign toolchain costs more in a sandboxed, multi-platform build than any feature it adds.
- Generate a derived artefact (a stylesheet, a table) from the same source the code uses, and check it in with a test that regenerates and compares, rather than maintaining a parallel copy by hand.
- Before generating and vendoring a third party's data set, look for a crate that already redistributes it; a maintained redistribution tracks upstream for free, where a checked-in copy is ours to re-cut forever.
- Store an embedded notation in the AST exactly as written and convert it only while rendering, so the choice of backend never reaches a serialized `.ast` file.
- Leave a pattern unexpanded in the AST and the index when several later phases must expand it against different sets, so no phase has to reimplement the matcher against its own narrower set.
- Prefer storing a graph plus each node's own data over a pre-flattened tree, since a flattened tree cannot be merged per document and loses the per-directive options that produced it.
- Declare a relationship on the side that writes it, so the declared source can never drift from actual use, and derive the reverse direction rather than asking for it twice.
- Give a generated identifier a deterministic source (position, not content) so a cached build artefact stays byte-identical, and say plainly which edits perturb it.
- Store only what a later reader needs in a shared index: prose belongs in the document, not in an artefact every phase loads.
- When implementing a subset of a language a reference implementation embeds whole, recognise the constructs outside the subset and reject each *by name*, since the common failure is an unsupported feature rather than a typo.
- Choose a parsing library for what it produces, not for what it saves: one that hands back a token tree still leaves the tree-building, the error wording and the panic-freedom to write by hand.
- Weigh a library against the features still to come, not only the one at hand: accepting a vendored asset and a larger output is worth it when the next construct in the same family reuses the library's harder parts instead of hand-rolling them too.
- Pin a rendering library to an asset-free, host-independent configuration and vendor whatever it then needs, since anything it resolves from the build host makes a cached artefact differ between machines.

## Porting from a reference implementation

- When emulating a reference implementation, deviate deliberately where it is silently permissive: keep the type/kind check on every lookup path even if the original skips it, so an author's mistake is reported instead of resolved to something plausible.
- When a reference implementation's subsystems implement the same rule with different strictness, reproduce each subsystem's rule rather than unifying them behind the more lenient one.
- When porting logic from a reference implementation, reproduce its observable outcome rather than its literal operation, since your own normalizing types can silently turn its miss into a hit.
- When the reference implementation's output discards information the source expressed, render that information faithfully instead of reproducing the loss, and say so in a comment so nobody "fixes" it back.
- Where a third-party library cannot reproduce the reference implementation's behaviour exactly, pick the narrower behaviour, diagnose what you refuse, and comment why so nobody widens it by accident.
- Prefer a declarative property that states what the reference implementation computes by measuring, since the target platform can measure later and better than the build can.
- When the reference implementation defines a rule over full Unicode character classes, port those classes rather than an ASCII approximation that happens to satisfy the current tests.
- Transcribe a reference implementation's pre-generated tables rather than re-deriving them, so the two cannot drift apart as either side's inputs change; fetch the actual source rather than reconstructing a table from memory.

## Diagnostics

- Documentation builds should fail loudly if content invariants (like missing diagram images) are violated.
- Keep the resilient parser and the strict build separate: the parser degrades bad input and records what went wrong, and the build step decides whether that is fatal.
- Do not diagnose a failure the author could not have caused or acted on: a value this codebase chose for them degrades silently, while one they wrote is reported.
- When a lookup is genuinely ambiguous, do not pick a winner — leave it unresolved and emit a warning that names every candidate, so the diagnostic tells the author what to disambiguate between.
- When porting a reference implementation, port its full diagnostic set, and additionally invent diagnostics of your own wherever it silently degrades valid-looking input into something else.
- When two phases must derive the same identifier, give them one function to call rather than two implementations to keep in step, and key it on something that cannot collide (a position) rather than on content that can.
- Before keeping an invented diagnostic, measure its false-positive rate over the benchmark corpus; a heuristic that stays silent across real documents is safe to keep unnarrowed.
- Every diagnostic carries a source position and a stable code; the position goes in a span and the code in an enum, never formatted into the message text.
- Report a position as a range rather than a point, even while only its start is printed, because the end is free wherever the start is and retrofitting it later re-touches every reporting site.
- Name a file a reader can actually open in a diagnostic — the source path, not an internal logical path that merely looks like one.
- Count columns in characters, not bytes, and convert to a protocol's encoding at that protocol's boundary rather than in the middle.
- Measure a source line against author-drawn markup in display columns, since the author drew it under what they saw, not under its encoded length.
- Any code that dedents or re-slices lines before parsing them must rebase the position context by the same amount, so a helper that trims should return how much it trimmed rather than discarding it.
- Report no position rather than a wrong one for content that corresponds to no source line.
- Author-facing suppression belongs in the document, spelled as a construct the reference implementation already ignores, so the file stays buildable by both tools.
- Apply suppression at the reporting boundary, never where a problem is detected: the detecting phase must record everything, and only the reporting phase sees every phase's output.
- A suppression must lift the consequence as well as the message — the strict-build failure and the machine-readable sidecar too — or it only hides the problem.
- Diagnose a suppression that names an unknown code, since one that silently matches nothing is the same silent degradation the diagnostics exist to catch.
- Generate an enum's string mapping and its parse-back from one table when both directions must be exact inverses.
- Build a feature's diagnostics with the feature, not after it: deferring them ships the silent degradation the diagnostics exist to catch, and retrofitting one usually turns out to need a data-model change the feature could have made for free.
- Attach a position's *file* to the position itself, not to the diagnostic quoting it, when a later phase reads that position out of a serialized artifact — a file recorded only on parse-time diagnostics cannot serve the render-time ones.
- Intern a repeated identifier to a small integer rather than storing the string, when the type carrying it is `Copy` and appears once per node; the indirection buys back both the trait and the wire size.
- Route every construction of a position-bearing value through one function once any of them needs extra context, so a site that skipped it cannot compile rather than silently reporting against the wrong file.
- Scope a suppression to the file it was written in; line numbers from two different files are not comparable, and matching them silences something the author never looked at.
- Distinguish a fault in the *build's configuration* from a fault in a document: the first is reported outside the suppression mechanism, since no document's comment should be able to silence it.
- When a pattern matches far more than the construct it is looking for, let an unrecognised match degrade to plain text rather than to a diagnostic — reporting on text the author never meant as markup is worse than staying quiet.
- Render an unrecognized construct visibly, quoting its source, rather than omitting it: invisible content loss is worse than an ugly page.
- Distinguish "this build does not know that name" from "it knows the name and had to refuse the content", since one diagnostic for both blames the wrong thing.
- Build the node a reader sees and the diagnostic a log records from one message, so the page and the build log cannot explain the same failure differently.
- Check a configuration invariant in both directions: the mistake of declaring something in one place and forgetting it in another is likelier than declaring it wrongly twice.

## Testing

- All functions — including private and helper functions — should be thoroughly unit tested.
- Tests should always follow the Given-When-Then pattern.
- Always create unit tests for new functions introduced during refactoring.
- Validate real-world/benchmark inputs against a parser before considering a feature done — a hand-built test suite can miss patterns (like a border line with a partial `+` set) that only show up in authentic external documents.
- When deleting tests whose subject moved to another phase, check the destination already covers those cases and add an integration test for the seam, rather than letting the coverage go with the code.
- Do not widen a public API to serve a test; keep test-only helpers and fixtures inside the `#[cfg(test)]` module that needs them, asserting through an existing observable effect instead.
- Check a test's premise against what the parser can actually produce before treating its failure as a bug in the code.
- Assert on the specific marker a feature emits rather than on a substring that a shared wrapper also contains, or the test stops distinguishing the two.
- When a table of related cases is maintained by hand, test its structural invariants (reflexivity, symmetry) across all entries rather than only asserting the individual rows, so a half-finished edit fails.
- Test a hand-maintained list against the enum it mirrors where one exists, and settle for a representative sample where none does, rather than writing a probe so general it fights every construct's own input requirements.
- Assert on the markers a renderer emits, not on the exact whitespace between them, or the test breaks on formatting that no reader would notice.

## Configuration and CLI

- Prefer a single configuration file over accumulating individual CLI flags; adding future fields requires no interface changes.
- Keep a resource's *name* in configuration and its *path* on the command line, so a sandboxed build can relocate the file without invalidating the config that refers to it.
- Configuration files should contain metadata and settings, not file paths; sandboxed build systems relocate files, breaking embedded paths.
- Check whether a rule about configuration is about *paths* or about *phases* before invoking it: a config file the parser reads is fine, and a workspace-relative path survives relocation — what a config file cannot do is declare a build input.
- Relative file paths work correctly for offline viewing; do not use inlining as a workaround for path computation.

## Build, examples and documentation

- Always add examples for all variants of a new feature to the example project.
- When adding new `.rst` files to the example project, always declare them in `srcs` in the corresponding `BUILD.bazel` file.
- Derive a target's inputs from the target it depends on rather than repeating them in the build file; where the build system cannot express that, choose the coarser granularity over a duplicated naming convention.
- Give data a build target reads its own attribute, separate from the attribute that declares dependencies on other targets.
- Name that attribute after *when* the build reads the files rather than after the one directive that first needed them, so the next reader of the same phase joins it instead of adding a third overlapping attribute.
- Before 1.0, rename an attribute outright rather than keeping the old spelling as an alias: one honest name is worth more than the churn of updating every call site.
- A test that proves a build declaration is load-bearing must also force the affected action to re-run, since Bazel does not invalidate an action when an input is merely removed.
- Resolve a tool from `PATH` in a test script rather than hardcoding its install location.
- In spec_gaps.md, mark a row 🔶 partial rather than ✅ whenever its own Notes say a specific argument/option/variant of that feature is unsupported, even though the feature's core case works.

## Interoperability

- Give a borrowed construct this project's own name and accept the other tool's spelling as an alias, so a migrating project keeps its documents without the foreign vocabulary becoming ours.
- Name a diagnostic code after the construct rather than the spelling it was written under, so one code serves every alias of it.
- Default a borrowed construct's options to what this project's own schema declares, not to the vocabulary the other tool hardcodes, and say in the ADR where the two deliberately disagree.
- When neither the other tool's default nor a schema-derived one would give a meaningful result, make the option mandatory and name the valid choices in the diagnostic, rather than defaulting to a plausible but wrong output.
- Do not reproduce a reference implementation's accidental duplication (e.g. per-start state that redraws the same message); deduplicate and record the deviation in the ADR.
- Generate a derived construct's content as data rather than lowering it to a template of another construct: the shortcut works, but every diagnostic then lands on text the author never wrote.
- Do not reproduce a structural node a reference implementation emits as plumbing, when wrapping the content would defeat the structure the construct exists to enable; splice it transparently and say in a comment why the wrapper is missing.
- Read a reference implementation's actual source before porting it, rather than a neighbouring construct that solves the same problem: two tools' answers to one question differ in exactly the details worth porting.
- Keep this project's own name unclaimed when a borrowed construct is only a compatibility bridge, adopt the other tool's spelling alone, and say in the ADR what the good name is being saved for.
- Ignore a foreign format's bookkeeping fields by an explicit list and report every other unrecognised one, since silently ignoring them all drops exactly the data the import exists to carry.
- Put an imported construct through the very same validation funnel the written one uses, so a value cannot be accepted by one path and refused by the other.
- Separate "this argument was never a path" from "this path could not be read" before touching the filesystem, since a reference implementation's config may resolve the argument first and blaming the filesystem for a missing config fails the build for a document that is not wrong.
- An exported record carries every registered field whether set or not, so treat an unrecognised empty value — or an explicit null — as absent rather than as something to report.
- Declare a name a *document* writes in the project's own configuration rather than in the build file, so the meaning travels with the documents that depend on it.
- When a borrowed construct has a sibling that differs in exactly one field, model that field as an enum from the first commit — one variant is enough — so adding the sibling later is a new arm rather than a change to every match and every already-serialized file.
- When a borrowed feature depends on reference behaviour the local implementation lacks (a bare `:ref:` showing its section title), implement that behaviour for the local path too rather than only for the new one.

## Scope of a guideline

- A guideline that bars a technique bars it as a workaround, not as the feature the user explicitly asked for.

## Workflow and commits

- After every code modification session, run `cargo clippy --tests` and resolve all warnings before finishing.
- After every code modification session, run `cargo fmt`.
- Do not commit changes unless explicitly requested by the user.
- Give every commit a conventional-commit subject line, e.g. `feat: support simple tables`.
- Always follow that subject with a body summarizing at a high level what the commit does and the notable decisions behind it; a bare subject line is never enough.
- After every code modification session, verify `bazel build //examples:site` succeeds.
- Treat clippy warnings as refactoring opportunities; do not suppress them with `#[allow(...)]` attributes.
- Style a new rendered construct in both `assets/default.css` and the inline `<style>` of `examples/custom_template.html`; the example site uses the latter, so editing only the former leaves the example unstyled.
- Verify a rendering change by screenshotting the built page in headless Chrome, not by reading the emitted HTML — markup that looks correct can still render as run-together text.
- Settle a CSS or browser-behaviour question by testing it in a headless browser, not from memory; `<details>`, in particular, hides every child but its `<summary>` and no rule exempts one.
- Keep presentation out of the entity meta-model: `entities.toml` is a parse-time input, so a flag there re-parses every document to change how something looks; put the switch in the render-time site config instead.
- When a template must reproduce what built-in rendering does, give it the declared labels and the document order rather than letting it hardcode a second copy of either.
- Render a declared vocabulary's parts in the order the schema declares them, not in document order or a map's key order; instances of one type should be comparable at a glance.
- When adding a rule for a class on an element a generic selector already styles (`.document code`, `table th`), match that selector's specificity or the new rule silently loses.
- When two phases must agree on a derived value, have both call one pure function rather than passing a sidecar between them; a sidecar makes the agreement a protocol, a shared function makes it impossible to break.
- When a second construct needs the same reporting plumbing, widen the existing error type with a code field rather than adding a parallel one; the duplicate would have to re-derive every downstream rule, including the ones nothing tests, like staying quiet in live preview.
- A value that two callers each supply separately is the one input they can supply differently — put it on the data being processed, not in a context each of them fills in.
- Measure a plausible-sounding build optimization before keeping it; batching many small Bazel actions into one action with many outputs can be slower than the actions it replaced.
- Record what an accepted regression actually costs, measured, in the ADR that accepts it — "some overhead" is not a decision anyone can revisit later.
- When a build system cannot honour an option's semantics (a `:save:` writing an undeclared path), diagnose it by name and point at what does work, rather than ignoring it or dropping the directive.
- Pay only for what you use: when the build system cannot discover whether a feature is used before running actions, make it an explicit per-library opt-in rather than a cost every document of every project pays.
- An opt-in is only honest if forgetting it fails loudly, at the line that needed it, naming the switch to flip.
- A feature that declares no build action of its own needs no opt-in at all: the per-library switch exists because actions are created before any document is read, so a step folded into an action that was already running is free and should stay unconditional.
- When two actions compute the same thing from the same inputs, fold the second into the first rather than keeping them in step; the cheapest agreement is one process.
- Check a generated artifact for the degenerate case the external tool it is fed to rejects; an empty PlantUML diagram fails the build with a syntax error naming a file nobody wrote, when the real cause is a filter that matched nothing.
- Run every shell test the CI workflow runs before calling a change done, and add new ones to that workflow in the same change.
- Make a feature opt-in when its trigger is a character sequence ordinary prose already contains; a construct nobody writes by accident (a directive name) can be always available, but `{{` cannot.
- Narrow an opt-in further with a cheap lexical check, so that within an opted-in library only the documents actually using the feature are subject to its rules.
- When emulating something another tool gets from a scripting hook, follow that tool's own resolution rules rather than the local convention, and say in the ADR where the two deliberately disagree.
- A transform whose output only ever feeds the next step of the same process needs no intermediate artifact; splitting it into a pipeline phase costs a second file for the metadata that cannot be reconstructed from the first, with no cache firewall to win. Give it a debugging flag instead.
- When a transform makes an existing position mapping non-affine, extend the lookup rather than the offset: consult the map once, at the single point that resolves a position, after every composable offset has been added.
- Refuse a construct whose semantics the surrounding mechanism cannot preserve (a whitespace trim that would swallow an injected marker) by name, rather than rendering something subtly wrong.
- When reimplementing a third party's directive, emit its own element structure and class names verbatim, so a project already styled for that extension keeps its appearance; the local rendering is what may differ, never the contract the stylesheet is written against.
- A container directive is worth implementing for what it *contains*: an unrecognized one never parses its body, so every construct inside it — targets, entities, diagrams — silently vanishes from every later phase.
- In a container, report a misplaced child and keep it rather than dropping it, just as an unreadable option must never cost the content written inside; both failures trade a visible warning for invisible content loss.
- Adding a block-level container means adding an arm to *every* traversal that walks block content, not only the renderer's; grep for an existing container's variant and follow it everywhere it appears.
- Keep a CSS width or layout rule at the specificity the upstream framework gives it; raising it (`> .sd-col` for `> *`) silently prevents the more specific override the framework expects to win.
- When a backend library cannot express a commonly used option (text at any angle) but the missing piece is small and self-contained, write that piece yourself rather than narrowing the option; narrowing is for gaps too large to own.
- Keep sibling directives on one selection language: refuse a legacy selection option by name with the `:filter:` to write instead, rather than translating it for one sibling alone.
