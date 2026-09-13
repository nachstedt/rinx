import argparse
import json
import os
import subprocess
import tempfile
from pathlib import Path

import benchmark_common
from benchmark_common import (
    WARMUP_PACKAGE,
    collect_warning_sidecars,
    write_frequency_summary,
)

TARGET_DIR = Path(tempfile.gettempdir()) / "rusty_sphinx_benchmark_cpython"
REPO_URL = "https://github.com/python/cpython.git"

# The one place the benchmark's Python version is decided.
#
# It pins BOTH halves of the corpus so they cannot drift apart: the CPython
# release tag whose `Doc/` we build (`v3.14.2`), and the interpreter the
# generated workspace resolves (`python.toolchain(python_version = ...)`).
# Previously the script cloned `main` while the toolchain came from
# rusty-sphinx's own MODULE.bazel, so the documentation described a
# development version whose APIs the interpreter did not have — every doctest
# using a newly added API failed for a reason that had nothing to do with
# rusty-sphinx. Examples seen: `re.Pattern.prefixmatch`,
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

def clone_repo(python_version: str):
    """Shallow-clones the CPython release tag matching `python_version`.

    Deliberately a tag rather than `main`: the documentation has to describe the
    same interpreter the generated workspace runs, or doctests fail on APIs that
    simply do not exist yet.
    """
    tag = f"v{python_version}"
    if not benchmark_common.clone_repo(REPO_URL, tag, TARGET_DIR):
        raise SystemExit(
            f"Failed to clone CPython at tag {tag!r}.\n"
            f"Check that the tag exists (`git ls-remote --tags {REPO_URL} '{tag}'`) "
            f"and that rules_python ships an interpreter for {python_version} — "
            f"both are required, see PYTHON_VERSION in this file."
        )


def generate_bazel_project(workspace_root: str, python_version: str):
    print("Generating artificial Bazel project in /tmp (MODULE.bazel, BUILD.bazel, config, and template)...")

    # Create MODULE.bazel
    #
    # The Python toolchain is pinned to the exact patch release whose `Doc/`
    # tree this workspace was cloned from, so an interpreter and the
    # documentation describing it can never disagree. `is_default = True` is
    # honoured only for the root module, and this generated workspace *is* the
    # root when the benchmark builds — so this pin wins over the 3.x toolchain
    # rusty-sphinx registers for its own tests.
    module_bazel = f"""module(name = "cpython_docs_bench")

bazel_dep(name = "rusty_sphinx", version = "0.0.0")
local_path_override(
    module_name = "rusty_sphinx",
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
    actual = "@rusty_sphinx//:assets/default.css",
    visibility = ["//visibility:public"],
)
""")

    doc_dir = TARGET_DIR / "Doc"
    
    # Create config
    (doc_dir / "rusty_sphinx.toml").write_text('project = "CPython Benchmark"\n')
    
    # Create template — use the project's default template for a nicely formatted output
    default_template_path = Path(workspace_root) / "templates" / "default.html"
    template_content = default_template_path.read_text()
    (doc_dir / "custom_template.html").write_text(template_content)
    
    # Create BUILD.bazel in Doc
    build_bazel = """load("@rusty_sphinx//:defs.bzl", "rusty_sphinx_library", "rusty_sphinx_site")

rusty_sphinx_library(
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

rusty_sphinx_site(
    name = "site",
    config = "rusty_sphinx.toml",
    template = "custom_template.html",
    css = "//assets:default.css",
    deps = [":cpython_docs"],
)
"""
    (doc_dir / "BUILD.bazel").write_text(build_bazel)

    benchmark_common.generate_warmup_package(TARGET_DIR, workspace_root)


