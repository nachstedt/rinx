import io
import json
import subprocess
from collections import Counter
from pathlib import Path

import pytest

import benchmark_common
import benchmark_entities
from benchmark_common import WarningEntry


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


class TestCopyOutsideSources:
    def test_the_file_is_placed_where_the_parser_will_look_for_it(self, tmp_path: Path) -> None:
        # Given a source outside the source root and the path the parser clamps
        # its reference to
        workspace = tmp_path / "docs"
        workspace.mkdir()
        source = tmp_path / "pharaoh.toml"
        source.write_text("[tool]\n")

        # When
        copies = benchmark_entities.copy_outside_sources(workspace, [("pharaoh.toml", source)])

        # Then the corpus itself is untouched; only the disposable generated
        # workspace gains a file, which the migration is told about
        assert (workspace / "pharaoh.toml").read_text() == "[tool]\n"
        assert copies == [workspace / "pharaoh.toml"]

    def test_a_nested_destination_has_its_directories_created(self, tmp_path: Path) -> None:
        # Given a destination inside a directory that does not exist yet
        workspace = tmp_path / "docs"
        workspace.mkdir()
        source = tmp_path / "cars.csv"
        source.write_text("a,b\n")

        # When
        benchmark_entities.copy_outside_sources(workspace, [("data/cars.csv", source)])

        # Then
        assert (workspace / "data" / "cars.csv").read_text() == "a,b\n"


