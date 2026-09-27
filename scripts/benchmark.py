"""Benchmark rinx against CPython's documentation: general RST coverage.

Clones a pinned CPython release, builds its `Doc/` tree with rinx in a
generated Bazel workspace, times the build, and reports what the corpus uses
that rinx does not support. See docs/benchmark.rst.
"""

import argparse
import json
import os
import subprocess
import tempfile
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, TextIO

import benchmark_common
from benchmark_common import (
    WARMUP_PACKAGE,
    ReportLayout,
    SectionCount,
    WarningEntry,
    WhitelistSummary,
    collect_warning_sidecars,
    json_objects,
    write_frequency_summary,
)

TARGET_DIR = Path(tempfile.gettempdir()) / "rinx_benchmark_cpython"
REPO_URL = "https://github.com/python/cpython.git"

# The one place the benchmark's Python version is decided.
#
# It pins BOTH halves of the corpus so they cannot drift apart: the CPython
# release tag whose `Doc/` we build (`v3.14.2`), and the interpreter the
# generated workspace resolves (`python.toolchain(python_version = ...)`).
# Previously the script cloned `main` while the toolchain came from
# rinx's own MODULE.bazel, so the documentation described a
# development version whose APIs the interpreter did not have — every doctest
# using a newly added API failed for a reason that had nothing to do with
# rinx. Examples seen: `re.Pattern.prefixmatch`,
# `IPv4Network.next_network`, `PrettyPrinter(expand=...)`, `shlex.quote(force=...)`.
#
# Changing this requires a version that exists on *both* sides:
#   - a `v<version>` tag in the CPython repository, and
#   - an entry in rules_python's `TOOL_VERSIONS` (python/versions.bzl) for the
#     rules_python release this workspace depends on.
# 3.14.2 is the newest that satisfies both today; CPython has later 3.14.x
# tags, but rules_python 2.0.0 does not ship interpreters for them.
PYTHON_VERSION = "3.14.2"

# Hand-authored list of domain-object warnings we accept, relative to the
# workspace root (main() chdirs there via BUILD_WORKSPACE_DIRECTORY).
WHITELIST_PATH = Path("scripts/domain_warnings_whitelist.json")

# The corpus package inside the generated workspace: the directory the `.rst`
# files live in, and therefore the one whose stale Bazel outputs get discarded.
CORPUS_PACKAGE = "Doc"


def clone_repo(python_version: str) -> None:
    """Shallow-clones the CPython release tag matching `python_version`.

    Deliberately a tag rather than `main`: the documentation has to describe the
    same interpreter the generated workspace runs, or doctests fail on APIs that
    simply do not exist yet.
    """
    tag = f"v{python_version}"
    if not benchmark_common.clone_repo(REPO_URL, tag, TARGET_DIR):
        message = (
            f"Failed to clone CPython at tag {tag!r}.\n"
            f"Check that the tag exists (`git ls-remote --tags {REPO_URL} '{tag}'`) "
            f"and that rules_python ships an interpreter for {python_version} — "
            f"both are required, see PYTHON_VERSION in this file."
        )
        raise SystemExit(message)


