"""Benchmark the entity model against a real sphinx-needs project.

`benchmark.py` measures general RST coverage against CPython. This one measures
*entity* coverage against the useblocks sphinx-needs demo: it converts that
project's own sphinx-needs configuration into a rusty-sphinx entity schema
(`needs_schema.py`), builds its documents against that schema, and reports
everything the pipeline did not understand.

Unlike the CPython benchmark this is not a throughput measurement — 39
documents will not stress the pipeline. What it produces is a triage list: each
diagnostic is either a converter bug or a genuine gap in the entity model, and
each entry in the converter's own report is a sphinx-needs feature our
meta-model has no vocabulary for. Eclipse S-CORE (`eclipse-score/score` plus
`eclipse-score/process_description`, ~600 documents sharing one 55-type
metamodel) is the obvious scale-up once this corpus builds clean; its schema
lives in a third repository in a YAML dialect of its own, which is why it is
not the starting point.

Run it with `bazel run //scripts:benchmark_entities`.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import tempfile
import tomllib
from pathlib import Path, PurePosixPath

import benchmark_common
import needs_schema
from benchmark_common import WARMUP_PACKAGE, write_frequency_summary

TARGET_DIR = Path(tempfile.gettempdir()) / "rusty_sphinx_benchmark_needs"
REPO_URL = "https://github.com/useblocks/sphinx-needs-demo.git"

# The one place the corpus version is decided.
#
# A tag rather than `main`, for the reason docs/benchmark.md gives for CPython:
# a benchmark against a moving branch produces numbers and warning lists that
# change on their own, so a delta could never be attributed to a local change.
# Unlike the CPython pin this one is one-sided — there is no interpreter to keep
# in step, only the corpus.
#
# v0.1.5 trails `main` by a few months but carries both files the conversion
# needs: `docs/ubproject.toml` (the declarative sphinx-needs config, announced
# to Sphinx by `needs_from_toml`) and `docs/schemas.json`.
DEMO_VERSION = "0.1.5"

# The clone's Sphinx source directory, which becomes the *Bazel workspace root*
# of the generated project rather than a package inside one.
#
# That is forced, not a preference: rusty-sphinx resolves a source-root-relative
# path (`/_images/logo.png`, and the `--doc-path` every phase keys off) against
# the workspace root, so a corpus whose `srcdir` is a subdirectory would have
# every absolute image path miss by that prefix. Making the two roots the same
# directory is the only arrangement in which the corpus' own paths resolve.
# The corpus therefore lives in the *root* package, `//:demo_docs`.
CORPUS_SRCDIR = "docs"

# Generated packages placed inside the workspace. Each holds a BUILD file, which
# makes it a package of its own — so the root package's `glob(["**/*.rst"])`
# stops at its boundary and cannot pick up the warm-up document.
GENERATED_PACKAGES = ("assets", WARMUP_PACKAGE)

# Hand-authored list of entity warnings we accept, relative to the workspace
# root (main() chdirs there via BUILD_WORKSPACE_DIRECTORY).
WHITELIST_PATH = Path("scripts/entity_warnings_whitelist.json")

# The generated schema, written into the clone rather than into this repository:
# it is derived data, regenerated on every run.
SCHEMA_NAME = "entities.toml"


def clone_repo(version: str):
    """Shallow-clones the sphinx-needs demo at the tag matching `version`."""
    tag = f"v{version}"
    if not benchmark_common.clone_repo(REPO_URL, tag, TARGET_DIR):
        raise SystemExit(
            f"Failed to clone the sphinx-needs demo at tag {tag!r}.\n"
            f"Check that the tag exists (`git ls-remote --tags {REPO_URL} '{tag}'`) — "
            "see DEMO_VERSION in this file."
        )


# ── Schema conversion ─────────────────────────────────────────────────────────

def convert_schema(docs_dir: Path):
    """Convert the corpus' own sphinx-needs configuration into an entity schema.

    Returns the converter's report of constructs with no equivalent in our
    meta-model — half the point of the whole benchmark."""
    print("Converting the project's sphinx-needs configuration into an entity schema...")
    with open(docs_dir / "ubproject.toml", "rb") as f:
        ubproject = tomllib.load(f)

    schemas_path = docs_dir / "schemas.json"
    schemas = json.loads(schemas_path.read_text()) if schemas_path.exists() else {}

    text, report = needs_schema.convert(ubproject, schemas)
    (docs_dir / SCHEMA_NAME).write_text(text)
    print(f"Wrote {docs_dir / SCHEMA_NAME}.")
    return report


# ── Bazel project generation ──────────────────────────────────────────────────

# `.. include::` and `.. literalinclude::` name their source in the argument;
# `.. csv-table::` names it in a `:file:` option.
TRANSCLUSION_DIRECTIVE = re.compile(
    r"^[ \t]*\.\.[ \t]+(?:include|literalinclude)::[ \t]*(\S+)[ \t]*$", re.MULTILINE
)
CSV_FILE_OPTION = re.compile(r"^[ \t]*:file:[ \t]*(\S+)[ \t]*$", re.MULTILINE)


def transclusion_targets(source_dir: Path):
    """Find every file the corpus splices in at parse time.

    Returns `(included_rst, outside)`. `included_rst` holds documents pulled in
    by `.. include::`, as paths from the source root — they must be kept *out*
    of `srcs`, or each would be published as a page of its own as well as being
    spliced into its includer. `outside` holds `(source, destination)` pairs for
    sources that live outside the source root: `source` is where the file
    actually is, `destination` the source-root-relative path the parser will
    look for it at.

    That second list exists because the parser resolves `..` by *popping*
    (`ast::normalize_path`), so a `.. literalinclude:: ../pharaoh.toml` written
    in a root-level document is looked for at `pharaoh.toml` — inside the
    source root. Copying the real file to that path is what makes the reference
    resolve, and is the only alternative to editing the corpus.

    A path starting with `/` is source-root-relative; anything else resolves
    against the directory holding the document. Both rules are the parser's."""
    included_rst = set()
    outside = {}

    for document in sorted(source_dir.rglob("*.rst")):
        relative_doc = document.relative_to(source_dir)
        text = document.read_text(errors="replace")
        targets = TRANSCLUSION_DIRECTIVE.findall(text) + CSV_FILE_OPTION.findall(text)
        for target in targets:
            # Pharaoh templates the argument (`{{page}}`); there is no file to
            # declare, and the parse reports it.
            if "{" in target or "}" in target:
                continue

            destination = resolve_from_document(target, relative_doc.as_posix())
            actual = actual_location(source_dir, relative_doc, target)
            if actual is not None and actual != source_dir / destination:
                outside[destination] = actual
            elif Path(destination).suffix == ".rst":
                included_rst.add(destination)

    return sorted(included_rst), sorted(outside.items())