class TestDiscardStaleCorpusOutputs:
    def test_corpus_outputs_go_but_the_warm_toolchain_stays(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        # Given a bazel-bin holding corpus outputs beside the fetched
        # repositories and the generated packages
        bazel_bin = tmp_path / "bazel-bin"
        for name in ("external", *benchmark_entities.GENERATED_PACKAGES, "coffee"):
            (bazel_bin / name).mkdir(parents=True)
        (bazel_bin / "index.ast").parent.mkdir(parents=True, exist_ok=True)
        (bazel_bin / "index.ast").write_text("{}")

        def fake_run(command: list[str], **_kwargs: object) -> subprocess.CompletedProcess[str]:
            return subprocess.CompletedProcess(command, 0, f"{bazel_bin}\n", "")

        monkeypatch.setattr(subprocess, "run", fake_run)

        # When
        benchmark_entities.discard_stale_corpus_outputs(tmp_path)

        # Then the corpus outputs are gone — a document dropped from the
        # corpus must not keep being counted — while the compiled
        # rinx binary under `external` stays warm
        assert not (bazel_bin / "coffee").exists()
        assert not (bazel_bin / "index.ast").exists()
        assert (bazel_bin / "external").exists()
        for name in benchmark_entities.GENERATED_PACKAGES:
            assert (bazel_bin / name).exists()

    def test_a_workspace_bazel_cannot_answer_for_is_left_alone(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        # Given a `bazel info` that fails (no output base yet)
        def fake_run(command: list[str], **_kwargs: object) -> subprocess.CompletedProcess[str]:
            return subprocess.CompletedProcess(command, 1, "", "no such workspace")

        monkeypatch.setattr(subprocess, "run", fake_run)

        # When / Then — it returns rather than raising
        benchmark_entities.discard_stale_corpus_outputs(tmp_path)


class TestParseBuildWarnings:
    def test_a_warning_line_becomes_a_whitelistable_entry(self) -> None:
        # Given the one warning shape the worker emits
        log = (
            "INFO: Analyzed target //docs:site.\n"
            "warning: docs/req.rst:28:4: entity.invalid-attribute-value: "
            '`:approved:`: expected `true` or `false`, found "False"\n'
        )

        # When
        entries = benchmark_entities.parse_build_warnings(log)

        # Then the code is the whitelist `kind` and the message its `target`;
        # line and column are dropped, so text moving above a warning does not
        # churn the whitelist
        assert entries == [
            {
                "doc_path": "docs/req.rst",
                "kind": "entity.invalid-attribute-value",
                "target": '`:approved:`: expected `true` or `false`, found "False"',
            }
        ]

    def test_the_same_warning_from_two_actions_is_counted_once(self) -> None:
        # Given a document that is parsed and rendered in separate actions,
        # each printing the same diagnostic
        log = (
            "warning: docs/a.rst:1:1: entity.unknown-target: `:req:` `R_1`\n"
            "warning: docs/a.rst:1:1: entity.unknown-target: `:req:` `R_1`\n"
        )

        # When
        entries = benchmark_entities.parse_build_warnings(log)

        # Then
        assert len(entries) == 1

    def test_ordinary_bazel_output_is_ignored(self) -> None:
        # Given a log with no diagnostics in it
        log = "INFO: Build completed successfully, 42 total actions\nERROR: nope\n"

        # When
        entries = benchmark_entities.parse_build_warnings(log)

        # Then
        assert entries == []


class TestWarningSections:
    def test_sections_are_built_from_the_codes_seen_most_frequent_first(self) -> None:
        # Given warnings carrying three different codes
        warnings: list[WarningEntry] = [
            {"kind": "entity.unknown-target"},
            {"kind": "entity.unknown-attribute"},
            {"kind": "entity.unknown-attribute"},
        ]

        # When
        sections = benchmark_entities.warning_sections(warnings)

        # Then — derived from the data, so a code added on the Rust side shows
        # up without this file being touched
        assert sections == [
            ("entity.unknown-attribute", "entity.unknown-attribute"),
            ("entity.unknown-target", "entity.unknown-target"),
        ]


class TestFormatEntityWarning:
    def test_the_code_is_dropped_when_the_section_already_states_it(self) -> None:
        # Given one warning
        entry: WarningEntry = {
            "doc_path": "docs/a.rst",
            "kind": "entity.unknown-attribute",
            "target": "`:foo:`",
        }

        # When / Then
        assert (
            benchmark_entities.format_entity_warning(entry)
            == "docs/a.rst: entity.unknown-attribute: `:foo:`"
        )
        assert (
            benchmark_entities.format_entity_warning(entry, include_kind=False)
            == "docs/a.rst: `:foo:`"
        )


class TestTallyAst:
    def test_entities_and_unknown_directives_are_counted_separately(self, tmp_path: Path) -> None:
        # Given an .ast holding one parsed entity and one unrecognized directive
        path = tmp_path / "doc.ast"
        path.write_text(
            json.dumps(
                {
                    "nodes": [
                        {"Directive": {"directive": {"Entity": {"type_name": "req", "id": "R_1"}}}},
                        {"Directive": {"directive": {"Entity": {"type_name": "req", "id": "R_2"}}}},
                        {"Directive": {"directive": {"Unknown": {"name": "needtable"}}}},
                    ]
                }
            )
        )

        # When
        unknown, entities = benchmark_entities.tally_ast([path])

        # Then a type with instances is the positive signal, and a type the
        # schema failed to declare would appear as an unknown directive
        assert entities == {"req": 2}
        assert unknown == {"needtable": 1}

    def test_an_unreadable_ast_does_not_abort_the_analysis(self, tmp_path: Path) -> None:
        # Given a truncated file among the outputs
        path = tmp_path / "broken.ast"
        path.write_text("{ not json")

        # When
        unknown, entities = benchmark_entities.tally_ast([path])

        # Then
        assert (unknown, entities) == ({}, {})


class TestWriteSchemaReport:
    def test_entries_are_grouped_by_category_largest_first(self) -> None:
        # Given a converter report with two categories
        report = [
            ("variants", "field a"),
            ("constraints", "4 rules"),
            ("variants", "field b"),
        ]
        out = io.StringIO()

        # When
        benchmark_entities.write_schema_report(out, report)

        # Then
        text = out.getvalue()
        assert text.index("variants (2)") < text.index("constraints (1)")
        assert "  - field a" in text

    def test_an_empty_report_says_so(self) -> None:
        # Given a configuration that converted completely
        out = io.StringIO()

        # When
        benchmark_entities.write_schema_report(out, [])

        # Then
        assert "None found." in out.getvalue()


class TestSummaryRows:
    def test_only_diagnostic_codes_that_occurred_are_listed(self) -> None:
        # Given one code with findings and one without
        whitelist = benchmark_common.WhitelistSummary(
            per_kind={
                "entity.unknown-target": benchmark_common.SectionCount(2, 3),
                "entity.role-type-mismatch": benchmark_common.SectionCount(0, 0),
            },
            suppressed=1,
            whitelist_size=1,
            stale_count=0,
            stale_pruned=False,
        )

        # When
        rows = benchmark_entities.summary_rows(
            Counter({"req": 4, "spec": 2}),
            Counter(),
            [("Dynamic functions", "copy()")],
            whitelist,
        )

        # Then
        labels = [row.label for row in rows]
        assert labels == [
            "Entity types exercised:",
            "Unsupported directives:",
            "Unconvertible constructs:",
            "entity.unknown-target:",
            "Suppressed by whitelist:",
        ]
        assert rows[0].value == "2 distinct (6 occurrences)"


class TestMigrate:
    def test_copies_the_outside_sources_a_dry_run_names_then_migrates_declaring_them(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given a source outside the corpus, which the dry run names
        workspace = tmp_path / "docs"
        workspace.mkdir()
        outside = tmp_path / "pharaoh.toml"
        outside.write_text("[tool]\n")
        monkeypatch.setattr(benchmark_entities, "corpus_workspace", lambda: workspace)
        calls: list[tuple[list[str], bool]] = []

        def migrate_corpus(
            _: Path, __: Path, extra_args: list[str], *, dry_run: bool = False
        ) -> benchmark_common.Migration:
            calls.append((list(extra_args), dry_run))
            outside_finding: benchmark_common.MigrationFinding = {
                "kind": "outside-source-root",
                "path": str(outside),
                "build_path": "pharaoh.toml",
                "message": "m",
            }
            unconverted: benchmark_common.MigrationFinding = {
                "kind": "needs-unconverted",
                "entry": {"category": "variants", "detail": "field a"},
                "message": "m",
            }
            return benchmark_common.Migration(files=[], findings=[outside_finding, unconverted])

        monkeypatch.setattr(benchmark_common, "migrate_corpus", migrate_corpus)

        (workspace / "BUILD.bazel").write_text("")

        # When
        report = benchmark_entities.migrate(tmp_path / "rinx")

        # Then — the warm-up's placeholder made way for the migration's
        assert not (workspace / "BUILD.bazel").exists()
        assert (workspace / "pharaoh.toml").read_text() == "[tool]\n"
        assert calls == [
            (["--jinja"], True),
            (["--jinja", "--parse-data", str(workspace / "pharaoh.toml")], False),
        ]
        assert report == [("variants", "field a")]

    def test_is_none_when_the_migration_fails(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given
        monkeypatch.setattr(benchmark_entities, "corpus_workspace", lambda: tmp_path)
        monkeypatch.setattr(benchmark_common, "migrate_corpus", lambda *_, **__: None)

        # When / Then
        assert benchmark_entities.migrate(tmp_path / "rinx") is None


class TestRunBenchmark:
    def test_migrates_the_corpus_between_the_warm_up_and_its_build(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given every step recording that it ran
        steps: list[str] = []
        monkeypatch.setattr(
            benchmark_entities, "warm_up", lambda **_: steps.append("warm") or (True, 1.0)
        )
        monkeypatch.setattr(
            benchmark_common, "find_rinx_binary", lambda _: steps.append("find") or tmp_path
        )
        monkeypatch.setattr(
            benchmark_entities,
            "migrate",
            lambda *_: steps.append("migrate") or [("variants", "x")],
        )
        built = benchmark_common.BuildResult(succeeded=True, warmup_seconds=1.0, corpus_seconds=2.0)
        monkeypatch.setattr(
            benchmark_entities, "build_corpus", lambda _: steps.append("build") or built
        )

        # When
        result, report = benchmark_entities.run_benchmark()

        # Then
        assert steps == ["warm", "find", "migrate", "build"]
        assert result == built
        assert report == [("variants", "x")]

    def test_a_failed_migration_fails_without_building_the_corpus(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given
        monkeypatch.setattr(benchmark_entities, "warm_up", lambda **_: (True, 1.0))
        monkeypatch.setattr(benchmark_common, "find_rinx_binary", lambda _: tmp_path)
        monkeypatch.setattr(benchmark_entities, "migrate", lambda *_: None)

        def build(_: float) -> benchmark_common.BuildResult:
            message = "the corpus must not be built"
            raise AssertionError(message)

        monkeypatch.setattr(benchmark_entities, "build_corpus", build)

        # When
        result, report = benchmark_entities.run_benchmark()

        # Then
        assert not result.succeeded
        assert report == []

    def test_a_failed_warm_up_skips_the_rest(self, monkeypatch: pytest.MonkeyPatch) -> None:
        # Given
        monkeypatch.setattr(benchmark_entities, "warm_up", lambda **_: (False, 1.0))

        def find(_: Path) -> Path:
            message = "no binary is looked for"
            raise AssertionError(message)

        monkeypatch.setattr(benchmark_common, "find_rinx_binary", find)

        # When
        result, _ = benchmark_entities.run_benchmark()

        # Then
        assert not result.succeeded
        assert result.warmup_seconds == 1.0
