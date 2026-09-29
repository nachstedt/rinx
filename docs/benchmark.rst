.. _benchmark:

Benchmarking Rinx
=================

This document explains how to benchmark the ``rinx`` documentation generator against real-world Sphinx projects.

There are two benchmarks, measuring different things:

.. list-table::
   :header-rows: 1

   * - Command
     - Corpus
     - Measures
   * - ``bazel run //scripts:benchmark``
     - CPython documentation
     - Throughput, and general RST/Sphinx coverage
   * - ``bazel run //scripts:benchmark_entities``
     - useblocks' sphinx-needs demo
     - Entity-model coverage against a real sphinx-needs project

The corpus-agnostic half of both lives in ``scripts/benchmark_common.py``: cloning a pinned tag, generating the warm-up site, timing a ``bazel build``, and diffing the resulting warnings against a hand-authored whitelist. Everything below the "Benchmarking the entity model" heading is specific to the second one.

In CI
=====

Both benchmarks run in every CI run — on each pull request, on ``main`` and for a release — as the reusable ``.github/workflows/benchmarks.yml``, and their rendered sites are published beside the documentation for the versions worth comparing against:

- ``pr/<N>/benchmarks/`` for a pull request, linked from its preview comment,
- https://nachstedt.github.io/rinx/main/benchmarks/index.html for ``main``,
- ``vX.Y.Z/benchmarks/`` for the newest release only; publishing a release removes the previous one's, while its documentation stays.

Each has a landing page linking both sites and their full reports.

**Only a corpus that no longer builds fails CI.** New warnings, whitelist entries that went stale and the timings are reported in the job's summary on the Actions page, never gated: the corpora are someone else's documentation, and a warning in them is a finding to triage rather than a regression. Timings on shared runners are too noisy to compare between runs.

Two flags make that possible, and work locally too:

``--site-out DIR``
   Once the corpus has built, copy its rendered site into ``DIR`` (replacing what is there), with the full report as ``report.txt`` and the site's front page named in ``entry.txt`` — which is all ``scripts/publish_pages.py`` needs to publish it.

``--summary-markdown FILE``
   Append the summary as a Markdown table to ``FILE`` — in CI, ``$GITHUB_STEP_SUMMARY``.

The published sites have to fit GitHub Pages' 1 GB limit together with every other version — the reason only the newest release keeps its benchmarks, and what made the sidebar collapse by default (``docs/decisions/031-collapsed-navigation.md``): before it, CPython's site alone was 612 MB. The summary's *Site size* line is there to keep an eye on that budget.

The CPython benchmark
=====================

We use the **CPython Documentation** as our primary benchmark target because of its size, complexity, and widespread use of various Sphinx extensions and syntax.

Running the Benchmark
---------------------

The benchmark is managed by a Python script integrated into the Bazel workspace.

You can execute it by running:

.. code-block:: bash

   bazel run //scripts:benchmark

What happens under the hood?
~~~~~~~~~~~~~~~~~~~~~~~~~~~~

1. **Cloning**: The script automatically performs a shallow clone of the CPython repository into a temporary directory (``rinx_benchmark_cpython`` under the system temp dir), at the **release tag** named by ``PYTHON_VERSION`` in ``scripts/benchmark.py`` — not at ``main``.

Why the version is pinned
~~~~~~~~~~~~~~~~~~~~~~~~~

``PYTHON_VERSION`` is the single place the benchmark's Python version is decided, and it pins *both* halves of the corpus so they cannot drift apart:

- the CPython release tag whose ``Doc/`` tree is cloned (``v3.14.2``), and
- the interpreter the generated workspace resolves, via a ``python.toolchain(python_version = ...)`` written into the injected ``MODULE.bazel``.

This matters for two reasons. First, **doctests**: the script used to clone ``main`` while the interpreter came from rinx's own ``MODULE.bazel``, so the documentation described a development version whose APIs the interpreter did not have. Every doctest exercising a newly added API failed for a reason that had nothing to do with rinx (``re.Pattern.prefixmatch``, ``IPv4Network.next_network``, ``PrettyPrinter(expand=...)``, ``shlex.quote(force=...)`` were all seen). Second, **reproducibility**: a benchmark against a moving branch produces numbers that change on their own, so a delta in ``benchmark_result.txt`` could never be attributed to a local change with confidence. A tag makes the corpus fixed.

Overriding it for a one-off comparison:

