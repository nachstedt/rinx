import json
import subprocess
from collections.abc import Sequence
from pathlib import Path

import pytest

import benchmark_common
from benchmark_common import WarningEntry


class TestCollectWarningSidecars:
    def test_flattens_sidecars_and_folds_in_doc_path(self, tmp_path: Path) -> None:
        # Given two sidecar files under a bazel-bin-like tree
        root = tmp_path
        (root / "a").mkdir()
        (root / "a" / "os.warnings.json").write_text(
            json.dumps(
                {
                    "doc_path": "Doc/library/os",
                    "warnings": [{"kind": "domain_object_reference", "target": "os.PathLike"}],
                }
            )
        )
        (root / "b").mkdir()
        (root / "b" / "xmlrpc.warnings.json").write_text(
            json.dumps(
                {
                    "doc_path": "Doc/library/xmlrpc.client",
                    "warnings": [
                        {
                            "kind": "object_type_mismatch",
                            "target": "Fault",
                            "requested_type": "py:exception",
                            "resolved_type": "py:class",
                        }
                    ],
                }
            )
        )

        # When
        corpus = benchmark_common.collect_warning_sidecars(root)

        # Then
        assert corpus.file_count == 2
        assert len(corpus.entries) == 2
        keys = {benchmark_common.warning_key(e) for e in corpus.entries}
        assert ("Doc/library/os", "domain_object_reference", "os.PathLike") in keys
        assert ("Doc/library/xmlrpc.client", "object_type_mismatch", "Fault") in keys

    def test_empty_tree_yields_no_entries_and_zero_files(self, tmp_path: Path) -> None:
        # Given an empty directory
        # When
        corpus = benchmark_common.collect_warning_sidecars(tmp_path)

        # Then
        assert corpus.entries == []
        assert corpus.file_count == 0


class TestWarningKey:
    def test_key_is_doc_path_kind_target_and_ignores_types(self) -> None:
        # Given two entries differing only in their type payload
        a: WarningEntry = {
            "doc_path": "d",
            "kind": "object_type_mismatch",
            "target": "Fault",
            "requested_type": "py:exception",
            "resolved_type": "py:class",
        }
        b: WarningEntry = {
            "doc_path": "d",
            "kind": "object_type_mismatch",
            "target": "Fault",
            "requested_type": "py:class",
            "resolved_type": "py:class",
        }

        # When / Then — the type fields are not part of the identity
        assert benchmark_common.warning_key(a) == benchmark_common.warning_key(b)
        assert benchmark_common.warning_key(a) == ("d", "object_type_mismatch", "Fault")


class TestPartitionWarnings:
    def test_whitelisted_warnings_are_suppressed_and_unlisted_are_new(self) -> None:
        # Given actual warnings, one of which is whitelisted (and occurs twice)
        actual: list[WarningEntry] = [
            {"doc_path": "d1", "kind": "domain_object_reference", "target": "known"},
            {"doc_path": "d1", "kind": "domain_object_reference", "target": "known"},
            {"doc_path": "d2", "kind": "domain_object_reference", "target": "fresh"},
        ]
        whitelist: list[WarningEntry] = [
            {"doc_path": "d1", "kind": "domain_object_reference", "target": "known"}
        ]

        # When
        new_warnings, suppressed = benchmark_common.partition_warnings(actual, whitelist)

        # Then — both occurrences of 'known' suppressed, 'fresh' is new (once)
        assert suppressed == 2
        assert len(new_warnings) == 1
        assert new_warnings[0]["target"] == "fresh"

    def test_new_warnings_are_deduplicated_by_key(self) -> None:
        # Given the same un-whitelisted warning twice
        actual: list[WarningEntry] = [
            {"doc_path": "d", "kind": "domain_object_reference", "target": "x"},
            {"doc_path": "d", "kind": "domain_object_reference", "target": "x"},
        ]

        # When
        new_warnings, suppressed = benchmark_common.partition_warnings(actual, [])

        # Then
        assert suppressed == 0
        assert len(new_warnings) == 1


class TestPruneWhitelist:
    def test_stale_entries_removed_and_matching_kept_with_comments(self) -> None:
        # Given a whitelist where one entry matches an actual warning and one
        # does not
        whitelist: list[WarningEntry] = [
            {
                "doc_path": "d",
                "kind": "domain_object_reference",
                "target": "still-here",
                "comment": "keep me",
            },
            {
                "doc_path": "d",
                "kind": "domain_object_reference",
                "target": "gone",
                "comment": "drop me",
            },
        ]
        actual: list[WarningEntry] = [
            {"doc_path": "d", "kind": "domain_object_reference", "target": "still-here"}
        ]

        # When
        kept, removed = benchmark_common.prune_whitelist(whitelist, actual)

        # Then
        assert len(kept) == 1
        assert kept[0]["target"] == "still-here"
        assert kept[0]["comment"] == "keep me"  # comment preserved
        assert len(removed) == 1
        assert removed[0]["target"] == "gone"


