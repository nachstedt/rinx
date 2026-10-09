import argparse
import json
import subprocess
from collections.abc import Sequence
from pathlib import Path

import pytest

import benchmark_common
import publish_pages
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


class TestRinxModuleBazel:
    def test_depends_on_the_local_rinx_checkout(self) -> None:
        # Given / When
        text = benchmark_common.rinx_module_bazel("bench", "/src/rinx")

        # Then
        assert 'module(name = "bench")' in text
        assert 'bazel_dep(name = "rinx", version = "0.0.0")' in text
        assert 'path = "/src/rinx"' in text

    def test_every_workspace_declares_rinx_identically(self) -> None:
        # Given / When — only the module's own name may differ, or the
        # workspaces' action keys, and with them the shared compile, part ways
        first = benchmark_common.rinx_module_bazel("a", "/src/rinx")
        second = benchmark_common.rinx_module_bazel("b", "/src/rinx")

        # Then
        assert first.replace('"a"', '"b"', 1) == second


class TestWriteAssetsAlias:
    def test_aliases_the_default_stylesheet(self, tmp_path: Path) -> None:
        # Given / When
        benchmark_common.write_assets_alias(tmp_path)

        # Then
        build = (tmp_path / "assets" / "BUILD.bazel").read_text()
        assert 'name = "default.css"' in build
        assert 'actual = "@rinx//:assets/default.css"' in build


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


class TestSummaryRows:
    def test_a_count_row_names_distinct_and_occurrences(self) -> None:
        # Given / When
        row = benchmark_common.count_row("Unsupported directives:", 3, 17)

        # Then
        assert row.label == "Unsupported directives:"
        assert row.value == "3 distinct (17 occurrences)"

    def test_a_count_row_without_occurrences_names_only_the_distinct(self) -> None:
        # Given / When
        row = benchmark_common.count_row("Parser diagnostics:", 4)

        # Then
        assert row.value == "4 distinct"

    def test_whitelist_rows_mention_stale_entries_only_when_there_are_some(self) -> None:
        # Given
        clean = _whitelist_summary(stale_count=0, stale_pruned=False)
        stale = _whitelist_summary(stale_count=2, stale_pruned=True)

        # When
        clean_rows = benchmark_common.whitelist_rows(clean)
        stale_rows = benchmark_common.whitelist_rows(stale)

        # Then
        assert [row.label for row in clean_rows] == ["Suppressed by whitelist:"]
        assert clean_rows[0].value == "5 occurrences (3 entries)"
        assert stale_rows[1] == benchmark_common.SummaryRow(
            "Stale whitelist entries:", "2 (removed)"
        )

    def test_timing_rows_show_both_builds(self) -> None:
        # Given
        result = benchmark_common.BuildResult(
            succeeded=True, warmup_seconds=61.25, corpus_seconds=15.61
        )

        # When
        rows = benchmark_common.timing_rows(result)

        # Then
        assert rows == [
            benchmark_common.SummaryRow("Warm-up build (untimed):", "61.2 s"),
            benchmark_common.SummaryRow("Corpus build:", "15.6 s"),
        ]

    def test_timing_rows_say_when_the_corpus_never_built(self) -> None:
        # Given a warm-up build that failed, so the corpus build never ran
        result = benchmark_common.BuildResult(
            succeeded=False, warmup_seconds=3.0, corpus_seconds=None
        )

        # When
        rows = benchmark_common.timing_rows(result)

        # Then
        assert rows[1] == benchmark_common.SummaryRow("Corpus build:", "not run")

    def test_a_failed_corpus_build_is_marked_as_failed(self) -> None:
        # Given
        result = benchmark_common.BuildResult(
            succeeded=False, warmup_seconds=3.0, corpus_seconds=9.0
        )

        # When
        rows = benchmark_common.timing_rows(result)

        # Then
        assert rows[1] == benchmark_common.SummaryRow("Corpus build:", "failed after 9.0 s")


