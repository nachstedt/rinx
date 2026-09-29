from pathlib import Path

import benchmark_common
import benchmark_warmup


def rinx_root(tmp_path: Path) -> Path:
    """A stand-in for the rinx checkout: only the template the warm-up copies."""
    root = tmp_path / "rinx"
    (root / "templates").mkdir(parents=True)
    (root / "templates" / "default.html").write_text("<html>{{ body }}</html>")
    return root


class TestGenerateWarmupWorkspace:
    def test_writes_a_workspace_holding_only_the_warmup_site(self, tmp_path: Path) -> None:
        # Given
        root = rinx_root(tmp_path)
        target = tmp_path / "warmup"

        # When
        benchmark_warmup.generate_warmup_workspace(target, str(root))

        # Then — the MODULE.bazel the corpora use, the stylesheet and the site
        assert (target / "MODULE.bazel").read_text() == benchmark_common.rinx_module_bazel(
            benchmark_warmup.MODULE_NAME, str(root)
        )
        assert (target / "assets" / "BUILD.bazel").is_file()
        # And its root is a package, which rules_rust's crate_universe needs
        assert (target / "BUILD.bazel").is_file()
        assert (target / benchmark_common.WARMUP_PACKAGE / "BUILD.bazel").is_file()

    def test_replaces_a_workspace_left_by_an_earlier_run(self, tmp_path: Path) -> None:
        # Given
        root = rinx_root(tmp_path)
        target = tmp_path / "warmup"
        (target / "stale").mkdir(parents=True)

        # When
        benchmark_warmup.generate_warmup_workspace(target, str(root))

        # Then
        assert not (target / "stale").exists()
