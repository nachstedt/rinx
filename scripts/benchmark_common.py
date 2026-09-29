"""Corpus-agnostic machinery shared by the benchmark scripts.

`benchmark.py` (CPython, general RST coverage) and `benchmark_entities.py`
(sphinx-needs, entity-model coverage) do the same five things — clone a corpus
at a pinned tag, generate a Bazel workspace around it, build a warm-up site,
time the corpus build, and diff the resulting warnings against a hand-authored
whitelist. Only the corpus, the generated `BUILD.bazel` and the shape of the
report differ, so everything else lives here.

The one structural difference from the original single-script version: every
function takes the generated workspace directory explicitly rather than reading
a module-level `TARGET_DIR`, since there are now two of them.
"""

import argparse
import json
import os
import shutil
import subprocess
import time
from collections.abc import Iterable, Iterator, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any, NamedTuple, Protocol, TextIO, TypeAlias, TypedDict

# Configuration flags shared by an inner `bazel build` and the `bazel info`
# used to locate its outputs. They must stay identical: `bazel info bazel-bin`
# answers for whichever configuration it is asked about, so querying with
# different flags returns a different (wrong) output directory.
BUILD_CONFIG_FLAGS = ["-c", "opt", "--host_compilation_mode=opt"]

# Package holding the throwaway one-document site whose build is what compiles
# the rinx binary, so its cost lands outside the timed corpus build.
WARMUP_PACKAGE = "bench_warmup"


class WarningEntry(TypedDict, total=False):
    """One warning as a sidecar or a whitelist file holds it.

    Every key is optional: a sidecar's entries are the worker's JSON and the
    whitelist's are hand-written, so each reader asks with `.get`.
    """

    doc_path: str
    kind: str
    target: str
    requested_type: str
    resolved_type: str
    candidates: list[str]
    # A whitelist entry's reason for being there.
    comment: str


# The identity of a warning for whitelist matching: (doc_path, kind, target).
WarningKey: TypeAlias = tuple[str | None, str | None, str | None]


class FormatEntry(Protocol):
    """Renders one warning as a line of the report."""

    def __call__(self, entry: WarningEntry, *, include_kind: bool) -> str:
        """Render `entry`, naming its kind only when `include_kind` asks."""
        ...


@dataclass(frozen=True)
class WarningCorpus:
    """Every warning a build emitted, and how many reports they came from."""

    entries: list[WarningEntry]
    # The number of files read, which the pruning safety guard needs: an empty
    # corpus must not prune.
    file_count: int


@dataclass(frozen=True)
class ReportLayout:
    """How one benchmark's warnings are listed.

    `sections` is a list of (title, kind) pairs: the new warnings are split by
    `kind` so lists that call for different fixes read as separate, scannable
    sections rather than being interleaved by frequency.
    """

    sections: Sequence[tuple[str, str]]
    format_entry: FormatEntry


class SectionCount(NamedTuple):
    """The size of one listed section."""

    distinct: int
    occurrences: int


@dataclass(frozen=True)
class WhitelistSummary:
    """The counts `report_warnings` hands back for the terminal summary."""

    # Each section's kind, or None for the trailing "Other" section.
    per_kind: dict[str | None, SectionCount]
    suppressed: int
    whitelist_size: int
    stale_count: int
    stale_pruned: bool


def clone_repo(repo_url: str, tag: str, target_dir: Path) -> bool:
    """Shallow-clones `repo_url` at `tag` into a freshly emptied `target_dir`.

    Deliberately a tag rather than a branch: a benchmark against a moving
    branch produces numbers that change on their own, so a delta could never be
    attributed to a local change with confidence.
    """
    print(f"Target directory: {target_dir}")
    if target_dir.exists():
        print("Directory already exists. Removing it for a fresh clone...")
        shutil.rmtree(target_dir, ignore_errors=False)
        if target_dir.exists():
            # This could happen on some filesystems due to latency or locks
            print("Warning: Directory still exists after rmtree, attempting one more time...")
            shutil.rmtree(target_dir, ignore_errors=True)

    print(f"Cloning {repo_url} at tag {tag}...")
    target_dir.parent.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        ["git", "clone", "--depth", "1", "--branch", tag, repo_url, str(target_dir)],
        check=False,
    )
    return result.returncode == 0