.. code-block:: bash

   bazel run //scripts:benchmark -- --python-version 3.13.11

A version must exist on **both** sides to be usable: as a ``v<version>`` tag in the CPython repository, and as an entry in rules_python's ``TOOL_VERSIONS`` (``python/versions.bzl``) for the rules_python release this workspace depends on. CPython ships later 3.14.x tags than rules_python 2.0.0 has interpreters for, which is why the default is 3.14.2 rather than the newest patch release.

Note that ``scripts/domain_warnings_whitelist.json`` is tied to the pinned corpus: entries for documents that do not exist at that tag are pruned automatically. Changing ``PYTHON_VERSION`` will therefore churn the whitelist.
2. **Bazel Project Generation**: A ``BUILD.bazel`` file is generated on the fly inside the ``Doc/`` directory of the clone, utilizing a ``glob(["**/*.rst"])`` statement to automatically capture all reStructuredText files into a single ``rinx_library`` target. It also generates a ``rinx_site`` target to assemble the HTML.
3. **Warm-up build**: A second, one-document site (``bench_warmup/``, generated beside ``Doc/``) is built first. Its only purpose is to compile the ``rinx`` binary and resolve the Rust, Java and Python toolchains *before* the clock starts on the documentation build — see "Rendering Time" below for why this is a separate build rather than a ``bazel build @rinx//:rinx``.
4. **Execution**: The script then runs ``bazel build //Doc:site``. This triggers ``rinx`` to parse, validate, and render every ``.rst`` file into HTML in parallel. The build currently succeeds outright against CPython's docs — toctree validation passes and HTML is produced for every page.
5. **Analysis**: Once the build completes, the script traverses the generated Abstract Syntax Tree (``.ast``) JSON files located in ``bazel-bin/``. It tallies up ``Directive::Unknown`` nodes (directives the parser doesn't recognize), ``Toctree.ignored_options`` (recognized toctree options the parser doesn't yet act on, e.g. ``:caption:``), and per-document parser diagnostics, and prints each as a frequency map.

Interpreting Results
--------------------

The full analysis is far too long for a terminal, so the script writes it to ``benchmark_result.txt`` in the workspace root and prints only a compact summary (the two timings, then distinct/occurrence counts per category) to the screen, headed by the corpus and its version, after a pointer to that file. Everything described below — the frequency tables and the domain-object listings — lives in ``benchmark_result.txt``; the terminal shows just the counts. (``benchmark_result.txt`` is git-ignored.)

1. Rendering Time
~~~~~~~~~~~~~~~~~
You will see two timings, one per ``bazel build``:
::

   Dependency build finished in Y.YY seconds.
   Documentation build succeeded in X.XX seconds.
   (Excludes Y.YY seconds spent building rinx itself.)

``X.XX`` is the number the benchmark is about: the raw time taken by Bazel to execute the ``rinx`` pipeline across the entire CPython documentation suite. Since Bazel runs these in parallel, this highlights the concurrency benefits of our architecture. ``Y.YY`` is the cost of compiling ``rinx`` and fetching its toolchains, which says nothing about documentation throughput and varies wildly with how warm the Bazel cache happened to be (it dominates everything after a ``--clean``).

Keeping the two apart is why the warm-up site exists. Building ``@rinx//:rinx`` directly would not do: build tools are compiled in Bazel's *exec* configuration, so that would warm a differently-configured binary and leave the real one to be compiled inside the timed step. Building a trivial site warms exactly the configurations the corpus build reuses, at the cost of one tiny document.

Each build's output is captured (so the script can time and parse it), which means neither log is streamed to your terminal. They are written to ``bazel_deps_build.log`` and ``bazel_build.log`` in the generated workspace (``$TMPDIR/rinx_benchmark_cpython/``), on both success and failure — inspect them there to see exactly what the inner builds printed. The Bazel timing profile is written alongside it as ``profile.json.gz`` (drop it into https://ui.perfetto.dev/ or ``chrome://tracing``).

2. Unsupported Directives Summary
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
A sorted list of Sphinx directives that ``rinx`` encountered but doesn't yet recognize (surfaced as ``Directive::Unknown`` nodes in the AST).

::

   Unsupported Directives Summary:
   -------------------------------
   doctest: 472
   availability: 394
   option: 356
   audit-event: 191
   ...

