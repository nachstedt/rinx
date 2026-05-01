import subprocess
import time
import json
import os
from pathlib import Path
import shutil
import tempfile

TARGET_DIR = Path(tempfile.gettempdir()) / "rusty_sphinx_benchmark_cpython"
REPO_URL = "https://github.com/python/cpython.git"

def clone_repo():
    print(f"Cloning {REPO_URL} into {TARGET_DIR}...")
    if TARGET_DIR.exists():
        print("Directory already exists. Removing it for a fresh clone.")
        shutil.rmtree(TARGET_DIR)

    
    TARGET_DIR.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "clone", "--depth", "1", REPO_URL, str(TARGET_DIR)], check=True)

def generate_bazel_project(workspace_root: str):
    print("Generating artificial Bazel project in /tmp (MODULE.bazel, BUILD.bazel, config, and template)...")
    
    # Create MODULE.bazel
    module_bazel = f"""module(name = "cpython_docs_bench")

bazel_dep(name = "rusty_sphinx", version = "0.0.0")
local_path_override(
    module_name = "rusty_sphinx",
    path = "{workspace_root}/bazel",
)

bazel_dep(name = "rusty_sphinx_workspace", version = "0.0.0")
local_path_override(
    module_name = "rusty_sphinx_workspace",
    path = "{workspace_root}",
)
"""
    (TARGET_DIR / "MODULE.bazel").write_text(module_bazel)
    
    # Create root BUILD.bazel for aliases
    root_build_bazel = """alias(
    name = "rusty_sphinx_worker",
    actual = "@rusty_sphinx_workspace//:rusty_sphinx_worker",
    visibility = ["//visibility:public"],
)

alias(
    name = "plantuml_tool",
    actual = "@rusty_sphinx_workspace//:plantuml_tool",
    visibility = ["//visibility:public"],
)
"""
    (TARGET_DIR / "BUILD.bazel").write_text(root_build_bazel)

    # Create assets/BUILD.bazel for the CSS dependency
    assets_dir = TARGET_DIR / "assets"
    assets_dir.mkdir(exist_ok=True)
    (assets_dir / "BUILD.bazel").write_text("""alias(
    name = "default.css",
    actual = "@rusty_sphinx_workspace//:assets/default.css",
    visibility = ["//visibility:public"],
)
""")

    doc_dir = TARGET_DIR / "Doc"
    
    # Create config
    (doc_dir / "rusty_sphinx.toml").write_text('project = "CPython Benchmark"\n')
    
    # Create template
    (doc_dir / "custom_template.html").write_text('<!DOCTYPE html><html><body><h1>CPython Benchmark</h1>{{ body }}</body></html>\n')
    
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

def run_benchmark():
    print("Running Bazel build...")
    start_time = time.time()
    
    # Run bazel build inside the decoupled workspace
    result = subprocess.run(
        ["bazel", "build", "--keep_going", "//Doc:site"],
        cwd=str(TARGET_DIR),
        capture_output=True,
        text=True
    )
    
    end_time = time.time()
    duration = end_time - start_time
    
    if result.returncode != 0:
        print("Bazel build failed (or finished with errors under --keep_going)!")
        print(result.stderr)
    else:
        print(f"Bazel build succeeded in {duration:.2f} seconds.")
    
    # Inform the user where the HTML is
    html_out = TARGET_DIR / "bazel-bin/Doc/site_site_out"
    print(f"\nHTML output is located at: {html_out}/")

def analyze_results():
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
            
    print("\nUnsupported Directives Summary:")
    print("-------------------------------")
    if not unknown_directives:
        print("None found.")
    else:
        for name, count in sorted(unknown_directives.items(), key=lambda x: x[1], reverse=True):
            print(f"{name}: {count}")

    print("\nIgnored Toctree Options Summary:")
    print("--------------------------------")
    if not ignored_toctree_options:
        print("None found.")
    else:
        for name, count in sorted(ignored_toctree_options.items(), key=lambda x: x[1], reverse=True):
            print(f":{name}: {count}")

    print("\nParser Diagnostics Summary:")
    print("---------------------------")
    if not parser_diagnostics:
        print("None found.")
    else:
        for msg, count in sorted(parser_diagnostics.items(), key=lambda x: x[1], reverse=True):
            print(f"{msg}: {count}")

def main():
    # If run via `bazel run`, change to the workspace root
    workspace_dir = os.environ.get("BUILD_WORKSPACE_DIRECTORY", os.getcwd())
    os.chdir(workspace_dir)
        
    # Ensure we run from the workspace root
    if not Path("WORKSPACE").exists() and not Path("MODULE.bazel").exists():
        print("Please run this script from the root of the rusty-sphinx workspace.")
        return
        
    clone_repo()
    generate_bazel_project(workspace_dir)
    run_benchmark()
    analyze_results()

if __name__ == "__main__":
    main()
