# Development Guidelines

What this repository has learned about how it wants to be built. Add a new
entry under the heading it belongs to, as a single short sentence. Record a
belief only when it will guide changes beyond the one that prompted it;
details of a single construct belong in its ADR.

## About these guidelines

- Guidelines are beliefs, preferences and style choices, not factual statements about tools or technologies.
- A guideline that bars a technique bars it as a workaround, not as the feature the user explicitly asked for.

## Naming

- Name functions and types after what they concretely do, not their abstract role or what they are used for (`build_project_index()`, not `analyze_many()`).
- Drop a name component the enclosing crate or directory already states (`scope/python.rs`, not `scope/python_scope.rs`).
- When renaming, rename everywhere at once (package, targets, docs, attributes) rather than keeping the old spelling as an alias; before 1.0 an honest name is worth the churn.

## Types and invariants

- Enforce invariants at the type level using opaque types with smart constructors ("parse, don't validate"), and re-validate them strictly on deserialization.
- Validate a user-declared vocabulary once at the boundary, so every later phase reads data already checked.
- Use enums for fixed sets of values, and prefer types that make a meaningless value unrepresentable (`NonZeroUsize` for a depth).
- When a value is transformed once and must never be re-transformed, give the result its own type that cannot express the untransformed state.
- Give each kind in a family its own variant carrying only its own fields, rather than one struct holding the union of every kind's options; put a field all kinds share once beside the family.
- Give two constructs one shared type when they are structurally identical, with a small enum recording which was written; split them only when they need different fields.
- When a construct may later gain a sibling differing in one field, model that field as an enum from the first commit, so the sibling is a new arm rather than a change to every match and serialized file.
- Keep two views of one fact consistent through the compiler — an exhaustive enum generated with its lookup and data from one table — rather than through a test that only catches what is exercised.
- Keep a declarative description free of behaviour, beside the code that acts on it, paired in a data-only registry, so several consumers (readers, completion, documentation) share it.
- Keep the open-ended case of an enum as a validated newtype rather than enumerating a third party's whole vocabulary.
- Never leave a field of a serialized type out of its wire form: put what only one process needs in a separate type with no serde impl, reached through an opaque handle only that process can mint.
- Give an operation one public entry point taking an options struct that implements `Default`, rather than a chain of wrappers each filling in one more setting.
- Group `match` arms by a real difference in handling, never to satisfy a lint's length limit.

## Module and file organization

- Split a file past roughly 800 lines along construct or responsibility boundaries, into a same-named subdirectory rather than filename-prefixed siblings.
- A module file that owns a same-named directory is a pure forwarder: doc comment, `mod` declarations and re-exports only.
- When an implementation is one cohesive unit, keep it whole and split only its tests into topic-named sibling files.
- Place a module under whichever dispatcher actually calls it, found by grepping its callers, and split general-purpose helpers out of a construct's file under a name saying what they do.
- Keep logic in the phase (and crate) that performs it, and keep a type with the only code that constructs and reads it.
- Give shared logic its own crate when the phases needing it may not depend on each other; when an existing crate all callers already depend on fits, put it there instead.
- Ask another crate for a fact it owns through an injected trait rather than keeping a second copy, since the copy drifts exactly when the check matters most.
- When two constructs share a sub-syntax, extract one shared helper rather than re-implementing it per construct; treat two functions with identical bodies as one and delete the duplicate.
- Co-locate unit tests in inline `#[cfg(test)]` modules, and give fixtures two test modules share their own `#[cfg(test)]` support module.
- Remove a code path once its last caller is gone, together with the helpers and tests only it used.

## Refactoring safely

- Never pick a weaker design to save mechanical edits the compiler finds for you.
- Move code verbatim and let `cargo fmt` re-indent; blanket whitespace edits silently rewrite string literals.
- Prove a pure reorganization behaviour-preserving, by comparing test counts and diffing moved string literals against the previous revision.

## Parsing and AST design

- Fix an ambiguity between constructs at the root, by tightening the detector to match the spec, rather than relying on dispatch order.
- Validate alignment-sensitive input strictly and reject a mismatch rather than silently padding over it.
- When a prefix is markup rather than part of a name, strip it while parsing and record its meaning as typed intent on the node.
- Two directives asking one question with different presentations stay separate nodes; put the generality in the layer that answers the question, not behind a mode switch.
- Keep parse-time configuration in one context object threaded through the parsers, and give a pure phase an injected trait for anything it cannot do itself (like reading a file).
- Store an embedded notation exactly as written and convert it only while rendering, so the backend never reaches a serialized artefact.
- Declare a relationship on the side that writes it and derive the reverse direction.
- Store only what a later reader needs in a shared index, never write data derivable from the AST into an artefact beside it, and compute state several phases need once, in memory.
- Give a generated identifier a deterministic source (a position, not content) so cached artefacts stay byte-identical.
- When implementing a subset of a language, recognise the constructs outside it and refuse each by name, pointing at what does work.
- Generate a derived construct's content as data rather than lowering it to a template of another construct, or every diagnostic lands on text the author never wrote.