This list serves as a prioritized roadmap for feature implementation. Implementing the most frequent missing directives will rapidly increase our compatibility with real-world Sphinx codebases.

Every one of these also reports ``directive.unknown`` and renders as a visible error block quoting its source, so the tally below and the corpus' own pages agree about what is missing.

3. Malformed Directives Summary
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
The same shape, for directives ``rinx`` *does* implement whose content it had to refuse (``Directive::Malformed`` nodes) — a ``.. figure::`` with no image path, a ``.. csv-table::`` whose data would not parse. These used to inflate the unsupported tally above, which made a document's mistake look like a gap in coverage. Each one has already reported its own diagnostic under the summary below, so this is a count rather than a roadmap.

4. Ignored Toctree Options / Parser Diagnostics Summaries
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
Two smaller summaries follow: options seen on ``.. toctree::`` directives that are recognized but not yet acted upon (e.g. ``:caption:``, ``:numbered:``, ``:hidden:``), and aggregated parser diagnostics (e.g. malformed grid tables) emitted per document. Both are minor compared to the unsupported-directives list, but flag smaller gaps worth closing.

5. Domain-Object Warnings vs. the Whitelist
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
The render step reports **domain-object** cross-references (``:func:``, ``:py:class:``, ``:c:type:``, …) that either fail to resolve or resolve only via an object-type alias fallback. Against a large corpus like CPython these number in the hundreds, and most are *expected* — references to stdlib/C-API symbols this doc set doesn't index. To separate those from genuine regressions, the benchmark diffs them against a checked-in whitelist.

**How it's wired:** each ``render`` action writes a machine-readable sidecar (``--warnings-output``, one ``*.warnings.json`` per document) into ``bazel-bin/Doc/site_warnings/``. These are a non-default output group (``domain_warnings``) — they never land in the published site bundle, but they're produced on every build. ``scripts/benchmark.py`` globs them and compares against the whitelist.

**The whitelist** lives at ``scripts/domain_warnings_whitelist.json`` and is **hand-authored** — there is no record/regenerate mode. Its shape:

.. code-block:: json

   {
     "entries": [
       {
         "doc_path": "Doc/library/xmlrpc.client",
         "kind": "object_type_mismatch",
         "target": "Fault",
         "comment": "Referenced as :exc: but defined as a class upstream; expected."
       }
     ]
   }