def run_benchmark(clean: bool = False):
    """Builds rusty-sphinx first, then times the CPython documentation build.

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
        subprocess.run(["bazel", "clean"], cwd=str(TARGET_DIR), capture_output=True)
    else:
        benchmark_common.discard_stale_corpus_outputs(TARGET_DIR, CORPUS_PACKAGE)

    print("Building rusty-sphinx and its toolchains (not timed as doc build)...")
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
        print(
            f"(Excludes {deps_duration:.2f} seconds spent building rusty-sphinx itself.)"
        )

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

def format_warning(entry, include_kind=True):
    """One-line human-readable rendering of a warning or whitelist entry.

    Surfaces the object-type info ("missed type"): a mismatch shows
    'requested -> resolved'; a broken reference (nothing resolved) shows just
    the requested type it was looking for; an ambiguous reference also lists
    the qualified names it matched, since choosing between them is the fix.
    Pass include_kind=False when the surrounding section already states the
    kind (avoids a redundant token)."""
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


def report_domain_warnings(bazel_bin_dir, whitelist_path, build_succeeded, out):
    """Diff the emitted domain-object warnings against the whitelist, write the
    detailed listing to the `out` file handle, and auto-prune stale whitelist
    entries when it is safe to do so. Returns a small dict of counts for the
    terminal summary.

    The three sections are separate because they call for different fixes:
    "couldn't resolve at all", "matched several objects" and "resolved to the
    wrong object type"."""
    actual, file_count = collect_warning_sidecars(bazel_bin_dir)
    summary = benchmark_common.report_warnings(
        actual,
        file_count,
        whitelist_path,
        build_succeeded,
        out,
        sections=[
            ("Unresolved Domain-Object References", "domain_object_reference"),
            ("Ambiguous Domain-Object References", "ambiguous_domain_object_reference"),
            ("Domain-Object Type Mismatches", "object_type_mismatch"),
        ],
        format_entry=format_warning,
    )

    per_kind = summary.pop("per_kind")
    unresolved, ambiguous, mismatch = (
        per_kind["domain_object_reference"],
        per_kind["ambiguous_domain_object_reference"],
        per_kind["object_type_mismatch"],
    )
    return {
        "unresolved_distinct": unresolved[0],
        "unresolved_occurrences": unresolved[1],
        "ambiguous_distinct": ambiguous[0],
        "ambiguous_occurrences": ambiguous[1],
        "mismatch_distinct": mismatch[0],
        "mismatch_occurrences": mismatch[1],
        **summary,
    }