def rinx_module_bazel(module_name: str, rinx_root: str) -> str:
    """A generated workspace's MODULE.bazel, depending on the local rinx checkout.

    Every benchmark workspace — and the warm-up one — starts from this text, so
    they declare rinx identically and their actions hash alike: that is what
    lets the corpora reuse the rinx binary the warm-up compiled into a shared
    disk cache, instead of compiling it once each.
    """
    return f"""module(name = "{module_name}")

bazel_dep(name = "rinx", version = "0.0.0")
local_path_override(
    module_name = "rinx",
    path = "{rinx_root}",
)
"""


def write_assets_alias(workspace: Path) -> None:
    """Writes the `//assets:default.css` alias the generated sites' stylesheet is."""
    assets_dir = workspace / "assets"
    assets_dir.mkdir(exist_ok=True)
    (assets_dir / "BUILD.bazel").write_text("""alias(
    name = "default.css",
    actual = "@rinx//:assets/default.css",
    visibility = ["//visibility:public"],
)
""")


def generate_warmup_package(target_dir: Path, workspace_root: str) -> None:
    """Writes a one-document site used to build rinx before timing.

    The measurement we want is the documentation build alone, but a fresh
    workspace has to compile the `rinx` binary (and resolve the Rust,
    Java and Python toolchains) first, and that dwarfs it. Those tools are
    built in Bazel's *exec* configuration, so `bazel build
    @rinx//:rinx` would warm a differently-configured
    binary and leave the real one to be compiled inside the timed step.

    Building a trivial site instead warms exactly the configurations the corpus
    build will use, for the cost of parsing and rendering one tiny document. It
    lives in its own top-level package so it stays outside the corpus package's
    `glob(["**/*.rst"])`.
    """
    warmup_dir = target_dir / WARMUP_PACKAGE
    warmup_dir.mkdir(exist_ok=True)

    (warmup_dir / "index.rst").write_text("""Warmup
======

A single paragraph, built only to compile the rinx binary.
""")
    (warmup_dir / "rinx.toml").write_text('project = "Warmup"\n')
    default_template_path = Path(workspace_root) / "templates" / "default.html"
    (warmup_dir / "custom_template.html").write_text(default_template_path.read_text())

    (warmup_dir / "BUILD.bazel").write_text("""load("@rinx//:defs.bzl", "rinx_library", "rinx_site")

rinx_library(
    name = "warmup_docs",
    srcs = ["index.rst"],
)

rinx_site(
    name = "site",
    config = "rinx.toml",
    template = "custom_template.html",
    css = "//assets:default.css",
    deps = [":warmup_docs"],
)
""")


def discard_stale_corpus_outputs(target_dir: Path, corpus_package: str) -> None:
    """Removes generated outputs for a corpus that is no longer checked out.

    The workspace directory is deleted and re-cloned on every run, but Bazel's
    output base lives outside it and survives — so `.ast` and `.warnings.json`
    files belonging to a *previous* corpus stay behind, get picked up by the
    `bazel-bin/<corpus_package>/**` globs during analysis, and inflate every
    count with documents that are not part of this build.

    Only the corpus outputs are discarded, not the whole cache — the Rust
    toolchain and worker binary stay warm. The `.ast` files have to be rebuilt
    regardless, since their sources changed.
    """
    # `bazel info bazel-bin` reports the path for the configuration it is asked
    # about, so it has to be given the same flags as the build — without them it
    # answers for `fastbuild` while the build writes to `opt`, and the wrong
    # directory gets deleted.
    info = subprocess.run(
        ["bazel", "info", *BUILD_CONFIG_FLAGS, "bazel-bin"],
        cwd=str(target_dir),
        capture_output=True,
        text=True,
        check=False,
    )
    if info.returncode != 0:
        return

    stale = Path(info.stdout.strip()) / corpus_package
    if stale.exists():
        print("Discarding generated outputs from a previous corpus...")
        shutil.rmtree(stale, ignore_errors=True)