class TestWhitelistIo:
    def test_write_then_load_round_trips_entries(self, tmp_path: Path) -> None:
        # Given entries and a temp path
        entries: list[WarningEntry] = [
            {
                "doc_path": "d",
                "kind": "object_type_mismatch",
                "target": "Fault",
                "comment": "expected",
            }
        ]
        path = tmp_path / "whitelist.json"

        # When
        benchmark_common.write_whitelist(path, entries)
        loaded = benchmark_common.load_whitelist(path)

        # Then
        assert loaded == entries

    def test_load_missing_file_returns_empty_list(self, tmp_path: Path) -> None:
        # Given a path that doesn't exist
        # When
        loaded = benchmark_common.load_whitelist(tmp_path / "nope.json")

        # Then
        assert loaded == []


class TestGenerateWarmupPackage:
    def test_writes_a_single_document_site_outside_the_doc_glob(self, tmp_path: Path) -> None:
        # Given a workspace root supplying the default template, and a target
        # directory standing in for the generated benchmark workspace
        root = tmp_path
        workspace = root / "workspace"
        (workspace / "templates").mkdir(parents=True)
        (workspace / "templates" / "default.html").write_text("<html>{{ body }}</html>")
        target = root / "corpus"
        target.mkdir()

        # When
        benchmark_common.generate_warmup_package(target, str(workspace))

        # Then — the package sits beside Doc/, so Doc's **/*.rst glob
        # cannot pick its document up
        warmup = target / benchmark_common.WARMUP_PACKAGE
        assert "Doc" not in warmup.relative_to(target).parts

        # And it is a buildable one-document site
        assert list(warmup.glob("*.rst")) == [warmup / "index.rst"]
        build = (warmup / "BUILD.bazel").read_text()
        assert 'srcs = ["index.rst"]' in build
        assert "rinx_site(" in build
        assert 'deps = [":warmup_docs"]' in build

        # And it carries its own config and a copy of the template
        assert "project =" in (warmup / "rinx.toml").read_text()
        assert (warmup / "custom_template.html").read_text() == "<html>{{ body }}</html>"


class TestTimedBazelBuild:
    @staticmethod
    def _run(
        monkeypatch: pytest.MonkeyPatch,
        tmp_path: Path,
        returncode: int,
        extra_flags: Sequence[str] = (),
    ) -> tuple[tuple[bool, float], list[tuple[list[str], str]]]:
        calls: list[tuple[list[str], str]] = []

        def fake_run(
            command: list[str], cwd: str, **_kwargs: object
        ) -> subprocess.CompletedProcess[str]:
            calls.append((command, cwd))
            return subprocess.CompletedProcess(command, returncode, "out\n", "err\n")

        monkeypatch.setattr(subprocess, "run", fake_run)
        result = benchmark_common.timed_bazel_build(
            tmp_path, "//Doc:site", "build.log", extra_flags=extra_flags
        )
        return result, calls

    def test_builds_the_target_in_the_generated_workspace_and_times_it(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        # Given a build that succeeds
        root = tmp_path

        # When
        (succeeded, duration), calls = self._run(
            monkeypatch, root, 0, extra_flags=["--profile=profile.json.gz"]
        )

        # Then
        assert succeeded
        assert duration >= 0.0

        # And the invocation carried the shared config flags, the extra
        # flags and the target, run from the generated workspace
        command, cwd = calls[0]
        assert cwd == str(root)
        assert command[:2] == ["bazel", "build"]
        for flag in benchmark_common.BUILD_CONFIG_FLAGS:
            assert flag in command
        assert "--profile=profile.json.gz" in command
        assert command[-1] == "//Doc:site"

    def test_captured_output_is_written_to_the_log_even_when_the_build_fails(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        # Given a build that fails
        root = tmp_path

        # When
        (succeeded, _duration), _calls = self._run(monkeypatch, root, 1)

        # Then the failure is reported, and both streams are on disk
        assert not succeeded
        assert (root / "build.log").read_text() == "out\nerr\n"


class TestJsonObjects:
    def test_yields_every_object_depth_first_including_the_root(self) -> None:
        # Given a tree nesting objects in both objects and lists
        tree = {"a": [{"b": 1}, 2, [{"c": {"d": 3}}]]}

        # When
        objects = list(benchmark_common.json_objects(tree))

        # Then
        assert objects == [tree, {"b": 1}, {"c": {"d": 3}}, {"d": 3}]

    def test_a_scalar_holds_no_objects(self) -> None:
        # Given / When / Then
        assert list(benchmark_common.json_objects("text")) == []


class TestWhyUntrustworthy:
    def test_a_failed_build_cannot_justify_pruning(self) -> None:
        # Given / When / Then
        reason = benchmark_common._why_untrustworthy(build_succeeded=False, file_count=3)
        assert reason == "build did not succeed"

    def test_a_build_that_wrote_no_sidecars_cannot_justify_pruning(self) -> None:
        # Given / When / Then
        reason = benchmark_common._why_untrustworthy(build_succeeded=True, file_count=0)
        assert reason == "no .warnings.json sidecars were found"

    def test_a_successful_build_with_sidecars_can(self) -> None:
        # Given / When / Then
        assert benchmark_common._why_untrustworthy(build_succeeded=True, file_count=1) is None