def resolve_from_document(written: str, doc_path: str):
    """The parser's `ast::resolve_from_document`, in Python.

    A leading `/` means "from the source root"; otherwise the path resolves
    against the document's directory. `..` pops, so the result never escapes
    the source root — which is exactly why an outside source needs copying to
    where the parser will look."""
    if written.startswith("/"):
        parts = written.lstrip("/").split("/")
    else:
        parts = PurePosixPath(doc_path).parent.as_posix().split("/") + written.split("/")

    normalized = []
    for part in parts:
        if part in ("", "."):
            continue
        if part == "..":
            if normalized:
                normalized.pop()
        else:
            normalized.append(part)
    return "/".join(normalized)


def actual_location(source_dir: Path, relative_doc: Path, written: str):
    """Where the named file really is on disk, or None if it is not there.

    Unlike the parser's view this one honours `..` as the filesystem does, so a
    source outside the source root is found rather than clamped."""
    if written.startswith("/"):
        candidate = source_dir / written.lstrip("/")
    else:
        candidate = source_dir / relative_doc.parent / written
    candidate = Path(os.path.normpath(candidate))
    return candidate if candidate.is_file() else None


def corpus_workspace() -> Path:
    """The generated Bazel workspace: the clone's Sphinx source directory."""
    return TARGET_DIR / CORPUS_SRCDIR


def generate_bazel_project(rusty_sphinx_root: str):
    """Turn the clone's source directory into a Bazel workspace of its own."""
    print("Generating a Bazel project around the clone...")
    workspace = corpus_workspace()

    (workspace / "MODULE.bazel").write_text(f"""module(name = "sphinx_needs_demo_bench")

bazel_dep(name = "rusty_sphinx", version = "0.0.0")
local_path_override(
    module_name = "rusty_sphinx",
    path = "{rusty_sphinx_root}",
)
""")

    assets_dir = workspace / "assets"
    assets_dir.mkdir(exist_ok=True)
    (assets_dir / "BUILD.bazel").write_text("""alias(
    name = "default.css",
    actual = "@rusty_sphinx//:assets/default.css",
    visibility = ["//visibility:public"],
)
""")

    (workspace / "rusty_sphinx.toml").write_text('project = "Sphinx-Needs Demo Benchmark"\n')
    default_template = Path(rusty_sphinx_root) / "templates" / "default.html"
    (workspace / "custom_template.html").write_text(default_template.read_text())

    included_rst, outside = transclusion_targets(workspace)
    copy_outside_sources(workspace, outside)
    (workspace / "BUILD.bazel").write_text(
        render_corpus_build_file(included_rst, [destination for destination, _ in outside])
    )

    benchmark_common.generate_warmup_package(workspace, rusty_sphinx_root)