## Dependencies

- Before hand-writing a parser, interpreter or vendored data set, look for a maintained library or redistribution, and record in the ADR why each candidate was rejected.
- Let the build system's constraints decide a library, not its feature list: a foreign toolchain costs more in a sandboxed multi-platform build than any feature adds.
- Keep every linked dependency under MIT or Apache-2.0.
- Pin a library, tool and runtime to a host-independent configuration inside the build, since anything resolved from the host makes one action key yield different outputs.
- When a backend cannot express a commonly used option but the missing piece is small and self-contained, write that piece rather than narrowing the option.
- Generate a derived artefact from the same source the code uses and check it in with a test that regenerates and compares.

## Porting from a reference implementation

- Read the reference implementation's actual source, and transcribe its tables rather than re-deriving them from memory.
- Reproduce its observable outcome rather than its literal operation, since your own normalizing types can turn its miss into a hit.
- Keep its default for anything that changes rendered output, so adopting the tool never alters a site that did not ask for it.
- Deviate deliberately where it is silently permissive or discards what the source expressed, and comment why so nobody "fixes" it back.
- Where a library cannot reproduce its behaviour exactly, pick the narrower behaviour, diagnose what you refuse, and comment why.
- When reimplementing a third party's directive, emit its element structure and class names verbatim, so existing stylesheets keep working.
- Do not reproduce a structural wrapper the reference emits as plumbing when it would defeat the construct's purpose; splice transparently and say why in a comment.

## Diagnostics

- Keep the resilient parser and the strict build separate: the parser degrades bad input and records what went wrong, and the build decides whether it is fatal.
- Build a feature's diagnostics with the feature, port every check the reference makes on a construct, and add your own wherever it silently degrades valid-looking input.
- Report a malformed value in the earliest phase that reads the author's text.
- Every diagnostic carries a span and a stable code from an enum, never formatted into the message; a code names the construct, not the spelling or code path that read it.
- Report a mistake in the grammar all directives share under one code everywhere.
- Name a file a reader can open, count columns in characters, convert to a protocol's encoding at its boundary, and report no position rather than a wrong one.
- Do not diagnose what the author could not have caused or acted on.
- When a lookup is ambiguous, pick no winner: leave it unresolved and name every candidate, and never let a result depend on input order.
- Measure a new or noisy diagnostic on the benchmark corpus by category before keeping, narrowing or quieting it.
- When a pattern matches far more than the construct it looks for, let an unrecognised match degrade to plain text.
- Render an unrecognized construct visibly, quoting its source, rather than omitting it, and keep a misplaced child or content behind an unreadable option.
- Distinguish "this build does not know that name" from "it knows the name and had to refuse the content".
- Build the node a reader sees and the diagnostic a log records from one message.
- Where the reference discards a whole construct over a shape error, report it and keep interpreting what was written.
- Author-facing suppression lives in the document, spelled as a construct the reference ignores; it applies at the reporting boundary, lifts every consequence (failure and sidecar too), is scoped to its file, and an unknown code in it is diagnosed.
- Report a fault in the build's configuration outside the suppression mechanism, and check a configuration invariant in both directions.
- When a static reader meets code it cannot run, apply what it could read and report exactly what it skipped.
- Lower an unknown construct's severity only when a known source explains it, and never ship one project's vocabulary to quiet it.
- Route every construction of a position-bearing value through one function once any needs extra context.

## Testing

- Unit test every function, including private helpers and those introduced while refactoring, in Given-When-Then style.
- Fix a bug test-first: write the regression test and see it fail before changing the code.
- Validate a feature against real-world corpora before calling it done; hand-built tests miss what authentic documents contain.
- Cover conversions and never-panic invariants with property tests, and check in the seeds they find.
- When deleting tests whose subject moved to another phase, check the destination covers the cases and test the seam.
- Do not widen a public API to serve a test.
- Assert on the specific markers a feature emits, not on whitespace or substrings a shared wrapper also contains.
- Give every user-visible editor feature an end-to-end test through real VS Code and the real binary.
- Run every shell test CI runs before calling a change done, and add new ones to the workflow in the same change.
- A test proving a build declaration is load-bearing must force the affected action to re-run, and detects a rerun with `-s`, never with `(cached)`.
- Resolve a tool from `PATH` in a test script rather than hardcoding its location.
- Settle rendering and browser-behaviour questions in a headless browser, by screenshot, not by reading HTML or from memory.