def timed_bazel_build(
    target_dir: Path, target: str, log_name: str, extra_flags: Iterable[str] = ()
) -> tuple[bool, float]:
    """Runs one `bazel build` in the generated workspace and times it.

    Returns (succeeded, duration_seconds). The build's output is captured (so
    the caller can keep the terminal readable) and written to `log_name` in the
    workspace on success and failure alike.
    """
    command = [
        "bazel",
        "build",
        *BUILD_CONFIG_FLAGS,
        "--spawn_strategy=local",
        *extra_flags,
        target,
    ]

    start_time = time.time()
    result = subprocess.run(
        command, cwd=str(target_dir), capture_output=True, text=True, check=False
    )
    duration = time.time() - start_time

    # Persist the inner build's captured output — it's swallowed by
    # capture_output above, so without this it's invisible on a successful
    # build. Written on success and failure alike, so it's always inspectable.
    log_path = target_dir / log_name
    log_path.write_text(result.stdout + result.stderr)

    if result.returncode != 0:
        print(f"Bazel build of {target} failed! Output: {log_path}")
        print(result.stderr)

    return result.returncode == 0, duration


def json_objects(node: object) -> Iterator[dict[str, Any]]:
    """Every JSON object in a parsed `.ast` tree, the root included, depth-first."""
    if isinstance(node, dict):
        yield node
        for value in node.values():
            yield from json_objects(value)
    elif isinstance(node, list):
        for item in node:
            yield from json_objects(item)


# ── Warning whitelist helpers (pure, unit-tested) ─────────────────────────────


def collect_warning_sidecars(bazel_bin_dir: Path) -> WarningCorpus:
    """Load every per-document .warnings.json sidecar under bazel_bin_dir.

    They are flattened into a single list of warning entries, each augmented
    with the doc_path of the report it came from.
    """
    warning_files = sorted(bazel_bin_dir.glob("**/*.warnings.json"))
    entries: list[WarningEntry] = []
    for warning_file in warning_files:
        try:
            report = json.loads(warning_file.read_text())
        except (OSError, ValueError) as e:
            print(f"Failed to parse {warning_file}: {e}")
            continue
        doc_path = report.get("doc_path", str(warning_file))
        for warning in report.get("warnings", []):
            entry = WarningEntry(**warning)
            entry["doc_path"] = doc_path
            entries.append(entry)
    return WarningCorpus(entries, len(warning_files))


def warning_key(entry: WarningEntry) -> WarningKey:
    """The identity of a warning for whitelist matching.

    The requested/resolved object types are informational payload, deliberately
    not part of the key.
    """
    return (entry.get("doc_path"), entry.get("kind"), entry.get("target"))


def load_whitelist(path: Path) -> list[WarningEntry]:
    """Return the list of whitelist entries, or [] if the file doesn't exist."""
    if not path.exists():
        return []
    data = json.loads(path.read_text())
    return [WarningEntry(**entry) for entry in data.get("entries", [])]


def write_whitelist(path: Path, entries: list[WarningEntry]) -> None:
    """Rewrite the whitelist file with the given entries."""
    path.write_text(json.dumps({"entries": entries}, indent=2) + "\n")


def partition_warnings(
    actual: Iterable[WarningEntry], whitelist_entries: Iterable[WarningEntry]
) -> tuple[list[WarningEntry], int]:
    """Split actual warnings into those not covered by the whitelist.

    Returns those 'new' warnings, de-duplicated by key, and how many actual
    occurrences the whitelist suppressed.
    """
    whitelist_keys = {warning_key(w) for w in whitelist_entries}
    new_by_key: dict[WarningKey, WarningEntry] = {}
    suppressed = 0
    for entry in actual:
        key = warning_key(entry)
        if key in whitelist_keys:
            suppressed += 1
        elif key not in new_by_key:
            new_by_key[key] = entry
    return list(new_by_key.values()), suppressed


def prune_whitelist(
    whitelist_entries: list[WarningEntry], actual: Iterable[WarningEntry]
) -> tuple[list[WarningEntry], list[WarningEntry]]:
    """Partition the whitelist into entries still justified by an actual warning or not.

    Returns ('kept', 'removed'): the latter no longer match anything.
    """
    actual_keys = {warning_key(a) for a in actual}
    kept = [w for w in whitelist_entries if warning_key(w) in actual_keys]
    removed = [w for w in whitelist_entries if warning_key(w) not in actual_keys]
    return kept, removed