def copy_outside_sources(workspace: Path, outside):
    """Place each source that lives outside the source root where the parser
    will look for it.

    The alternative would be editing the corpus' own `.. literalinclude::`
    arguments, which would make the benchmark measure a document set nobody
    wrote. The generated workspace is disposable, so adding a file to it is the
    same kind of scaffolding as the generated BUILD file next to it."""
    for destination, source in outside:
        target = workspace / destination
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(Path(source).read_bytes())
        print(f"Copied {source} -> {target} (spliced in from outside the source root).")


def render_corpus_build_file(included_rst, spliced_from_outside):
    """The workspace root package's BUILD.bazel.

    The two lists are what makes the generated file corpus-specific: an
    `.. include::`d document is excluded from `srcs` (or it would be published
    as a page as well as spliced in) and declared as `parse_data` instead, and
    so is every source copied in from outside the source root. The generated
    `assets` and warm-up packages need no exclusion — a glob never crosses a
    package boundary."""
    excludes = "".join(f"            {json.dumps(path)},\n" for path in included_rst)
    spliced = "".join(
        f"        {json.dumps(path)},\n"
        for path in sorted(set(included_rst) | set(spliced_from_outside))
    )
    return f'''load("@rusty_sphinx//:defs.bzl", "rusty_sphinx_library", "rusty_sphinx_site")

rusty_sphinx_library(
    name = "demo_docs",
    srcs = glob(
        ["**/*.rst"],
        exclude = [
{excludes}        ],
    ),
    # The project's own sphinx-needs vocabulary, converted to an entity schema.
    # A parse-time input: it is what makes `.. req::` a directive rather than an
    # unknown name.
    entity_schema = "{SCHEMA_NAME}",
    images = glob(
        [
            "**/*.png",
            "**/*.jpg",
            "**/*.jpeg",
            "**/*.gif",
            "**/*.svg",
            "**/*.webp",
        ],
        allow_empty = True,
    ),
    # Everything the *parser* reads at parse time: the documents `.. include::`
    # splices in, plus the data and code `.. literalinclude::` and
    # `.. csv-table:: :file:` pull from.
    parse_data = glob(
        [
            "**/*.json",
            "**/*.toml",
            "**/*.py",
            "**/*.need",
            "**/*.csv",
            "**/*.txt",
            "**/*.yaml",
        ],
        allow_empty = True,
    ) + [
{spliced}    ],
)

rusty_sphinx_site(
    name = "site",
    config = "rusty_sphinx.toml",
    template = "custom_template.html",
    css = "//assets:default.css",
    # Declared again here: the index action derives back-links from it and the
    # render actions read its labels. A library parsed against a different
    # schema than the site indexes with is reported as `entity.schema-mismatch`.
    entity_schema = "{SCHEMA_NAME}",
    deps = [":demo_docs"],
)
'''


# ── Build ─────────────────────────────────────────────────────────────────────

def discard_stale_corpus_outputs(workspace: Path):
    """Remove generated outputs left over from a previous run.

    The workspace is re-cloned every run but Bazel's output base survives it, so
    outputs for documents that are no longer there would be picked up by the
    `.ast` glob and inflate every count. `benchmark_common`'s version takes a
    package name; the corpus here *is* the root package, so what is kept is
    named instead: `external` (the fetched repositories and the compiled
    rusty-sphinx binary, which stay warm) and the generated packages."""
    info = subprocess.run(
        ["bazel", "info", *benchmark_common.BUILD_CONFIG_FLAGS, "bazel-bin"],
        cwd=str(workspace),
        capture_output=True,
        text=True,
    )
    if info.returncode != 0:
        return

    bazel_bin = Path(info.stdout.strip())
    if not bazel_bin.exists():
        return

    print("Discarding generated outputs from a previous corpus...")
    keep = {"external", *GENERATED_PACKAGES}
    for entry in bazel_bin.iterdir():
        if entry.name in keep:
            continue
        if entry.is_dir() and not entry.is_symlink():
            shutil.rmtree(entry, ignore_errors=True)
        else:
            entry.unlink(missing_ok=True)


