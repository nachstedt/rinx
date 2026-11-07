"""Benchmark the entity model against a real sphinx-needs project.

`benchmark.py` measures general RST coverage against CPython. This one measures
*entity* coverage against the useblocks sphinx-needs demo: it migrates that
project with `rinx init --from-sphinx`, which converts its own sphinx-needs
configuration into a rinx entity schema (the conversion the language server
runs too), builds its documents against that schema, and reports everything
the pipeline did not understand.

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
import sys
import tempfile
from collections import Counter
from collections.abc import Iterable
from pathlib import Path
from typing import TextIO, TypeAlias

import benchmark_common
from benchmark_common import (
    WARMUP_PACKAGE,
    BuildResult,
    ReportLayout,
    SummaryRow,
    WarningCorpus,
    WarningEntry,
    WhitelistSummary,
    add_output_arguments,
    count_row,
    json_objects,
    whitelist_rows,
    write_frequency_summary,
    written_directive,
)

# One `(category, detail)` entry per construct the converter could not carry
# over, as `rinx init --from-sphinx` reports them.
Report: TypeAlias = list[tuple[str, str]]

TARGET_DIR = Path(tempfile.gettempdir()) / "rinx_benchmark_needs"
REPO_URL = "https://github.com/useblocks/sphinx-needs-demo.git"

# The one place the corpus version is decided.
#
# A tag rather than `main`, for the reason docs/benchmark.rst gives for CPython:
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
# That is forced, not a preference: rinx resolves a source-root-relative
# path (`/_images/logo.png`, and the `--doc-path` every phase keys off) against
# the workspace root, so a corpus whose `srcdir` is a subdirectory would have
# every absolute image path miss by that prefix. Making the two roots the same
# directory is the only arrangement in which the corpus' own paths resolve.
# The corpus therefore lives in the *root* package, `//:docs`.
CORPUS_SRCDIR = "docs"

# Generated packages placed inside the workspace. Each holds a BUILD file, which
# makes it a package of its own — so the root package's `glob(["**/*.rst"])`
# stops at its boundary and cannot pick up the warm-up document.
GENERATED_PACKAGES = ("assets", WARMUP_PACKAGE)

# The root package's build file, which `rinx init` writes for the corpus.
ROOT_BUILD_FILE = "BUILD.bazel"

# Hand-authored list of entity warnings we accept, relative to the workspace
# root (main() chdirs there via BUILD_WORKSPACE_DIRECTORY).
WHITELIST_PATH = Path("scripts/entity_warnings_whitelist.json")

# Where the analysis writes its full report, relative to the workspace root.
RESULT_PATH = Path("benchmark_entities_result.txt")


def clone_repo(version: str) -> None:
    """Shallow-clones the sphinx-needs demo at the tag matching `version`."""
    tag = f"v{version}"
    if not benchmark_common.clone_repo(REPO_URL, tag, TARGET_DIR):
        message = (
            f"Failed to clone the sphinx-needs demo at tag {tag!r}.\n"
            f"Check that the tag exists (`git ls-remote --tags {REPO_URL} '{tag}'`) — "
            "see DEMO_VERSION in this file."
        )
        raise SystemExit(message)


def corpus_workspace() -> Path:
    """The generated Bazel workspace: the clone's Sphinx source directory."""
    return TARGET_DIR / CORPUS_SRCDIR


def site_dir() -> Path:
    """Where the corpus build leaves the rendered site: the root package's site."""
    return corpus_workspace() / "bazel-bin" / "site_site_out"


def generate_bazel_project(rinx_root: str) -> None:
    """Make the clone's source directory a Bazel workspace of its own.

    The corpus' own build is not written here: `rinx init --from-sphinx`
    writes it from `conf.py` once the warm-up has built the binary (see
    `run_benchmark`), as it would for any Sphinx project.
    """
    print("Generating a Bazel workspace around the clone...")
    workspace = corpus_workspace()
    (workspace / "MODULE.bazel").write_text(
        benchmark_common.rinx_module_bazel("sphinx_needs_demo_bench", rinx_root)
    )
    benchmark_common.write_assets_alias(workspace)
    benchmark_common.generate_warmup_package(workspace, rinx_root)