def report_warnings(
    corpus: WarningCorpus,
    whitelist_path: Path,
    layout: ReportLayout,
    *,
    build_succeeded: bool,
    out: TextIO,
) -> WhitelistSummary:
    """Diff emitted warnings against the whitelist and write the listing to `out`.

    Stale whitelist entries are auto-pruned when it is safe to do so. Warnings
    whose kind matches none of the layout's sections land in a trailing "Other"
    section, so a kind added on the Rust side can never be silently dropped
    from the report.

    Pruning is destructive (it deletes hand-written comments), so it only runs
    when the warning data is trustworthy: the build succeeded AND at least one
    sidecar was found.
    """
    actual = corpus.entries
    whitelist_entries = load_whitelist(whitelist_path)
    new_warnings, suppressed = partition_warnings(actual, whitelist_entries)
    occurrences = _count_occurrences(actual)

    per_kind: dict[str | None, SectionCount] = {}
    for title, kind in layout.sections:
        section = [e for e in new_warnings if e.get("kind") == kind]
        per_kind[kind] = _write_warning_section(
            out, title, section, occurrences, layout.format_entry
        )

    known_kinds = {kind for _, kind in layout.sections}
    other = [e for e in new_warnings if e.get("kind") not in known_kinds]
    if other:
        per_kind[None] = _write_warning_section(
            out, "Other Warnings", other, occurrences, layout.format_entry
        )

    print(
        f"\n{suppressed} warning occurrence(s) suppressed by whitelist "
        f"({len(whitelist_entries)} entr(y/ies)).",
        file=out,
    )

    kept, removed = prune_whitelist(whitelist_entries, actual)
    _list_stale_entries(out, removed, layout.format_entry)
    pruned = False
    if removed:
        pruned = _prune_stale_entries(
            out,
            whitelist_path,
            kept,
            len(removed),
            untrustworthy_because=_why_untrustworthy(
                build_succeeded=build_succeeded, file_count=corpus.file_count
            ),
        )

    return WhitelistSummary(
        per_kind=per_kind,
        suppressed=suppressed,
        whitelist_size=len(whitelist_entries),
        stale_count=len(removed),
        stale_pruned=pruned,
    )


def _count_occurrences(actual: Iterable[WarningEntry]) -> dict[WarningKey, int]:
    """How many times each distinct warning key occurs across the corpus."""
    counts: dict[WarningKey, int] = {}
    for entry in actual:
        key = warning_key(entry)
        counts[key] = counts.get(key, 0) + 1
    return counts


def _write_warning_section(
    out: TextIO,
    title: str,
    section: list[WarningEntry],
    occurrences: Mapping[WarningKey, int],
    format_entry: FormatEntry,
) -> SectionCount:
    """Write one titled, most-frequent-first warning listing, returning its size."""
    header = f"{title} ({len(section)}):"
    print(f"\n{header}", file=out)
    print("-" * len(header), file=out)
    if not section:
        print("None found.", file=out)
    else:
        for entry in sorted(section, key=lambda e: occurrences[warning_key(e)], reverse=True):
            print(
                f"{occurrences[warning_key(entry)]:5d}  {format_entry(entry, include_kind=False)}",
                file=out,
            )
    return SectionCount(len(section), sum(occurrences[warning_key(e)] for e in section))


def _list_stale_entries(
    out: TextIO, removed: list[WarningEntry], format_entry: FormatEntry
) -> None:
    """List whitelist entries no longer justified by a warning."""
    if not removed:
        return
    print("\nStale Whitelist Entries:", file=out)
    print("------------------------", file=out)
    for entry in removed:
        comment = entry.get("comment", "")
        suffix = f"  # {comment}" if comment else ""
        print(f"- {format_entry(entry, include_kind=True)}{suffix}", file=out)