def generate_bazel_project(workspace_root: str, python_version: str) -> None:
    """Write the throwaway Bazel workspace around the cloned `Doc/` tree."""
    print(
        "Generating artificial Bazel project in /tmp "
        "(MODULE.bazel, BUILD.bazel, config, and template)..."
    )

    # Create MODULE.bazel
    #
    # The Python toolchain is pinned to the exact patch release whose `Doc/`
    # tree this workspace was cloned from, so an interpreter and the
    # documentation describing it can never disagree. `is_default = True` is
    # honoured only for the root module, and this generated workspace *is* the
    # root when the benchmark builds — so this pin wins over the 3.x toolchain
    # rinx registers for its own tests.
    module_bazel = f"""module(name = "cpython_docs_bench")

bazel_dep(name = "rinx", version = "0.0.0")
local_path_override(
    module_name = "rinx",
    path = "{workspace_root}",
)

# Pinned to the exact release this corpus was cloned from ({python_version});
# see PYTHON_VERSION in scripts/benchmark.py.
bazel_dep(name = "rules_python", version = "2.0.0")

python = use_extension("@rules_python//python/extensions:python.bzl", "python")
python.toolchain(
    python_version = "{python_version}",
    is_default = True,
)
"""
    (TARGET_DIR / "MODULE.bazel").write_text(module_bazel)

    # Create root BUILD.bazel (empty, as aliases are no longer needed)
    (TARGET_DIR / "BUILD.bazel").write_text("")

    # Create assets/BUILD.bazel for the CSS dependency
    assets_dir = TARGET_DIR / "assets"
    assets_dir.mkdir(exist_ok=True)
    (assets_dir / "BUILD.bazel").write_text("""alias(
    name = "default.css",
    actual = "@rinx//:assets/default.css",
    visibility = ["//visibility:public"],
)
""")

    doc_dir = TARGET_DIR / "Doc"

    # Create config
    (doc_dir / "rinx.toml").write_text('project = "CPython Benchmark"\n')

    # Create template — use the project's default template for a nicely formatted output
    default_template_path = Path(workspace_root) / "templates" / "default.html"
    template_content = default_template_path.read_text()
    (doc_dir / "custom_template.html").write_text(template_content)

    # Create BUILD.bazel in Doc
    build_bazel = """load("@rinx//:defs.bzl", "rinx_library", "rinx_site")

rinx_library(
    name = "cpython_docs",
    srcs = glob(["**/*.rst"]),
    # CPython's documents show real pictures, and an image has to be declared
    # to reach the site — the same rule `csv_data` follows for the files a
    # `.. csv-table::` reads. Globbed rather than listed because this file is
    # generated: the corpus decides what is there, not us.
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
)

rinx_site(
    name = "site",
    config = "rinx.toml",
    template = "custom_template.html",
    css = "//assets:default.css",
    deps = [":cpython_docs"],
)
"""
    (doc_dir / "BUILD.bazel").write_text(build_bazel)

    benchmark_common.generate_warmup_package(TARGET_DIR, workspace_root)


def run_benchmark(*, clean: bool = False) -> bool:
    """Builds rinx first, then times the CPython documentation build.

    The two are separate `bazel build` invocations on purpose. What the
    benchmark is about is how long it takes to turn 500-odd `.rst` files into
    HTML, and a single build would fold compiling the Rust binary and
    downloading toolchains into that number — dominating it after `--clean`,
    and making the figure depend on how warm the Bazel cache happened to be.
    Building the warmup site first pays that cost outside the measurement, in
    exactly the configurations the corpus build then reuses.
    """
    if clean:
        # Run bazel clean to avoid caching from previous runs
        print("Cleaning Bazel cache...")
        subprocess.run(["bazel", "clean"], cwd=str(TARGET_DIR), capture_output=True, check=False)
    else:
        benchmark_common.discard_stale_corpus_outputs(TARGET_DIR, CORPUS_PACKAGE)

    print("Building rinx and its toolchains (not timed as doc build)...")
    deps_built, deps_duration = benchmark_common.timed_bazel_build(
        TARGET_DIR, f"//{WARMUP_PACKAGE}:site", "bazel_deps_build.log"
    )
    print(f"Dependency build finished in {deps_duration:.2f} seconds.")
    if not deps_built:
        return False

    print("\nRunning Bazel build of the CPython documentation...")
    build_succeeded, duration = benchmark_common.timed_bazel_build(
        TARGET_DIR,
        "//Doc:site",
        "bazel_build.log",
        extra_flags=["--profile=profile.json.gz"],
    )

    if build_succeeded:
        print(f"Documentation build succeeded in {duration:.2f} seconds.")
        print(f"(Excludes {deps_duration:.2f} seconds spent building rinx itself.)")

    build_log = TARGET_DIR / "bazel_build.log"
    print(f"\nBazel build output was captured to: {build_log}")

    # Inform the user where the HTML is
    html_out = TARGET_DIR / "bazel-bin/Doc/site_site_out"
    print(f"HTML output is located at: {html_out}/")

    # Inform the user where the Bazel profile is
    profile_out = TARGET_DIR / "profile.json.gz"
    print(f"Bazel profile is located at: {profile_out}")
    print("You can view it by dropping the file into https://ui.perfetto.dev/ or chrome://tracing")

    return build_succeeded


# ── Domain-object warning whitelist helpers (pure, unit-tested) ───────────────