def run_benchmark(clean: bool = False):
    """Build the warm-up site, then the corpus. Returns whether the corpus built."""
    workspace = corpus_workspace()
    if clean:
        print("Cleaning Bazel cache...")
        subprocess.run(["bazel", "clean"], cwd=str(workspace), capture_output=True)
    else:
        discard_stale_corpus_outputs(workspace)

    print("Building rusty-sphinx and its toolchains (not timed as doc build)...")
    deps_built, deps_duration = benchmark_common.timed_bazel_build(
        workspace, f"//{WARMUP_PACKAGE}:site", "bazel_deps_build.log"
    )
    print(f"Dependency build finished in {deps_duration:.2f} seconds.")
    if not deps_built:
        return False

    print("\nRunning Bazel build of the sphinx-needs demo documentation...")
    build_succeeded, duration = benchmark_common.timed_bazel_build(
        workspace, "//:site", "bazel_build.log"
    )
    if build_succeeded:
        print(f"Documentation build succeeded in {duration:.2f} seconds.")

    print(f"\nBazel build output was captured to: {workspace / 'bazel_build.log'}")
    print(f"HTML output is located at: {workspace}/bazel-bin/site_site_out/")
    return build_succeeded


# ── Analysis ──────────────────────────────────────────────────────────────────

# The one warning shape `crates/worker/src/commands/diagnostics.rs` emits, for
# parse, render and preview alike.
WARNING_LINE = re.compile(
    r"^warning: (?P<path>[^ ]+?)(?::\d+:\d+)?: "
    r"(?P<code>[a-z][a-z0-9]*(?:[.\-][a-z0-9]+)+): (?P<message>.*)$"
)


def parse_build_warnings(log_text: str):
    """Read the diagnostics the build printed out of the captured Bazel log.

    The log is the only place render-time entity diagnostics
    (`entity.unknown-target`, `entity.role-type-mismatch`) can be read from: the
    `--warnings-output` sidecar carries domain-object warnings only. Parse-time
    diagnostics are in the `.ast` files too, but taking both from one source
    keeps them in one table.

    Note this only works because every corpus action re-runs on every benchmark
    run — the workspace is re-cloned and `discard_stale_corpus_outputs` drops
    the previous outputs, so no warning is lost to a cached action.

    The position is optional in the pattern as well as dropped from the entry.
    A diagnostic found while *indexing* — a derived back-link pointing at an id
    no document declares — belongs to a document but to no line of it, and
    matching only the positioned shape would silently lose exactly the
    project-wide findings this benchmark exists for. Dropping the position also
    keeps a warning's identity from churning whenever text above it moves,
    which matters because the whitelist matches on `(doc_path, kind, target)`."""
    entries = {}
    for raw_line in log_text.splitlines():
        match = WARNING_LINE.match(raw_line.strip())
        if match is None:
            continue
        entry = {
            "doc_path": match.group("path"),
            "kind": match.group("code"),
            "target": match.group("message"),
        }
        # Bazel prints an action's stderr once per execution, but a document
        # is parsed and rendered in separate actions, so an identical warning
        # can legitimately appear twice; count it once per (document, code,
        # message) triple.
        entries[benchmark_common.warning_key(entry)] = entry
    return list(entries.values())


def format_entity_warning(entry, include_kind=True):
    """One-line rendering of an entity warning or whitelist entry."""
    kind = f"{entry.get('kind')}: " if include_kind else ""
    return f"{entry.get('doc_path')}: {kind}{entry.get('target')}"


def tally_ast(ast_files):
    """Walk the built `.ast` files, counting unknown directives and the entity
    instances that did parse.

    The second half is the positive signal: a type with zero instances means the
    corpus never exercised it, and a type the schema failed to declare shows up
    in the first half as an unknown directive instead."""
    unknown_directives = {}
    entities = {}

    def traverse(node):
        if isinstance(node, dict):
            directive = node.get("Directive")
            if isinstance(directive, dict):
                if "Unknown" in directive:
                    name = directive["Unknown"].get("name", "unnamed")
                    unknown_directives[name] = unknown_directives.get(name, 0) + 1
                elif "Entity" in directive:
                    name = directive["Entity"].get("type_name", "unnamed")
                    entities[name] = entities.get(name, 0) + 1
            for value in node.values():
                traverse(value)
        elif isinstance(node, list):
            for item in node:
                traverse(item)

    for ast_file in ast_files:
        try:
            with open(ast_file, "r") as f:
                traverse(json.load(f))
        except Exception as e:
            print(f"Failed to parse {ast_file}: {e}")

    return unknown_directives, entities