def _why_untrustworthy(*, build_succeeded: bool, file_count: int) -> str | None:
    """Why the warning data cannot justify pruning, or None when it can.

    Pruning is destructive (it deletes hand-written comments), so it only runs
    when the build succeeded AND at least one sidecar was found.
    """
    if not build_succeeded:
        return "build did not succeed"
    if file_count == 0:
        return "no .warnings.json sidecars were found"
    return None


def _prune_stale_entries(
    out: TextIO,
    whitelist_path: Path,
    kept: list[WarningEntry],
    stale_count: int,
    *,
    untrustworthy_because: str | None,
) -> bool:
    """Rewrite the whitelist without its stale entries if the data allows; return whether it did."""
    if untrustworthy_because is None:
        write_whitelist(whitelist_path, kept)
        print(f"\nRemoved {stale_count} stale entr(y/ies) from {whitelist_path}.", file=out)
        return True
    print(
        f"\nSkipped auto-pruning ({untrustworthy_because}); "
        f"{stale_count} entr(y/ies) left untouched.",
        file=out,
    )
    return False


# ── Report formatting ─────────────────────────────────────────────────────────


def write_frequency_summary(
    out: TextIO,
    title: str,
    counts: Mapping[str, int],
    key_prefix: str = "",
    key_suffix: str = "",
) -> None:
    """Write one `name: count` frequency table (descending) to the detailed report `out`."""
    print(f"\n{title}:", file=out)
    print("-" * (len(title) + 1), file=out)
    if not counts:
        print("None found.", file=out)
        return
    for name, count in sorted(counts.items(), key=lambda x: x[1], reverse=True):
        print(f"{key_prefix}{name}{key_suffix}: {count}", file=out)


class SummaryRow(NamedTuple):
    """One line of a benchmark's compact summary: a label and its value, already worded."""

    label: str
    value: str


@dataclass(frozen=True)
class BuildResult:
    """How the two builds of one benchmark run went."""

    # Whether the corpus built — what decides the benchmark's exit status.
    succeeded: bool
    warmup_seconds: float
    # None when the warm-up build failed, so the corpus build never ran.
    corpus_seconds: float | None


# The names a benchmark's full report and its front page's path are exported
# under, beside its site; scripts/publish_pages.py reads both.
REPORT_NAME = "report.txt"
ENTRY_NAME = "entry.txt"


def count_row(label: str, distinct: int, occurrences: int | None = None) -> SummaryRow:
    """A summary row counting distinct findings, and how often they occur when known."""
    occ = f" ({occurrences} occurrences)" if occurrences is not None else ""
    return SummaryRow(label, f"{distinct} distinct{occ}")


def whitelist_rows(summary: WhitelistSummary) -> list[SummaryRow]:
    """The trailing whitelist rows every benchmark's summary ends with."""
    rows = [
        SummaryRow(
            "Suppressed by whitelist:",
            f"{summary.suppressed} occurrences ({summary.whitelist_size} entries)",
        )
    ]
    if summary.stale_count:
        action = "removed" if summary.stale_pruned else "left untouched"
        rows.append(SummaryRow("Stale whitelist entries:", f"{summary.stale_count} ({action})"))
    return rows


def timing_rows(result: BuildResult) -> list[SummaryRow]:
    """The two builds' durations, the corpus build's marked when it failed or never ran."""
    if result.corpus_seconds is None:
        corpus = "not run"
    elif result.succeeded:
        corpus = f"{result.corpus_seconds:.1f} s"
    else:
        corpus = f"failed after {result.corpus_seconds:.1f} s"
    return [
        SummaryRow("Warm-up build (untimed):", f"{result.warmup_seconds:.1f} s"),
        SummaryRow("Corpus build:", corpus),
    ]


def render_terminal_summary(title: str, rows: Iterable[SummaryRow]) -> str:
    """The compact, aligned summary printed at the end of a run."""
    lines = [f"=== {title} ==="]
    lines.extend(f"  {row.label:<28}{row.value}" for row in rows)
    return "\n".join(lines)