def format_warning(entry: WarningEntry, *, include_kind: bool = True) -> str:
    """One-line human-readable rendering of a warning or whitelist entry.

    Surfaces the object-type info ("missed type"): a mismatch shows
    'requested -> resolved'; a broken reference (nothing resolved) shows just
    the requested type it was looking for; an ambiguous reference also lists
    the qualified names it matched, since choosing between them is the fix.
    Pass include_kind=False when the surrounding section already states the
    kind (avoids a redundant token).
    """
    requested = entry.get("requested_type")
    resolved = entry.get("resolved_type")
    candidates = entry.get("candidates")
    if requested and resolved:
        detail = f" ({requested} -> {resolved})"
    elif requested:
        detail = f" (referenced as {requested})"
    elif resolved:
        detail = f" (resolved as {resolved})"
    else:
        detail = ""
    if candidates:
        detail += f" matching {', '.join(candidates)}"
    kind = f"{entry.get('kind')} " if include_kind else ""
    return f"{entry.get('doc_path')}: {kind}'{entry.get('target')}'{detail}"


@dataclass(frozen=True)
class DomainSummary:
    """The domain-object warning counts of the terminal summary."""

    unresolved: SectionCount
    ambiguous: SectionCount
    mismatch: SectionCount
    whitelist: WhitelistSummary


DOMAIN_WARNING_LAYOUT = ReportLayout(
    sections=[
        ("Unresolved Domain-Object References", "domain_object_reference"),
        ("Ambiguous Domain-Object References", "ambiguous_domain_object_reference"),
        ("Domain-Object Type Mismatches", "object_type_mismatch"),
    ],
    format_entry=format_warning,
)


def report_domain_warnings(
    bazel_bin_dir: Path, whitelist_path: Path, *, build_succeeded: bool, out: TextIO
) -> DomainSummary:
    """Diff the emitted domain-object warnings against the whitelist.

    Writes the detailed listing to the `out` file handle, and auto-prunes stale
    whitelist entries when it is safe to do so. Returns the counts for the
    terminal summary.

    The three sections are separate because they call for different fixes:
    "couldn't resolve at all", "matched several objects" and "resolved to the
    wrong object type".
    """
    summary = benchmark_common.report_warnings(
        collect_warning_sidecars(bazel_bin_dir),
        whitelist_path,
        DOMAIN_WARNING_LAYOUT,
        build_succeeded=build_succeeded,
        out=out,
    )
    return DomainSummary(
        unresolved=summary.per_kind["domain_object_reference"],
        ambiguous=summary.per_kind["ambiguous_domain_object_reference"],
        mismatch=summary.per_kind["object_type_mismatch"],
        whitelist=summary,
    )


# The `.. toctree::` options with a value, as the AST names them.
TOCTREE_VALUE_OPTIONS = ("maxdepth", "numbered", "caption", "name")


@dataclass
class AstTally:
    """What the corpus' `.ast` files contain that the report counts."""

    unknown_directives: Counter[str] = field(default_factory=Counter)
    malformed_directives: Counter[str] = field(default_factory=Counter)
    toctree_options_used: Counter[str] = field(default_factory=Counter)
    parser_diagnostics: Counter[str] = field(default_factory=Counter)

    def count(self, tree: object) -> None:
        """Walk one parsed `.ast` file, counting everything in it."""
        for node in json_objects(tree):
            directive = node.get("Directive")
            if isinstance(directive, dict):
                self.count_directive(directive)
            diagnostics = node.get("diagnostics")
            if isinstance(diagnostics, list):
                self.count_diagnostics(diagnostics)

    def count_directive(self, directive: dict[str, Any]) -> None:
        """Count one `Directive` node, if it is of a kind the report tallies."""
        if "Unknown" in directive:
            self.unknown_directives[directive["Unknown"].get("name", "unnamed")] += 1
        elif "Malformed" in directive:
            # A directive this build *does* implement, whose content it had to
            # refuse. Counted separately from the unsupported tally above,
            # which it used to inflate: "not implemented" and "implemented, and
            # this document got it wrong" are different numbers, and only the
            # first is a gap in coverage.
            self.malformed_directives[directive["Malformed"].get("name", "unnamed")] += 1
        elif "Toctree" in directive:
            # Which `.. toctree::` options the corpus actually exercises. This
            # used to count options the parser *ignored*, back when it recorded
            # them in an `ignored_options` list and honoured none of them.
            # Every option is honoured now and that field is gone, so counting
            # usage is what keeps the number honest — a zero here would
            # otherwise look like success while only meaning the field had been
            # removed. An option the parser does not know is not counted here
            # at all; it surfaces as a `directive.toctree-unknown-option` entry
            # under parser diagnostics.
            options = directive["Toctree"].get("options", {})
            for name in TOCTREE_VALUE_OPTIONS:
                if options.get(name) is not None:
                    self.toctree_options_used[name] += 1
            for flag in options.get("flags", []):
                # `TitlesOnly` -> `titlesonly`, as an author writes it.
                self.toctree_options_used[flag.lower()] += 1

    def count_diagnostics(self, diagnostics: list[object]) -> None:
        """Count a document's diagnostics by code."""
        for diag in diagnostics:
            # Aggregate by the diagnostic's own code. This used to key off the
            # message's prefix as a stand-in; the code is the thing that prefix
            # was approximating, and unlike a message it never varies with the
            # offending text.
            if isinstance(diag, dict):
                code = diag.get("code", "unknown")
            else:
                # An .ast written before diagnostics were structured.
                code = str(diag).split(":")[0]
            self.parser_diagnostics[code] += 1


