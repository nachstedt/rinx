"""Compile rinx for the benchmarks, before either of them starts.

Each benchmark already runs an untimed warm-up build in its own generated
workspace, so its *reported* corpus time never includes compiling rinx. But
in CI a step's duration is what a reader sees first, and the first benchmark's
step would otherwise hold the whole compile. This builds the same one-document
warm-up site in a generated workspace of its own, declared exactly as the
corpora's are (`benchmark_common.rinx_module_bazel`), so the binary it
compiles lands in the shared disk cache under the same keys. Both benchmark
steps then take about as long as their site builds.

Run it with `bazel run //scripts:benchmark_warmup`. It only pays off with a
disk or remote cache shared between workspaces, which CI configures; locally,
each benchmark's own output base stays warm between runs anyway.
"""

import os
import shutil
import sys
import tempfile
from pathlib import Path

import benchmark_common
from benchmark_common import WARMUP_PACKAGE

TARGET_DIR = Path(tempfile.gettempdir()) / "rinx_benchmark_warmup"
MODULE_NAME = "rinx_benchmark_warmup"


def generate_warmup_workspace(target_dir: Path, rinx_root: str) -> None:
    """Writes a workspace holding only the warm-up site, replacing any earlier one."""
    if target_dir.exists():
        shutil.rmtree(target_dir)
    target_dir.mkdir(parents=True)
    (target_dir / "MODULE.bazel").write_text(
        benchmark_common.rinx_module_bazel(MODULE_NAME, rinx_root)
    )
    # Empty, but it makes the root a package: rules_rust's crate_universe
    # resolves `@@//:MODULE.bazel` and fails to evaluate without it.
    (target_dir / "BUILD.bazel").write_text("")
    benchmark_common.write_assets_alias(target_dir)
    benchmark_common.generate_warmup_package(target_dir, rinx_root)


def main() -> int:
    """Build the warm-up site, returning a non-zero status when it did not build."""
    rinx_root = os.environ.get("BUILD_WORKSPACE_DIRECTORY", str(Path.cwd()))
    generate_warmup_workspace(TARGET_DIR, rinx_root)

    print("Building rinx and its toolchains for the benchmarks...")
    built, duration = benchmark_common.timed_bazel_build(
        TARGET_DIR, f"//{WARMUP_PACKAGE}:site", "bazel_build.log"
    )
    print(f"Warm-up build {'finished' if built else 'failed'} in {duration:.2f} seconds.")
    return benchmark_common.exit_status(build_succeeded=built)


if __name__ == "__main__":
    sys.exit(main())