def render_markdown_summary(title: str, rows: Iterable[SummaryRow]) -> str:
    """The same summary as a Markdown table, for a CI job's step summary."""
    lines = [f"### {title}", "", "| | |", "|---|---|"]
    lines.extend(
        f"| {_markdown_cell(row.label.removesuffix(':'))} | {_markdown_cell(row.value)} |"
        for row in rows
    )
    lines.append("")
    return "\n".join(lines) + "\n"


def _markdown_cell(text: str) -> str:
    """`text` escaped so it cannot end its table cell."""
    return text.replace("|", "\\|")


def append_markdown_summary(path: Path, title: str, rows: Iterable[SummaryRow]) -> None:
    """Appends the Markdown summary to `path`, which other steps may also write to."""
    with path.open("a") as out:
        out.write(render_markdown_summary(title, rows))


def export_site(site_dir: Path, report: Path, entry: str, dest: Path) -> int:
    """Copies the built site and its report into `dest`, returning the site's size in bytes.

    Contents only, and every symlink Bazel's output tree holds is followed: the
    copy has to outlive the generated workspace, and has to be writable, since
    Bazel leaves its outputs read-only and a later export replaces this one.
    `entry`, the front page relative to the site, is written beside it so a
    publisher needs to know nothing about any one benchmark.
    """
    if dest.exists():
        shutil.rmtree(dest)
    size = 0
    for directory, _, files in os.walk(site_dir, followlinks=True):
        for name in files:
            source = Path(directory) / name
            target = dest / source.relative_to(site_dir)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            size += target.stat().st_size
    dest.mkdir(parents=True, exist_ok=True)
    (dest / ENTRY_NAME).write_text(entry + "\n")
    if report.is_file():
        shutil.copyfile(report, dest / REPORT_NAME)
    return size


def site_size_row(size: int) -> SummaryRow:
    """The exported site's size, which the published branch has a budget for."""
    return SummaryRow("Site size:", f"{size / 1_000_000:.1f} MB")


def add_output_arguments(parser: argparse.ArgumentParser) -> None:
    """The two flags a CI run exports a benchmark's results through."""
    parser.add_argument(
        "--site-out",
        type=Path,
        help=(
            "Copy the rendered site, and the full report as report.txt, into this "
            "directory once the corpus has built (replacing what is there)."
        ),
    )
    parser.add_argument(
        "--summary-markdown",
        type=Path,
        help="Append the summary as a Markdown table to this file, e.g. $GITHUB_STEP_SUMMARY.",
    )


@dataclass(frozen=True)
class RunOutcome:
    """What one benchmark run produced: its builds, its summary, and where its files are."""

    title: str
    build: BuildResult
    analysis_rows: Sequence[SummaryRow]
    site_dir: Path
    # The site's front page, relative to `site_dir`.
    entry: str
    report: Path


@dataclass(frozen=True)
class RunOutputs:
    """Where `--site-out` and `--summary-markdown` asked for the results, if anywhere."""

    site_out: Path | None = None
    summary_markdown: Path | None = None

    @classmethod
    def from_args(cls, args: argparse.Namespace) -> "RunOutputs":
        """The outputs the flags `add_output_arguments` declares asked for."""
        return cls(site_out=args.site_out, summary_markdown=args.summary_markdown)


def finish_run(outcome: RunOutcome, outputs: RunOutputs) -> int:
    """Prints the summary, writes the outputs asked for, and returns the exit status.

    The site is exported only when the corpus built: a failed build leaves no
    site, or a stale one from an earlier run.
    """
    rows = [*timing_rows(outcome.build)]
    if outputs.site_out is not None and outcome.build.succeeded:
        size = export_site(outcome.site_dir, outcome.report, outcome.entry, outputs.site_out)
        rows.append(site_size_row(size))
        print(f"Site exported to: {outputs.site_out}")
    rows.extend(outcome.analysis_rows)
    print("\n" + render_terminal_summary(outcome.title, rows))
    if outputs.summary_markdown is not None:
        append_markdown_summary(outputs.summary_markdown, outcome.title, rows)
    return exit_status(build_succeeded=outcome.build.succeeded)


def exit_status(*, build_succeeded: bool) -> int:
    """The benchmark's exit status: failing only when the corpus did not build.

    New warnings never fail it; they are what the report is for.
    """
    return 0 if build_succeeded else 1