## Configuration and CLI

- Prefer one configuration file over accumulating CLI flags.
- Keep a resource's name in configuration and its path on the command line: sandboxes relocate files, so a config file holds no paths — though a config file a phase reads is fine, as long as it declares no build input.
- Give each phase its own configuration file, and put a setting in the latest phase that needs it, so editing it never invalidates earlier phases' cached work.
- Decide which files a tool reads from explicit configuration, never from another tool's convention such as `.gitignore`.

## Build, examples and documentation

- Add examples for every variant of a new feature to the example project, declaring new `.rst` files in their `BUILD.bazel` `srcs`.
- Derive a target's inputs from the targets it depends on rather than repeating them; give data a target reads its own attribute, named after when the build reads it.
- Pay only for what you use: when Bazel cannot know before running actions whether a feature is used, make it a per-library opt-in, and a feature that declares no action of its own needs none.
- An opt-in is only honest if forgetting it fails loudly at the line that needed it, naming the switch.
- Make a feature opt-in when its trigger is a character sequence ordinary prose contains.
- When two actions compute the same thing from the same inputs, fold them into one; when two phases must agree on a derived value, have both call one pure function rather than pass a sidecar.
- A transform whose output only feeds the next step of the same process needs no intermediate artefact; give it a debugging flag instead.
- Measure a plausible-sounding build optimization before keeping it, and record what an accepted regression costs, measured, in its ADR.
- When a build's output outgrows a platform limit, shrink it at its source.
- Gate CI on hard failures and only report soft signals such as new warnings and timings.
- Test every integration the documentation promises in CI, and keep a configuration snippet the docs show in a file a test runs and the page includes.
- Keep docs/compatibility.rst neutral and brief, naming only what is missing or deviates; a row is 🔶 partial while something we intend to implement is missing, and a gap left out to keep a change small is not a deliberate deviation.
- Every user-visible change adds a bullet to CHANGELOG.md's `[Unreleased]` in the same pull request; refactors, CI, tests and dependency bumps do not.
- Keep manual checklists (e.g. the PR template) to what a human must judge.
- Match the specificity an existing or upstream CSS rule uses, or the new rule silently loses or blocks the override the framework expects.
- Keep CLAUDE.md files to rules, gotchas and invariants likely to guide future changes; leave per-feature specifics to the ADRs and the code.

## Interoperability

- Give a borrowed construct this project's own name and accept the other tool's spelling as an alias.
- Default a borrowed construct's options to what this project's schema declares; when no default gives a meaningful result, make the option mandatory and name the valid choices.
- Ignore a foreign format's bookkeeping fields by an explicit list and report every other unrecognised one.
- Put an imported construct through the same validation funnel the written one uses.
- Let an unmigrated Sphinx project try the tool without adopting Bazel first.
- Make a preview show what the reader will see, and render a third party's format with its own code built for the new platform rather than a reimplementation.

## Editor features

- Implement a language-server feature in the server itself, so it works in every editor and in `rinx lsp --check`.
- An editor feature treats a reference exactly as the built page does.
- Follow a setting the Sphinx project already configures rather than adding an editor setting for it.
- When VS Code ships a counterpart feature for another language, mirror its behaviour and settings.
- A rename rewrites the name everywhere it is written rather than adding an alias.

## Workflow and commits

- After every code change, run `cargo clippy --workspace --tests` (fixing warnings rather than suppressing them), `cargo fmt`, and `bazel build //examples:site`.
- Do not commit unless explicitly asked.
- Give every commit a conventional-commit subject and a body summarizing what it does and the notable decisions behind it.
- File a GitHub issue for every problem discovered but not resolved in the current change.
- Ship a construct together with any sibling syntax whose absence would make existing input render worse, and leave a sibling case out only for a concrete technical reason.
- When a feature touches behaviour that should hold for every construct of its kind, leave it out and introduce it as one general concept in a follow-up.
- Defer a known gap only to a roadmap step whose scope covers it; otherwise add a step of its own.
- When a second construct needs the same plumbing, widen the existing type rather than adding a parallel one.
- A value two callers each supply separately belongs on the data being processed, not in a context each fills in.

## Python

- Hold Python code to the same bar as Rust: fully annotated, type-checked, linted and formatted in CI, with lint findings treated as refactoring signals.
- Use the Astral toolchain (uv, ruff, ty) configured in one `pyproject.toml`, and run tests under Bazel with that same file.
- Write Python tests in plain pytest style.

## JavaScript

- Run JavaScript tests hermetically under Bazel (rules_js, as a dev dependency) rather than with a locally installed Node.js.
