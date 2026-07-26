import argparse
import subprocess
import time
import json
import os
from collections import Counter
from pathlib import Path
import shutil
import tempfile

TARGET_DIR = Path(tempfile.gettempdir()) / "rusty_sphinx_benchmark_cpython"
REPO_URL = "https://github.com/python/cpython.git"
# Hand-authored list of domain-object warnings we accept, relative to the
# workspace root (main() chdirs there via BUILD_WORKSPACE_DIRECTORY).
WHITELIST_PATH = Path("scripts/domain_warnings_whitelist.json")

def clone_repo():
    print(f"Target directory: {TARGET_DIR}")
    if TARGET_DIR.exists():
        print("Directory already exists. Removing it for a fresh clone...")
        shutil.rmtree(TARGET_DIR, ignore_errors=False)
        if TARGET_DIR.exists():
            # This could happen on some filesystems due to latency or locks
            print("Warning: Directory still exists after rmtree, attempting one more time...")
            shutil.rmtree(TARGET_DIR, ignore_errors=True)
    
    print(f"Cloning {REPO_URL}...")
    TARGET_DIR.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "clone", "--depth", "1", REPO_URL, str(TARGET_DIR)], check=True)

def generate_bazel_project(workspace_root: str):
    print("Generating artificial Bazel project in /tmp (MODULE.bazel, BUILD.bazel, config, and template)...")
    
    # Create MODULE.bazel
    module_bazel = f"""module(name = "cpython_docs_bench")

bazel_dep(name = "rusty_sphinx", version = "0.0.0")
local_path_override(
    module_name = "rusty_sphinx",
    path = "{workspace_root}",
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

def run_benchmark(clean: bool = False):
    if clean:
        # Run bazel clean to avoid caching from previous runs
        print("Cleaning Bazel cache...")
        subprocess.run(["bazel", "clean"], cwd=str(TARGET_DIR), capture_output=True)

    print("Running Bazel build...")

    start_time = time.time()
    
    # Run bazel build inside the decoupled workspace
    result = subprocess.run(
        ["bazel", "build", "-c", "opt", "--host_compilation_mode=opt", "--spawn_strategy=local", "--profile=profile.json.gz", "//Doc:site"],
        cwd=str(TARGET_DIR),
        capture_output=True,
        text=True
    )
    
    end_time = time.time()
    duration = end_time - start_time

    # Persist the inner build's captured output — it's swallowed by
    # capture_output above, so without this it's invisible on a successful
    # build. Written on success and failure alike, so it's always inspectable.
    build_log = TARGET_DIR / "bazel_build.log"
    build_log.write_text(result.stdout + result.stderr)

    if result.returncode != 0:
        print("Bazel build failed!")
        print(result.stderr)
    else:
        print(f"Bazel build succeeded in {duration:.2f} seconds.")

    print(f"\nBazel build output was captured to: {build_log}")

    # Inform the user where the HTML is
    html_out = TARGET_DIR / "bazel-bin/Doc/site_site_out"
    print(f"HTML output is located at: {html_out}/")

    # Inform the user where the Bazel profile is
    profile_out = TARGET_DIR / "profile.json.gz"
    print(f"Bazel profile is located at: {profile_out}")
    print("You can view it by dropping the file into https://ui.perfetto.dev/ or chrome://tracing")

    return result.returncode == 0

# ── Domain-object warning whitelist helpers (pure, unit-tested) ───────────────

def collect_domain_warnings(bazel_bin_dir):
    """Load every per-document .warnings.json sidecar under bazel_bin_dir and
    flatten them into a single list of warning entries, each augmented with the
    doc_path of the report it came from.

    Returns (entries, file_count) — file_count is the number of sidecar files
    found, used by the pruning safety guard (an empty corpus must not prune)."""
    warning_files = sorted(Path(bazel_bin_dir).glob("**/*.warnings.json"))
    entries = []
    for warning_file in warning_files:
        try:
            with open(warning_file, "r") as f:
                report = json.load(f)
        except Exception as e:
            print(f"Failed to parse {warning_file}: {e}")
            continue
        doc_path = report.get("doc_path", str(warning_file))
        for warning in report.get("warnings", []):
            entry = dict(warning)
            entry["doc_path"] = doc_path
            entries.append(entry)
    return entries, len(warning_files)


def warning_key(entry):
    """The identity of a warning for whitelist matching: (doc_path, kind,
    target). The requested/resolved object types are informational payload,
    deliberately not part of the key."""
    return (entry.get("doc_path"), entry.get("kind"), entry.get("target"))


def load_whitelist(path):
    """Return the list of whitelist entries, or [] if the file doesn't exist."""
    path = Path(path)
    if not path.exists():
        return []
    with open(path, "r") as f:
        data = json.load(f)
    return data.get("entries", [])