class TestRenderSummary:
    ROWS = (
        benchmark_common.SummaryRow("Corpus build:", "15.6 s"),
        benchmark_common.SummaryRow("Parser diagnostics:", "4 distinct"),
    )

    def test_the_terminal_summary_aligns_values_under_a_heading(self) -> None:
        # Given / When
        text = benchmark_common.render_terminal_summary("Benchmark Summary", self.ROWS)

        # Then
        assert text.splitlines() == [
            "=== Benchmark Summary ===",
            f"  {'Corpus build:':<28}15.6 s",
            f"  {'Parser diagnostics:':<28}4 distinct",
        ]

    def test_the_markdown_summary_is_a_two_column_table(self) -> None:
        # Given / When
        text = benchmark_common.render_markdown_summary("CPython", self.ROWS)

        # Then
        assert text.splitlines() == [
            "### CPython",
            "",
            "| | |",
            "|---|---|",
            "| Corpus build | 15.6 s |",
            "| Parser diagnostics | 4 distinct |",
            "",
        ]

    def test_markdown_table_cells_cannot_break_out_of_their_column(self) -> None:
        # Given a label holding the table's own separator
        rows = [benchmark_common.SummaryRow("a|b:", "c|d")]

        # When
        text = benchmark_common.render_markdown_summary("T", rows)

        # Then
        assert "| a\\|b | c\\|d |" in text


class TestExportSite:
    def test_copies_the_site_and_the_report_and_measures_the_site(self, tmp_path: Path) -> None:
        # Given a site as Bazel leaves it: nested, and read-only
        site = tmp_path / "site"
        (site / "Doc").mkdir(parents=True)
        (site / "Doc" / "index.html").write_text("<p>hi</p>")
        (site / "objects.inv").write_bytes(b"inv")
        for path in (site / "Doc" / "index.html", site / "objects.inv"):
            path.chmod(0o444)
        report = tmp_path / "result.txt"
        report.write_text("report")
        dest = tmp_path / "out"

        # When
        size = benchmark_common.export_site(site, report, "Doc/index.html", dest)

        # Then
        assert (dest / "Doc" / "index.html").read_text() == "<p>hi</p>"
        assert (dest / "objects.inv").read_bytes() == b"inv"
        assert (dest / benchmark_common.REPORT_NAME).read_text() == "report"
        assert (dest / benchmark_common.ENTRY_NAME).read_text() == "Doc/index.html\n"
        assert size == len("<p>hi</p>") + len("inv")

    def test_the_copy_is_writable_so_a_later_run_can_replace_it(self, tmp_path: Path) -> None:
        # Given a read-only site
        site = tmp_path / "site"
        site.mkdir()
        (site / "index.html").write_text("x")
        (site / "index.html").chmod(0o444)
        report = tmp_path / "result.txt"
        report.write_text("report")
        dest = tmp_path / "out"

        # When
        benchmark_common.export_site(site, report, "index.html", dest)
        (dest / "index.html").write_text("replaced")

        # Then
        assert (dest / "index.html").read_text() == "replaced"

    def test_a_missing_report_is_skipped(self, tmp_path: Path) -> None:
        # Given a site whose analysis wrote no report
        site = tmp_path / "site"
        site.mkdir()
        (site / "index.html").write_text("x")

        # When
        benchmark_common.export_site(site, tmp_path / "absent.txt", "index.html", tmp_path / "out")

        # Then
        assert not (tmp_path / "out" / benchmark_common.REPORT_NAME).exists()


class TestAppendMarkdownSummary:
    def test_appends_rather_than_replacing(self, tmp_path: Path) -> None:
        # Given a step summary another step already wrote to
        summary = tmp_path / "summary.md"
        summary.write_text("earlier\n")

        # When
        benchmark_common.append_markdown_summary(
            summary, "CPython", [benchmark_common.SummaryRow("Corpus build:", "1.0 s")]
        )

        # Then
        text = summary.read_text()
        assert text.startswith("earlier\n")
        assert "### CPython" in text


class TestExitStatus:
    def test_a_corpus_that_built_exits_zero(self) -> None:
        # Given / When / Then
        assert benchmark_common.exit_status(build_succeeded=True) == 0

    def test_a_corpus_that_did_not_build_exits_non_zero(self) -> None:
        # Given / When / Then
        assert benchmark_common.exit_status(build_succeeded=False) == 1

    def test_a_corpus_that_crossed_a_bar_exits_non_zero(self) -> None:
        # Given / When / Then
        assert benchmark_common.exit_status(build_succeeded=True, failures=["too many"]) == 1