The two ``kind`` values distinguish the two failure modes:
- ``domain_object_reference`` — the reference **didn't resolve at all**. Its ``requested_type`` records the object type the role asked for (the "missed type"), e.g. ``py:class``.
- ``object_type_mismatch`` — the reference **did** resolve, but to a different object type than the role asked for (via the alias fallback, e.g. ``class``/``exception`` or ``c``'s ``function``/``macro``). It carries both ``requested_type`` and ``resolved_type``.

An entry is matched to a warning by the exact triple ``(doc_path, kind, target)`` — the ``requested_type``/``resolved_type`` fields are informational, not part of the identity. ``comment`` is a free-text note explaining why the warning is accepted; it's yours to write and is preserved across runs.

The benchmark prints the new (non-whitelisted) warnings as two separate, most-frequent-first sections so the two failure modes don't drown each other out — the leading number on each line is that warning's occurrence count across the corpus:
- **Unresolved Domain-Object References** — ``domain_object_reference`` warnings, shown as ``doc: 'target' (referenced as <missed type>)``.
- **Domain-Object Type Mismatches** — ``object_type_mismatch`` warnings, shown as ``doc: 'target' (<requested> -> <resolved>)``.

It then prints a count of occurrences suppressed by the whitelist, and a **Stale Whitelist Entries** list (entries that no longer match any emitted warning).

**Auto-pruning:** stale entries are removed from the whitelist file automatically, but only when the warning data is trustworthy — the Bazel build succeeded **and** at least one sidecar was found. Otherwise pruning is skipped (and says why), so a broken or empty build can never silently delete your accepted entries along with their comments. This is why the build no longer runs under ``--keep_going``: a failed render must fail the whole build rather than under-report a document's warnings and get its whitelist entries pruned as "stale".

The benchmark always exits 0 — it reports and prunes, but never gates the build on new warnings.

Benchmarking the entity model
=============================

``bazel run //scripts:benchmark_entities`` builds a real sphinx-needs project — **useblocks' own `sphinx-needs demo <https://github.com/useblocks/sphinx-needs-demo>`__**, pinned at tag ``v0.1.5`` — against rinx's entity model, and reports everything the pipeline did not understand.

It is deliberately *not* a throughput measurement. 39 documents will not stress the pipeline the way CPython's 500-odd do. What it produces is a triage list: every line is either a bug in the conversion or a genuine gap in the entity model.

Why this corpus
---------------

The entity feature exists mostly to replace sphinx-needs, and until this benchmark existed it was exercised only by ``examples/entities/`` — a schema we wrote ourselves against documents we wrote ourselves, which cannot tell us what a real sphinx-needs project writes.

Three public corpora were considered:

.. list-table::
   :header-rows: 1

   * - Corpus
     - Size
     - Where its vocabulary is declared
   * - **useblocks/sphinx-needs-demo**
     - 39 ``.rst``, 180 KB; 20 need types, 14 link types, ~30 custom fields
     - ``docs/ubproject.toml`` + ``docs/schemas.json``
   * - eclipse-score/score
     - 292 ``.rst``, 1.6 MB
     - ``metamodel.yaml`` in the separate ``eclipse-score/docs-as-code`` repository — 55 need types, 28 link types
   * - eclipse-score/process_description
     - 304 ``.rst``, 1.4 MB
     - the same metamodel

The Eclipse S-CORE pair is the larger and more demanding target — Apache-2.0, tagged, already Bazel-native, with real ``:derived_from:``/``:satisfied_by:``/``:safety: ASIL_B`` traceability — and is the obvious scale-up once the demo builds clean. It is not the starting point because its vocabulary lives in a third repository in a YAML dialect of its own, whereas the demo's is a declarative TOML written by the people who define sphinx-needs' semantics, which converts mechanically.

The schema conversion (``scripts/needs_schema.py``)
---------------------------------------------------

A sphinx-needs project using ubCode keeps its configuration in ``ubproject.toml`` (announced to Sphinx by ``needs_from_toml``). That file is data, so the vocabulary it declares converts into an ``entities.toml``:

.. list-table::
   :header-rows: 1

   * - sphinx-needs
     - rinx
   * - ``[[needs.types]]`` ``directive``/``title``/``prefix``
     - ``[[entity_type]]`` ``name``/``label``/``id = { prefix }``, with ``argument = { fields = ["title"] }``
   * - ``[needs.fields.<n>]``, plus sphinx-needs' own built-in options
     - ``[[entity_type.attribute]]`` on **every** type — sphinx-needs scopes fields to no type
   * - ``[needs.fields.<n>.schema]`` ``type``/``enum``
     - the attribute's ``type`` (``string``, ``int``, ``bool``, ``enum``, ``list<string>``, ``list<enum>``) and ``values``
   * - ``[needs.links.<n>]``
     - ``[[entity_type.relation]]``, with sphinx-needs' derived ``<n>_back`` as the ``incoming`` option and its ``outgoing``/``incoming`` strings as the two labels. ``to`` is left off: a sphinx-needs link accepts any type
   * - each ``directive``
     - a ``[[role]]`` of that name, plus one ``need`` role over every type

The generated file lands in the clone (``<workspace>/entities.toml``), never in this repository — it is derived data, rewritten on every run.

``schemas.json`` is read for one thing only: an option that a rule makes *unconditionally* required on one need type is narrowed onto that type. Everything else — id patterns, ``allOf`` conditional selects, ``network`` constraints across linked entities — is reported instead. Honouring the ``required`` half of a conditional rule would be worse than not honouring it at all.

Everything the conversion cannot carry over is collected into a report rather than dropped, and printed under **"Sphinx-Needs Constructs Without An Entity-Model Equivalent"**. That section is as much the point of the benchmark as the diagnostics are: each entry is a question about the entity meta-model.

What the generated workspace looks like
---------------------------------------

Two things differ from the CPython benchmark's generated project, both forced by the corpus rather than chosen:

- **The corpus is the root package.** rinx resolves a source-root-relative path (``/_images/logo.png``, and the ``--doc-path`` every phase keys off) against the Bazel workspace root, so a corpus whose Sphinx ``srcdir`` is a subdirectory would have every absolute image path miss by that prefix. The clone's ``docs/`` therefore *becomes* the workspace root, and the corpus lives in ``//:demo_docs``. The generated ``assets`` and warm-up packages need no glob exclusions — a Bazel glob never crosses a package boundary.
- **Every glob excludes** ``bazel-*/**``. A consequence of the point above: Bazel's convenience symlinks (``bazel-bin``, ``bazel-out``, …) are created in the workspace root, which here is also the globbed package, so without the exclusion ``/*.jpg`` matches the *previous run's* ``site_site_out/_images/`` and makes the site's own output an input to the action that writes it — the image bundling then fails on a path Bazel cannot materialize. It hides while that action is an action-cache hit and surfaces as soon as anything re-keys it, such as a change to the rinx binary, which is the one thing this benchmark exists to measure. ``discard_stale_corpus_outputs`` is not a substitute: Bazel leaves its output directories read-only, so clearing them is best-effort. The CPython benchmark needs none of this — its corpus is the ``Doc/`` subdirectory, so the symlinks fall outside the globbed package.
- **Sources spliced in from outside the source root are copied in.** The parser resolves ``..`` by popping, so a ``.. literalinclude:: ../pharaoh.toml`` written in a root-level document is looked for at ``pharaoh.toml``. The script copies the real file to that path in the generated workspace and declares it in ``parse_data``. The alternative would be editing the corpus' own directives, which would make the benchmark measure a document set nobody wrote.

- **The corpus's sources are Jinja templates.** Its ``conf.py`` connects the
  ``source-read`` event and runs every document through Jinja2; 24 of the 31 open
  with a ``{% set %}`` naming the page and an ``{% include %}`` pulling in a shared
  header. The generated library therefore sets ``jinja = True``, and
  ``transclusion_targets`` collects Jinja include targets alongside the
  ``.. include::`` ones so each template is declared in ``parse_data`` and kept out
  of ``srcs``. Without it the two lines parse as the prose they literally are and
  the header reaches no page — see ``docs/decisions/013-source-templating.md``.

``entity_schema`` is declared on the library *and* on the site: it is a parse-time input (it is what makes ``.. req::`` a directive) and an index/render-time one, and a mismatch between the two is ``entity.schema-mismatch``.

Reading the report
------------------

The full listing goes to ``benchmark_entities_result.txt`` in the workspace root (git-ignored); the terminal shows counts only.

1. **Entities Parsed By Type** — the positive signal. A declared type with zero instances means either the corpus never used it, or its directives sit somewhere the parser never reached (nested inside a directive we do not recognize, for instance).
2. **Unsupported Directives Summary** — what remains of sphinx-needs and its neighbours that the entity model does not attempt — ``needservice`` is refused by name and so is counted in its own ``needservice.unsupported`` section below instead, and ``needtable``, ``needflow``, ``needsequence``, ``needpie``, ``needbar``, ``needuml``, ``needarch``, ``needimport`` and ``needextend`` are supported and no longer appear there. Counting them is how we find out what a migrating project would lose.
3. **Sphinx-Needs Constructs Without An Entity-Model Equivalent** — the converter's report, grouped by category.
4. **One section per diagnostic code** (``entity.unknown-target``, ``entity.role-type-mismatch``, ``entity.invalid-attribute-value``, …), most frequent first, diffed against ``scripts/entity_warnings_whitelist.json``.

The sections are built from the codes actually seen rather than hard-coded, so a diagnostic code added on the Rust side appears without this script being touched.

Where the warnings come from
~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Unlike the CPython benchmark, which reads the machine-readable ``*.warnings.json`` sidecars, this one parses the diagnostics out of the captured ``bazel_build.log``. The sidecar carries domain-object warnings only, and the render-time entity diagnostics — a ``:req:`` pointing at an id no document declares — exist nowhere else.

Two consequences worth knowing:

- The log is only complete because **every corpus action re-runs on every run**: the workspace is re-cloned and ``discard_stale_corpus_outputs`` drops the previous outputs, so no warning is lost to a cached action. It keeps ``external/`` (where the compiled rinx binary lives) and the generated packages, so the toolchain stays warm.
- A warning's identity is ``(document, code, message)`` — **the line and column are dropped**. A diagnostic found while *indexing* (a derived back-link pointing at an unknown id) belongs to a document but to no line of it, and a position in the key would also make the whitelist churn whenever text above a warning moves.

The whitelist works exactly like ``scripts/domain_warnings_whitelist.json``: hand-authored, matched on that triple, with a free-text ``comment`` explaining why an entry is accepted, and stale entries auto-pruned only when the build succeeded and there was output to read. The benchmark always exits 0.