def write_whitelist(path, entries):
    """Rewrite the whitelist file with the given entries."""
    with open(path, "w") as f:
        json.dump({"entries": entries}, f, indent=2)
        f.write("\n")


def partition_warnings(actual, whitelist_entries):
    """Split actual warnings into those not covered by the whitelist ('new',
    de-duplicated by key) and count how many actual occurrences the whitelist
    suppressed."""
    whitelist_keys = {warning_key(w) for w in whitelist_entries}
    new_by_key = {}
    suppressed = 0
    for entry in actual:
        key = warning_key(entry)
        if key in whitelist_keys:
            suppressed += 1
        elif key not in new_by_key:
            new_by_key[key] = entry
    return list(new_by_key.values()), suppressed


def prune_whitelist(whitelist_entries, actual):
    """Partition the whitelist into entries still justified by an actual
    warning ('kept') and entries that no longer match anything ('removed')."""
    actual_keys = {warning_key(a) for a in actual}
    kept = [w for w in whitelist_entries if warning_key(w) in actual_keys]
    removed = [w for w in whitelist_entries if warning_key(w) not in actual_keys]
    return kept, removed


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

    Pruning is destructive (it deletes hand-written comments), so it only runs
    when the warning data is trustworthy: the build succeeded AND at least one
    sidecar was found."""
    actual, file_count = collect_domain_warnings(bazel_bin_dir)
    whitelist_entries = load_whitelist(whitelist_path)

    new_warnings, suppressed = partition_warnings(actual, whitelist_entries)
    occurrences = Counter(warning_key(a) for a in actual)

    # Split the new warnings by kind so "couldn't resolve at all", "matched
    # several objects" and "resolved to the wrong object type" read as
    # distinct, separately-scannable lists rather than being interleaved by
    # frequency — they call for different fixes. The leading number is each
    # warning's occurrence count across the corpus.
    def write_section(title, kind):
        section = [e for e in new_warnings if e.get("kind") == kind]
        header = f"{title} ({len(section)}):"
        print(f"\n{header}", file=out)
        print("-" * len(header), file=out)
        if not section:
            print("None found.", file=out)
        else:
            for entry in sorted(
                section, key=lambda e: occurrences[warning_key(e)], reverse=True
            ):
                print(
                    f"{occurrences[warning_key(entry)]:5d}  "
                    f"{format_warning(entry, include_kind=False)}",
                    file=out,
                )
        occ = sum(occurrences[warning_key(e)] for e in section)
        return len(section), occ

    unresolved_distinct, unresolved_occ = write_section(
        "Unresolved Domain-Object References", "domain_object_reference"
    )
    ambiguous_distinct, ambiguous_occ = write_section(
        "Ambiguous Domain-Object References", "ambiguous_domain_object_reference"
    )
    mismatch_distinct, mismatch_occ = write_section(
        "Domain-Object Type Mismatches", "object_type_mismatch"
    )
    print(
        f"\n{suppressed} warning occurrence(s) suppressed by whitelist "
        f"({len(whitelist_entries)} entr(y/ies)).",
        file=out,
    )

    kept, removed = prune_whitelist(whitelist_entries, actual)
    pruned = False
    if removed:
        can_prune = build_succeeded and file_count > 0
        print("\nStale Whitelist Entries:", file=out)
        print("------------------------", file=out)
        for entry in removed:
            comment = entry.get("comment", "")
            suffix = f"  # {comment}" if comment else ""
            print(f"- {format_warning(entry)}{suffix}", file=out)
        if can_prune:
            write_whitelist(whitelist_path, kept)
            pruned = True
            print(
                f"\nRemoved {len(removed)} stale entr(y/ies) from {whitelist_path}.",
                file=out,
            )
        else:
            reason = (
                "build did not succeed"
                if not build_succeeded
                else "no .warnings.json sidecars were found"
            )
            print(
                f"\nSkipped auto-pruning ({reason}); "
                f"{len(removed)} entr(y/ies) left untouched.",
                file=out,
            )

    return {
        "unresolved_distinct": unresolved_distinct,
        "unresolved_occurrences": unresolved_occ,
        "ambiguous_distinct": ambiguous_distinct,
        "ambiguous_occurrences": ambiguous_occ,
        "mismatch_distinct": mismatch_distinct,
        "mismatch_occurrences": mismatch_occ,
        "suppressed": suppressed,
        "whitelist_size": len(whitelist_entries),
        "stale_count": len(removed),
        "stale_pruned": pruned,
    }


def analyze_results(build_succeeded=False):
    print("Analyzing AST output for unsupported constructs...")
    
    ast_files = list(TARGET_DIR.glob("bazel-bin/Doc/**/*.ast"))

    if not ast_files:
        print("No .ast files found in bazel-bin. Did the build succeed?")
        return
        
    unknown_directives = {}
    ignored_toctree_options = {}
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
                    elif "Toctree" in directive:
                        # Extract ignored options from Toctree
                        options = directive["Toctree"].get("ignored_options", [])
                        for opt in options:
                            # Normalize option name for aggregation (e.g., :caption: text -> :caption:)
                            opt_name = opt.split(":")[1] if ":" in opt else opt
                            ignored_toctree_options[opt_name] = ignored_toctree_options.get(opt_name, 0) + 1
            
            # Check for document diagnostics
            if "diagnostics" in node and isinstance(node["diagnostics"], list):
                for diag in node["diagnostics"]:
                    # Aggregate by message prefix to avoid explosion if unique values are present
                    msg = diag.split(":")[0]
                    parser_diagnostics[msg] = parser_diagnostics.get(msg, 0) + 1

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
        write_frequency_summary(
            out,
            "Ignored Toctree Options Summary",
            ignored_toctree_options,
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
        ignored_toctree_options,
        parser_diagnostics,
        domain,
    )


def write_frequency_summary(out, title, counts, key_prefix="", key_suffix=""):
    """Write one `name: count` frequency table (descending) to the `out` file
    handle — used for the detailed report file."""
    print(f"\n{title}:", file=out)
    print("-" * (len(title) + 1), file=out)
    if not counts:
        print("None found.", file=out)
        return
    for name, count in sorted(counts.items(), key=lambda x: x[1], reverse=True):
        print(f"{key_prefix}{name}{key_suffix}: {count}", file=out)


def print_benchmark_summary(result_path, unknown, toctree_opts, diagnostics, domain):
    """Print the compact, terminal-friendly summary (counts only) and point at
    the full report file."""
    def line(label, distinct, occurrences=None):
        occ = f"  ({occurrences} occurrences)" if occurrences is not None else ""
        print(f"  {label:<28}{distinct:6d} distinct{occ}")

    print("\n=== Benchmark Summary ===")
    line("Unsupported directives:", len(unknown), sum(unknown.values()))
    line("Ignored toctree options:", len(toctree_opts))
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
    print(
        f"  {'Suppressed by whitelist:':<28}"
        f"{domain['suppressed']:6d} occurrences ({domain['whitelist_size']} entries)"
    )
    if domain["stale_count"]:
        action = "removed" if domain["stale_pruned"] else "left untouched"
        print(f"  {'Stale whitelist entries:':<28}{domain['stale_count']:6d} ({action})")
    print(f"\nFull report written to: {result_path}")

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--clean", action="store_true", help="Clear the Bazel cache before building")
    args = parser.parse_args()

    # If run via `bazel run`, change to the workspace root
    workspace_dir = os.environ.get("BUILD_WORKSPACE_DIRECTORY", os.getcwd())
    os.chdir(workspace_dir)
        
    # Ensure we run from the workspace root
    if not Path("WORKSPACE").exists() and not Path("MODULE.bazel").exists():
        print("Please run this script from the root of the rusty-sphinx workspace.")
        return
        
    clone_repo()
    generate_bazel_project(workspace_dir)
    build_succeeded = run_benchmark(clean=args.clean)
    analyze_results(build_succeeded=build_succeeded)

if __name__ == "__main__":
    main()