def analyze_results(build_succeeded=False):
    print("Analyzing AST output for unsupported constructs...")
    
    ast_files = list(TARGET_DIR.glob("bazel-bin/Doc/**/*.ast"))

    if not ast_files:
        print("No .ast files found in bazel-bin. Did the build succeed?")
        return
        
    unknown_directives = {}
    malformed_directives = {}
    toctree_options_used = {}
    parser_diagnostics = {}
    
    def traverse(node):
        if isinstance(node, dict):
            # Check if this node is an Unknown directive
            if "Directive" in node:
                directive = node["Directive"]
                if isinstance(directive, dict):
                    if "Unknown" in directive:
                        name = directive["Unknown"].get("name", "unnamed")
                        unknown_directives[name] = unknown_directives.get(name, 0) + 1
                    elif "Malformed" in directive:
                        # A directive this build *does* implement, whose
                        # content it had to refuse. Counted separately from
                        # the unsupported tally above, which it used to
                        # inflate: "not implemented" and "implemented, and
                        # this document got it wrong" are different numbers,
                        # and only the first is a gap in coverage.
                        name = directive["Malformed"].get("name", "unnamed")
                        malformed_directives[name] = malformed_directives.get(name, 0) + 1
                    elif "Toctree" in directive:
                        # Which `.. toctree::` options the corpus actually
                        # exercises. This used to count options the parser
                        # *ignored*, back when it recorded them in an
                        # `ignored_options` list and honoured none of them.
                        # Every option is honoured now and that field is gone,
                        # so counting usage is what keeps the number honest —
                        # a zero here would otherwise look like success while
                        # only meaning the field had been removed. An option
                        # the parser does not know is not counted here at all;
                        # it surfaces as a `directive.toctree-unknown-option`
                        # entry under parser diagnostics.
                        options = directive["Toctree"].get("options", {})
                        for name in ("maxdepth", "numbered", "caption", "name"):
                            if options.get(name) is not None:
                                toctree_options_used[name] = (
                                    toctree_options_used.get(name, 0) + 1
                                )
                        for flag in options.get("flags", []):
                            # `TitlesOnly` -> `titlesonly`, as an author writes it.
                            key = flag.lower()
                            toctree_options_used[key] = (
                                toctree_options_used.get(key, 0) + 1
                            )
            
            # Check for document diagnostics
            if "diagnostics" in node and isinstance(node["diagnostics"], list):
                for diag in node["diagnostics"]:
                    # Aggregate by the diagnostic's own code. This used to key
                    # off the message's prefix as a stand-in; the code is the
                    # thing that prefix was approximating, and unlike a message
                    # it never varies with the offending text.
                    if isinstance(diag, dict):
                        code = diag.get("code", "unknown")
                    else:
                        # An .ast written before diagnostics were structured.
                        code = str(diag).split(":")[0]
                    parser_diagnostics[code] = parser_diagnostics.get(code, 0) + 1

            # Recursively traverse all values
            for v in node.values():
                traverse(v)
        elif isinstance(node, list):
            for item in node:
                traverse(item)
                
    for ast_file in ast_files:
        try:
            with open(ast_file, "r") as f:
                data = json.load(f)
                traverse(data)
        except Exception as e:
            print(f"Failed to parse {ast_file}: {e}")
            
    # The full listing is far too long for the terminal, so write it to a file
    # and show only a compact summary on screen.
    result_path = Path("benchmark_result.txt")
    with open(result_path, "w") as out:
        print("=== rusty-sphinx benchmark: full report ===", file=out)
        write_frequency_summary(out, "Unsupported Directives Summary", unknown_directives)
        write_frequency_summary(out, "Malformed Directives Summary", malformed_directives)
        write_frequency_summary(
            out,
            "Toctree Options Exercised Summary",
            toctree_options_used,
            key_prefix=":",
            key_suffix=":",
        )
        write_frequency_summary(out, "Parser Diagnostics Summary", parser_diagnostics)
        domain = report_domain_warnings(
            TARGET_DIR / "bazel-bin", WHITELIST_PATH, build_succeeded, out
        )

    print_benchmark_summary(
        result_path,
        unknown_directives,
        malformed_directives,
        toctree_options_used,
        parser_diagnostics,
        domain,
    )


def print_benchmark_summary(
    result_path, unknown, malformed, toctree_opts, diagnostics, domain
):
    """Print the compact, terminal-friendly summary (counts only) and point at
    the full report file."""
    line = benchmark_common.summary_line

    print("\n=== Benchmark Summary ===")
    line("Unsupported directives:", len(unknown), sum(unknown.values()))
    line("Malformed directives:", len(malformed), sum(malformed.values()))
    line("Toctree options exercised:", len(toctree_opts), sum(toctree_opts.values()))
    line("Parser diagnostics:", len(diagnostics))
    line(
        "Unresolved domain refs:",
        domain["unresolved_distinct"],
        domain["unresolved_occurrences"],
    )
    line(
        "Ambiguous domain refs:",
        domain["ambiguous_distinct"],
        domain["ambiguous_occurrences"],
    )
    line(
        "Domain type mismatches:",
        domain["mismatch_distinct"],
        domain["mismatch_occurrences"],
    )
    benchmark_common.print_whitelist_summary(domain)
    print(f"\nFull report written to: {result_path}")

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--clean", action="store_true", help="Clear the Bazel cache before building")
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
    workspace_dir = os.environ.get("BUILD_WORKSPACE_DIRECTORY", os.getcwd())
    os.chdir(workspace_dir)
        
    # Ensure we run from the workspace root
    if not Path("WORKSPACE").exists() and not Path("MODULE.bazel").exists():
        print("Please run this script from the root of the rusty-sphinx workspace.")
        return
        
    print(f"Benchmarking against CPython {args.python_version}.")
    clone_repo(args.python_version)
    generate_bazel_project(workspace_dir, args.python_version)
    build_succeeded = run_benchmark(clean=args.clean)
    analyze_results(build_succeeded=build_succeeded)

if __name__ == "__main__":
    main()