def analyze_results(build_succeeded, schema_report):
    """Write the full report and print the compact terminal summary."""
    print("Analyzing the build for entity-model gaps...")

    # The warm-up site's one document is scaffolding, not corpus.
    ast_files = [
        path
        for path in corpus_workspace().glob("bazel-bin/**/*.ast")
        if WARMUP_PACKAGE not in path.parts
    ]
    if not ast_files:
        print("No .ast files found in bazel-bin. Did the build succeed?")

    unknown_directives, entities = tally_ast(ast_files)

    log_path = corpus_workspace() / "bazel_build.log"
    log_text = log_path.read_text() if log_path.exists() else ""
    warnings = parse_build_warnings(log_text)

    result_path = Path("benchmark_entities_result.txt")
    with open(result_path, "w") as out:
        print("=== rusty-sphinx entity benchmark: full report ===", file=out)
        write_frequency_summary(out, "Entities Parsed By Type", entities)
        write_frequency_summary(out, "Unsupported Directives Summary", unknown_directives)
        write_schema_report(out, schema_report)
        summary = benchmark_common.report_warnings(
            warnings,
            len(ast_files),
            WHITELIST_PATH,
            build_succeeded,
            out,
            sections=warning_sections(warnings),
            format_entry=format_entity_warning,
        )

    print_entity_summary(result_path, entities, unknown_directives, schema_report, summary)


def warning_sections(warnings):
    """One report section per diagnostic code seen, most frequent first.

    Built from the data rather than hard-coded: a code added on the Rust side
    should appear in the report without this file being touched."""
    counts = {}
    for entry in warnings:
        counts[entry["kind"]] = counts.get(entry["kind"], 0) + 1
    ordered = sorted(counts.items(), key=lambda item: (-item[1], item[0]))
    return [(code, code) for code, _count in ordered]


def write_schema_report(out, schema_report):
    """List what the sphinx-needs configuration expresses and our model cannot,
    grouped by category."""
    print("\nSphinx-Needs Constructs Without An Entity-Model Equivalent:", file=out)
    print("-" * 58, file=out)
    if not schema_report:
        print("None found.", file=out)
        return
    by_category = {}
    for category, detail in schema_report:
        by_category.setdefault(category, []).append(detail)
    for category in sorted(by_category, key=lambda c: (-len(by_category[c]), c)):
        print(f"\n{category} ({len(by_category[category])}):", file=out)
        for detail in by_category[category]:
            print(f"  - {detail}", file=out)


def print_entity_summary(result_path, entities, unknown, schema_report, summary):
    """The compact, terminal-friendly summary."""
    line = benchmark_common.summary_line

    print("\n=== Entity Benchmark Summary ===")
    line("Entity types exercised:", len(entities), sum(entities.values()))
    line("Unsupported directives:", len(unknown), sum(unknown.values()))
    line("Unconvertible constructs:", len({c for c, _ in schema_report}), len(schema_report))
    for code, (distinct, occurrences) in summary["per_kind"].items():
        if distinct:
            line(f"{code}:", distinct, occurrences)
    benchmark_common.print_whitelist_summary(summary)
    print(f"\nFull report written to: {result_path}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--clean", action="store_true", help="Clear the Bazel cache before building"
    )
    parser.add_argument(
        "--demo-version",
        default=DEMO_VERSION,
        help=(
            "sphinx-needs-demo release to benchmark against; selects the cloned "
            f"tag (default: {DEMO_VERSION})."
        ),
    )
    args = parser.parse_args()

    rusty_sphinx_root = os.environ.get("BUILD_WORKSPACE_DIRECTORY", os.getcwd())
    os.chdir(rusty_sphinx_root)

    if not Path("WORKSPACE").exists() and not Path("MODULE.bazel").exists():
        print("Please run this script from the root of the rusty-sphinx workspace.")
        return

    print(f"Benchmarking the entity model against sphinx-needs-demo {args.demo_version}.")
    clone_repo(args.demo_version)
    schema_report = convert_schema(corpus_workspace())
    generate_bazel_project(rusty_sphinx_root)
    build_succeeded = run_benchmark(clean=args.clean)
    analyze_results(build_succeeded, schema_report)


if __name__ == "__main__":
    main()