def _whitelist_summary(
    *, stale_count: int, stale_pruned: bool
) -> benchmark_common.WhitelistSummary:
    return benchmark_common.WhitelistSummary(
        per_kind={},
        suppressed=5,
        whitelist_size=3,
        stale_count=stale_count,
        stale_pruned=stale_pruned,
    )


class TestFinishRun:
    def _site(self, tmp_path: Path) -> tuple[Path, Path]:
        site = tmp_path / "site"
        site.mkdir()
        (site / "index.html").write_text("x" * 2_000_000)
        report = tmp_path / "result.txt"
        report.write_text("report")
        return site, report

    def test_a_built_corpus_is_exported_and_summarized(
        self, tmp_path: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        # Given
        site, report = self._site(tmp_path)
        result = benchmark_common.BuildResult(
            succeeded=True, warmup_seconds=1.0, corpus_seconds=2.0
        )
        summary = tmp_path / "summary.md"

        # When
        status = benchmark_common.finish_run(
            benchmark_common.RunOutcome(
                "CPython",
                result,
                [benchmark_common.count_row("Parser diagnostics:", 4)],
                site_dir=site,
                entry="index.html",
                report=report,
            ),
            benchmark_common.RunOutputs(site_out=tmp_path / "out", summary_markdown=summary),
        )

        # Then
        assert status == 0
        assert (tmp_path / "out" / "index.html").is_file()
        markdown = summary.read_text()
        assert "| Site size | 2.0 MB |" in markdown
        assert "| Parser diagnostics | 4 distinct |" in markdown
        assert "=== CPython ===" in capsys.readouterr().out

    def test_a_failed_corpus_exports_nothing_and_fails(self, tmp_path: Path) -> None:
        # Given a site left over from an earlier run
        site, report = self._site(tmp_path)
        result = benchmark_common.BuildResult(
            succeeded=False, warmup_seconds=1.0, corpus_seconds=2.0
        )

        # When
        status = benchmark_common.finish_run(
            benchmark_common.RunOutcome(
                "CPython", result, [], site_dir=site, entry="index.html", report=report
            ),
            benchmark_common.RunOutputs(site_out=tmp_path / "out"),
        )

        # Then
        assert status == 1
        assert not (tmp_path / "out").exists()

    def test_a_crossed_bar_fails_a_built_corpus_and_says_why(
        self, tmp_path: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        # Given
        site, report = self._site(tmp_path)
        result = benchmark_common.BuildResult(
            succeeded=True, warmup_seconds=1.0, corpus_seconds=2.0
        )

        # When
        status = benchmark_common.finish_run(
            benchmark_common.RunOutcome(
                "CPython",
                result,
                [],
                site_dir=site,
                entry="index.html",
                report=report,
                failures=["the language server shows 9 warnings, more than 5"],
            ),
            benchmark_common.RunOutputs(),
        )

        # Then
        assert status == 1
        assert "FAILED: the language server shows 9 warnings" in capsys.readouterr().out


class TestRunOutputs:
    def test_reads_the_two_output_flags(self) -> None:
        # Given
        parser = argparse.ArgumentParser()
        benchmark_common.add_output_arguments(parser)

        # When
        args = parser.parse_args(["--site-out", "out", "--summary-markdown", "s.md"])
        outputs = benchmark_common.RunOutputs.from_args(args)

        # Then
        assert outputs == benchmark_common.RunOutputs(Path("out"), Path("s.md"))

    def test_asks_for_nothing_by_default(self) -> None:
        # Given
        parser = argparse.ArgumentParser()
        benchmark_common.add_output_arguments(parser)

        # When
        outputs = benchmark_common.RunOutputs.from_args(parser.parse_args([]))

        # Then
        assert outputs == benchmark_common.RunOutputs()


class TestExportedNames:
    def test_the_publisher_reads_the_names_the_export_writes(self) -> None:
        # Given / When / Then — two scripts, one contract
        assert publish_pages.BENCHMARK_REPORT == benchmark_common.REPORT_NAME
        assert publish_pages.BENCHMARK_ENTRY == benchmark_common.ENTRY_NAME