def copy_outside_sources(workspace: Path, outside: Iterable[tuple[str, Path]]) -> list[Path]:
    """Place each source that lives outside the source root where the build looks for it.

    `outside` pairs that place, relative to `workspace`, with the source: what
    `rinx init` reports as `outside-source-root`. The parser resolves `..` by
    popping, so a `.. literalinclude:: ../pharaoh.toml` written in a
    root-level document is looked for at `pharaoh.toml` — inside the source
    root. Returns the copies, which the migration then declares.

    The alternative would be editing the corpus' own `.. literalinclude::`
    arguments, which would make the benchmark measure a document set nobody
    wrote. The generated workspace is disposable, so adding a file to it is the
    same kind of scaffolding as the generated BUILD file next to it.
    """
    copies = []
    for destination, source in outside:
        target = workspace / destination
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(Path(source).read_bytes())
        print(f"Copied {source} -> {target} (spliced in from outside the source root).")
        copies.append(target)
    return copies


# ── Build ─────────────────────────────────────────────────────────────────────


def discard_stale_corpus_outputs(workspace: Path) -> None:
    """Remove generated outputs left over from a previous run.

    The workspace is re-cloned every run but Bazel's output base survives it, so
    outputs for documents that are no longer there would be picked up by the
    `.ast` glob and inflate every count. `benchmark_common`'s version takes a
    package name; the corpus here *is* the root package, so what is kept is
    named instead: `external` (the fetched repositories and the compiled
    rinx binary, which stay warm) and the generated packages.
    """
    info = subprocess.run(
        ["bazel", "info", *benchmark_common.BUILD_CONFIG_FLAGS, "bazel-bin"],
        cwd=str(workspace),
        capture_output=True,
        text=True,
        check=False,
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


def warm_up(*, clean: bool = False) -> tuple[bool, float]:
    """Build the warm-up site, compiling rinx; whether it built, and how long it took."""
    workspace = corpus_workspace()
    if clean:
        print("Cleaning Bazel cache...")
        subprocess.run(["bazel", "clean"], cwd=str(workspace), capture_output=True, check=False)
    else:
        discard_stale_corpus_outputs(workspace)

    print("Building rinx and its toolchains (not timed as doc build)...")
    deps_built, deps_duration = benchmark_common.timed_bazel_build(
        workspace, f"//{WARMUP_PACKAGE}:site", "bazel_deps_build.log"
    )
    print(f"Dependency build finished in {deps_duration:.2f} seconds.")
    return deps_built, deps_duration


def build_corpus(warmup_seconds: float) -> BuildResult:
    """Build the corpus, after a warm-up that took `warmup_seconds`."""
    workspace = corpus_workspace()
    print("\nRunning Bazel build of the sphinx-needs demo documentation...")
    build_succeeded, duration = benchmark_common.timed_bazel_build(
        workspace, "//:site", "bazel_build.log"
    )
    if build_succeeded:
        print(f"Documentation build succeeded in {duration:.2f} seconds.")

    print(f"\nBazel build output was captured to: {workspace / 'bazel_build.log'}")
    print(f"HTML output is located at: {site_dir()}/")
    return BuildResult(
        succeeded=build_succeeded, warmup_seconds=warmup_seconds, corpus_seconds=duration
    )


# The corpus renders every document as a Jinja template before Sphinx parses
# it — `docs/conf.py`'s `rstjinja` handler on the `source-read` event, which
# reading `conf.py` cannot see — so the migration is told.
INIT_ARGS = ("--jinja",)


def migrate(binary: Path) -> Report | None:
    """Migrate the corpus with `rinx init --from-sphinx`.

    A dry run first names the sources the corpus splices in from outside its
    source root, which are copied to where the build looks for them and
    declared; the migration's report of sphinx-needs constructs the entity
    model has no vocabulary for — half the point of the whole benchmark — is
    returned. None when the migration failed.
    """
    workspace = corpus_workspace()
    # `rinx init` writes the root package's build file and overwrites none,
    # so the one an earlier run wrote into the clone makes way.
    (workspace / ROOT_BUILD_FILE).unlink(missing_ok=True)
    preview = benchmark_common.migrate_corpus(binary, workspace, INIT_ARGS, dry_run=True)
    if preview is None:
        return None
    outside = [
        (finding["build_path"], Path(finding["path"]))
        for finding in preview.of_kind("outside-source-root")
    ]
    copies = copy_outside_sources(workspace, outside)
    parse_data = [arg for copy in copies for arg in ("--parse-data", str(copy))]
    migration = benchmark_common.migrate_corpus(binary, workspace, [*INIT_ARGS, *parse_data])
    if migration is None:
        return None
    return [
        (finding["entry"]["category"], finding["entry"]["detail"])
        for finding in migration.of_kind("needs-unconverted")
    ]


def run_benchmark(*, clean: bool = False) -> tuple[BuildResult, Report]:
    """Warm up, migrate the corpus with the binary that built, then build it.

    The migration needs the `rinx` binary, and the warm-up is what compiles
    it, so it runs between the two builds and is not timed with either.
    """
    deps_built, deps_duration = warm_up(clean=clean)
    failed = BuildResult(succeeded=False, warmup_seconds=deps_duration, corpus_seconds=None)
    if not deps_built:
        return failed, []
    binary = benchmark_common.find_rinx_binary(corpus_workspace())
    if binary is None:
        print("Could not find the rinx binary the warm-up built.")
        return failed, []
    schema_report = migrate(binary)
    if schema_report is None:
        return failed, []
    return build_corpus(deps_duration), schema_report


# ── Analysis ──────────────────────────────────────────────────────────────────

# The one warning shape `crates/worker/src/commands/diagnostics.rs` emits, for
# parse, render and preview alike.
WARNING_LINE = re.compile(
    r"^warning: (?P<path>[^ ]+?)(?::\d+:\d+)?: "
    r"(?P<code>[a-z][a-z0-9]*(?:[.\-][a-z0-9]+)+): (?P<message>.*)$"
)


def parse_build_warnings(log_text: str) -> list[WarningEntry]:
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
    which matters because the whitelist matches on `(doc_path, kind, target)`.
    """
    entries: dict[benchmark_common.WarningKey, WarningEntry] = {}
    for raw_line in log_text.splitlines():
        match = WARNING_LINE.match(raw_line.strip())
        if match is None:
            continue
        entry: WarningEntry = {
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


def format_entity_warning(entry: WarningEntry, *, include_kind: bool = True) -> str:
    """One-line rendering of an entity warning or whitelist entry."""
    kind = f"{entry.get('kind')}: " if include_kind else ""
    return f"{entry.get('doc_path')}: {kind}{entry.get('target')}"


def tally_ast(ast_files: Iterable[Path]) -> tuple[Counter[str], Counter[str]]:
    """Walk the built `.ast` files, counting unknown directives and parsed entity instances.

    The second half is the positive signal: a type with zero instances means the
    corpus never exercised it, and a type the schema failed to declare shows up
    in the first half as an unknown directive instead.
    """
    unknown_directives: Counter[str] = Counter()
    entities: Counter[str] = Counter()

    for ast_file in ast_files:
        try:
            tree = json.loads(ast_file.read_text())
        except (OSError, ValueError) as e:
            print(f"Failed to parse {ast_file}: {e}")
            continue
        for node in json_objects(tree):
            directive = written_directive(node)
            if directive is None:
                continue
            if "Unknown" in directive:
                unknown_directives[directive["Unknown"].get("name", "unnamed")] += 1
            elif "Entity" in directive:
                entities[directive["Entity"].get("type_name", "unnamed")] += 1

    return unknown_directives, entities


def analyze_results(*, build_succeeded: bool, schema_report: Report) -> list[SummaryRow]:
    """Write the full report, returning the compact summary's rows."""
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

    with RESULT_PATH.open("w") as out:
        print("=== rinx entity benchmark: full report ===", file=out)
        write_frequency_summary(out, "Entities Parsed By Type", entities)
        write_frequency_summary(out, "Unsupported Directives Summary", unknown_directives)
        write_schema_report(out, schema_report)
        summary = benchmark_common.report_warnings(
            WarningCorpus(warnings, len(ast_files)),
            WHITELIST_PATH,
            ReportLayout(warning_sections(warnings), format_entity_warning),
            build_succeeded=build_succeeded,
            out=out,
        )

    print(f"Full report written to: {RESULT_PATH}")
    return summary_rows(entities, unknown_directives, schema_report, summary)


def warning_sections(warnings: Iterable[WarningEntry]) -> list[tuple[str, str]]:
    """One report section per diagnostic code seen, most frequent first.

    Built from the data rather than hard-coded: a code added on the Rust side
    should appear in the report without this file being touched.
    """
    counts = Counter(entry["kind"] for entry in warnings)
    ordered = sorted(counts.items(), key=lambda item: (-item[1], item[0]))
    return [(code, code) for code, _count in ordered]


def write_schema_report(out: TextIO, schema_report: Report) -> None:
    """List what the sphinx-needs configuration expresses and our model cannot, by category."""
    print("\nSphinx-Needs Constructs Without An Entity-Model Equivalent:", file=out)
    print("-" * 58, file=out)
    if not schema_report:
        print("None found.", file=out)
        return
    by_category: dict[str, list[str]] = {}
    for category, detail in schema_report:
        by_category.setdefault(category, []).append(detail)
    for category in sorted(by_category, key=lambda c: (-len(by_category[c]), c)):
        print(f"\n{category} ({len(by_category[category])}):", file=out)
        for detail in by_category[category]:
            print(f"  - {detail}", file=out)


def summary_rows(
    entities: Counter[str],
    unknown: Counter[str],
    schema_report: Report,
    summary: WhitelistSummary,
) -> list[SummaryRow]:
    """The compact summary's counts; the full listing is in the report."""
    rows = [
        count_row("Entity types exercised:", len(entities), entities.total()),
        count_row("Unsupported directives:", len(unknown), unknown.total()),
        count_row(
            "Unconvertible constructs:", len({c for c, _ in schema_report}), len(schema_report)
        ),
    ]
    rows.extend(
        count_row(f"{code}:", distinct, occurrences)
        for code, (distinct, occurrences) in summary.per_kind.items()
        if distinct
    )
    rows.extend(whitelist_rows(summary))
    return rows


def main() -> int:
    """Clone the sphinx-needs demo, build it against its converted schema, and report.

    Returns the exit status: non-zero only when the corpus did not build.
    """
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
    add_output_arguments(parser)
    args = parser.parse_args()

    rinx_root = os.environ.get("BUILD_WORKSPACE_DIRECTORY", str(Path.cwd()))
    os.chdir(rinx_root)

    if not Path("WORKSPACE").exists() and not Path("MODULE.bazel").exists():
        print("Please run this script from the root of the rinx workspace.")
        return 2

    print(f"Benchmarking the entity model against sphinx-needs-demo {args.demo_version}.")
    clone_repo(args.demo_version)
    generate_bazel_project(rinx_root)
    result, schema_report = run_benchmark(clean=args.clean)
    rows = analyze_results(build_succeeded=result.succeeded, schema_report=schema_report)
    outcome = benchmark_common.RunOutcome(
        title=f"sphinx-needs demo {args.demo_version}",
        build=result,
        analysis_rows=rows,
        site_dir=site_dir(),
        # The corpus is the workspace's root package, so its root document is the site's.
        entry="index.html",
        report=RESULT_PATH,
    )
    return benchmark_common.finish_run(outcome, benchmark_common.RunOutputs.from_args(args))


if __name__ == "__main__":
    sys.exit(main())