def analyze_results(*, build_succeeded: bool = False) -> None:
    """Tally the corpus' `.ast` files and warnings into the report and the summary."""
    print("Analyzing AST output for unsupported constructs...")

    ast_files = list(TARGET_DIR.glob("bazel-bin/Doc/**/*.ast"))

    if not ast_files:
        print("No .ast files found in bazel-bin. Did the build succeed?")
        return

    tally = AstTally()
    for ast_file in ast_files:
        try:
            tally.count(json.loads(ast_file.read_text()))
        except (OSError, ValueError) as e:
            print(f"Failed to parse {ast_file}: {e}")

    # The full listing is far too long for the terminal, so write it to a file
    # and show only a compact summary on screen.
    result_path = Path("benchmark_result.txt")
    with result_path.open("w") as out:
        print("=== rinx benchmark: full report ===", file=out)
        write_frequency_summary(out, "Unsupported Directives Summary", tally.unknown_directives)
        write_frequency_summary(out, "Malformed Directives Summary", tally.malformed_directives)
        write_frequency_summary(
            out,
            "Toctree Options Exercised Summary",
            tally.toctree_options_used,
            key_prefix=":",
            key_suffix=":",
        )
        write_frequency_summary(out, "Parser Diagnostics Summary", tally.parser_diagnostics)
        domain = report_domain_warnings(
            TARGET_DIR / "bazel-bin", WHITELIST_PATH, build_succeeded=build_succeeded, out=out
        )

    print_benchmark_summary(result_path, tally, domain)


def print_benchmark_summary(result_path: Path, tally: AstTally, domain: DomainSummary) -> None:
    """Print the compact, terminal-friendly summary (counts only) and point at the report."""
    line = benchmark_common.summary_line
    unknown = tally.unknown_directives
    malformed = tally.malformed_directives
    toctree_opts = tally.toctree_options_used

    print("\n=== Benchmark Summary ===")
    line("Unsupported directives:", len(unknown), unknown.total())
    line("Malformed directives:", len(malformed), malformed.total())
    line("Toctree options exercised:", len(toctree_opts), toctree_opts.total())
    line("Parser diagnostics:", len(tally.parser_diagnostics))
    line("Unresolved domain refs:", *domain.unresolved)
    line("Ambiguous domain refs:", *domain.ambiguous)
    line("Domain type mismatches:", *domain.mismatch)
    benchmark_common.print_whitelist_summary(domain.whitelist)
    print(f"\nFull report written to: {result_path}")


def main() -> None:
    """Clone CPython, build its documentation with rinx, and report on the result."""
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--clean", action="store_true", help="Clear the Bazel cache before building"
    )
    parser.add_argument(
        "--python-version",
        default=PYTHON_VERSION,
        help=(
            "CPython release to benchmark against. Selects both the cloned "
            f"release tag and the pinned toolchain (default: {PYTHON_VERSION}). "
            "Must exist as a v<version> tag in CPython and as an entry in "
            "rules_python's TOOL_VERSIONS."
        ),
    )
    args = parser.parse_args()

    # If run via `bazel run`, change to the workspace root
    workspace_dir = os.environ.get("BUILD_WORKSPACE_DIRECTORY", str(Path.cwd()))
    os.chdir(workspace_dir)

    # Ensure we run from the workspace root
    if not Path("WORKSPACE").exists() and not Path("MODULE.bazel").exists():
        print("Please run this script from the root of the rinx workspace.")
        return

    print(f"Benchmarking against CPython {args.python_version}.")
    clone_repo(args.python_version)
    generate_bazel_project(workspace_dir, args.python_version)
    build_succeeded = run_benchmark(clean=args.clean)
    analyze_results(build_succeeded=build_succeeded)


if __name__ == "__main__":
    main()
