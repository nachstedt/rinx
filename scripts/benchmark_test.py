import io
import json
import subprocess
from pathlib import Path

import pytest

import benchmark
import benchmark_common
from benchmark_common import WarningEntry


class TestFormatWarning:
    def test_mismatch_shows_requested_and_resolved(self) -> None:
        entry: WarningEntry = {
            "doc_path": "d",
            "kind": "object_type_mismatch",
            "target": "Fault",
            "requested_type": "py:exception",
            "resolved_type": "py:class",
        }
        assert (
            benchmark.format_warning(entry)
            == "d: object_type_mismatch 'Fault' (py:exception -> py:class)"
        )

    def test_broken_reference_shows_requested_missed_type(self) -> None:
        # Given a broken reference: requested type present, nothing resolved
        entry: WarningEntry = {
            "doc_path": "d",
            "kind": "domain_object_reference",
            "target": "os.PathLike",
            "requested_type": "py:function",
        }
        # Then the missed type is surfaced
        assert (
            benchmark.format_warning(entry)
            == "d: domain_object_reference 'os.PathLike' (referenced as py:function)"
        )

    def test_ambiguous_reference_lists_its_candidates(self) -> None:
        # Given an ambiguous reference: the fix is choosing between the
        # matches, so they belong in the one-line rendering
        entry: WarningEntry = {
            "doc_path": "d",
            "kind": "ambiguous_domain_object_reference",
            "target": "close",
            "requested_type": "py:method",
            "candidates": ["tarfile.tarfile.close", "zipfile.zipfile.close"],
        }
        assert (
            benchmark.format_warning(entry)
            == "d: ambiguous_domain_object_reference 'close' (referenced as py:method)"
            " matching tarfile.tarfile.close, zipfile.zipfile.close"
        )

    def test_entry_without_types_has_no_detail(self) -> None:
        entry: WarningEntry = {"doc_path": "d", "kind": "domain_object_reference", "target": "x"}
        assert benchmark.format_warning(entry) == "d: domain_object_reference 'x'"


class TestReportDomainWarnings:
    @staticmethod
    def _write_sidecar(root: Path, doc_path: str, warnings: list[WarningEntry]) -> None:
        (root / f"{doc_path}.warnings.json").write_text(
            json.dumps({"doc_path": doc_path, "warnings": warnings})
        )

    def test_writes_detail_to_handle_and_returns_summary_counts(self, tmp_path: Path) -> None:
        # Given a corpus with two unresolved refs (one twice), one ambiguous
        # ref and one mismatch
        root = tmp_path
        self._write_sidecar(
            root,
            "a",
            [
                {
                    "kind": "domain_object_reference",
                    "target": "x",
                    "requested_type": "py:class",
                },
                {
                    "kind": "domain_object_reference",
                    "target": "x",
                    "requested_type": "py:class",
                },
                {"kind": "domain_object_reference", "target": "y", "requested_type": "py:func"},
                {
                    "kind": "ambiguous_domain_object_reference",
                    "target": "close",
                    "requested_type": "py:method",
                    "candidates": ["a.B.close", "c.D.close"],
                },
                {
                    "kind": "object_type_mismatch",
                    "target": "Z",
                    "requested_type": "py:class",
                    "resolved_type": "py:exception",
                },
            ],
        )
        out = io.StringIO()

        # When
        summary = benchmark.report_domain_warnings(
            root, root / "wl.json", build_succeeded=True, out=out
        )

        # Then — counts reflect distinct keys and total occurrences
        assert summary.unresolved == (2, 3)
        assert summary.ambiguous == (1, 1)
        assert summary.mismatch == (1, 1)
        assert summary.whitelist.suppressed == 0

        # And the detail went to the handle, split into one section per
        # kind — each calls for a different fix
        text = out.getvalue()
        assert "Unresolved Domain-Object References (2):" in text
        assert "Ambiguous Domain-Object References (1):" in text
        assert "Domain-Object Type Mismatches (1):" in text

    def test_stale_entries_are_pruned_only_on_a_trustworthy_build(self, tmp_path: Path) -> None:
        # Given a whitelist entry that matches no actual warning, and an empty
        # corpus (no sidecars) — pruning must be skipped
        root = tmp_path
        wl = root / "wl.json"
        benchmark_common.write_whitelist(
            wl,
            [{"doc_path": "d", "kind": "domain_object_reference", "target": "gone"}],
        )

        # When the build succeeded but there were zero sidecars
        summary = benchmark.report_domain_warnings(
            root, wl, build_succeeded=True, out=io.StringIO()
        )

        # Then the stale entry is reported but left untouched on disk
        assert summary.whitelist.stale_count == 1
        assert not summary.whitelist.stale_pruned
        assert len(benchmark_common.load_whitelist(wl)) == 1


class TestAstTally:
    def test_directives_are_counted_by_kind_and_name(self) -> None:
        # Given one unknown, one malformed and one toctree directive, nested
        # the way a document holds them
        tree = {
            "nodes": [
                {"Directive": {"Unknown": {"name": "automodule"}}},
                {"Section": {"children": [{"Directive": {"Malformed": {"name": "csv-table"}}}]}},
                {
                    "Directive": {
                        "Toctree": {
                            "options": {"maxdepth": 2, "caption": None, "flags": ["TitlesOnly"]}
                        }
                    }
                },
            ]
        }
        tally = benchmark.AstTally()

        # When
        tally.count(tree)

        # Then — only options actually written count, spelled as authors do
        assert tally.unknown_directives == {"automodule": 1}
        assert tally.malformed_directives == {"csv-table": 1}
        assert tally.toctree_options_used == {"maxdepth": 1, "titlesonly": 1}

    def test_diagnostics_are_counted_by_code(self) -> None:
        # Given a structured diagnostic and one from an older, string-only .ast
        tree = {
            "diagnostics": [
                {"code": "directive.unknown", "message": "..."},
                "role.unknown: :foo:",
            ]
        }
        tally = benchmark.AstTally()

        # When
        tally.count(tree)

        # Then
        assert tally.parser_diagnostics == {"directive.unknown": 1, "role.unknown": 1}


class TestRenderCorpusBuildFile:
    def test_the_files_pages_download_are_declared(self) -> None:
        # Given / When
        text = benchmark.render_corpus_build_file()

        # Then CPython's example files are declared — without them the site's
        # validate_assets fails as `download.undeclared` and no page renders
        downloads = text.split("downloads = ")[1]
        assert downloads.startswith("glob(")
        assert '"includes/**"' in downloads

    def test_rst_fragments_are_not_declared_as_downloads(self) -> None:
        # Given / When
        text = benchmark.render_corpus_build_file()

        # Then the fragments under includes/ stay sources, not downloads
        downloads = text.split("downloads = ")[1].split("),")[0]
        assert '"**/*.rst"' in downloads.split("exclude = ")[1]

    def test_images_are_still_declared(self) -> None:
        # Given / When
        text = benchmark.render_corpus_build_file()

        # Then
        assert "images = glob(" in text

    def test_the_site_writes_the_python_module_index(self) -> None:
        # Given / When
        text = benchmark.render_corpus_build_file()

        # Then CPython's ~300 modules get the index Sphinx writes for them
        site = text.split("rinx_site(")[1]
        assert 'domain_indices = ["py-modindex"],' in site


class TestSummaryRows:
    def test_every_count_is_listed_then_the_whitelist(self) -> None:
        # Given a tally and a domain summary
        tally = benchmark.AstTally()
        tally.unknown_directives.update({"automodule": 3, "autoclass": 1})
        tally.parser_diagnostics.update({"role.unknown": 2})
        whitelist = benchmark_common.WhitelistSummary(
            per_kind={}, suppressed=7, whitelist_size=4, stale_count=0, stale_pruned=False
        )
        count = benchmark_common.SectionCount
        domain = benchmark.DomainSummary(
            unresolved=count(5, 9), ambiguous=count(0, 0), mismatch=count(1, 2), whitelist=whitelist
        )

        # When
        rows = benchmark.summary_rows(tally, domain)

        # Then
        values = {row.label: row.value for row in rows}
        assert values["Unsupported directives:"] == "2 distinct (4 occurrences)"
        assert values["Parser diagnostics:"] == "1 distinct"
        assert values["Unresolved domain refs:"] == "5 distinct (9 occurrences)"
        assert rows[-1].label == "Suppressed by whitelist:"


def _check(*severities_and_codes: tuple[int, str]) -> dict[str, object]:
    """A `rinx lsp --check` result holding one document with these diagnostics."""
    return {
        "documents": 1,
        "published": {
            "file:///Doc/index.rst": [
                {"severity": severity, "code": code} for severity, code in severities_and_codes
            ]
        },
    }


class TestCountLspCheck:
    def test_counts_by_severity_and_warnings_by_code(self) -> None:
        # Given
        check = _check(
            (2, "directive.unknown"),
            (2, "directive.unknown"),
            (2, "link.broken-ref"),
            (3, "directive.unknown"),
            (4, "link.broken-object"),
        )

        # When
        counts = benchmark.count_lsp_check(check)

        # Then
        assert counts.documents == 1
        assert counts.warnings == 3
        assert counts.by_severity == {"warnings": 3, "information": 1, "hints": 1}
        assert counts.warnings_by_code == {"directive.unknown": 2, "link.broken-ref": 1}

    def test_a_diagnostic_without_code_or_severity_is_counted_as_the_protocol_reads_it(
        self,
    ) -> None:
        # Given — the include summaries carry no code
        check = {"documents": 1, "published": {"u": [{"severity": 2}, {}]}}

        # When
        counts = benchmark.count_lsp_check(check)

        # Then
        assert counts.warnings_by_code == {"(no code)": 1}
        assert counts.by_severity["errors"] == 1


class TestLspRows:
    def test_names_each_severity_shown_and_the_bar(self) -> None:
        # Given
        counts = benchmark.count_lsp_check(_check((2, "a"), (4, "b")))

        # When
        rows = benchmark.lsp_rows(counts)

        # Then
        assert rows == [
            benchmark_common.SummaryRow(
                "Language server:",
                f"1 warnings, 1 hints over 1 documents "
                f"(at most {benchmark.MAX_LSP_WARNINGS} warnings)",
            )
        ]

    def test_says_when_the_server_did_not_run(self) -> None:
        # Given / When / Then
        assert benchmark.lsp_rows(None) == [
            benchmark_common.SummaryRow("Language server:", "not run")
        ]


class TestLspFailures:
    def test_up_to_the_limit_is_no_failure(self) -> None:
        # Given
        counts = benchmark.count_lsp_check(_check((2, "a"), (2, "a")))

        # When / Then
        assert benchmark.lsp_failures(counts, limit=2) == []

    def test_more_than_the_limit_fails_and_says_by_how_much(self) -> None:
        # Given
        counts = benchmark.count_lsp_check(_check((2, "a"), (2, "a"), (2, "a")))

        # When
        failures = benchmark.lsp_failures(counts, limit=2)

        # Then
        assert len(failures) == 1
        assert "shows 3 warnings" in failures[0]
        assert "the 2 MAX_LSP_WARNINGS" in failures[0]

    def test_a_server_that_did_not_run_is_no_failure_of_its_own(self) -> None:
        # Given / When / Then
        assert benchmark.lsp_failures(None, limit=0) == []


class TestFirstExecutable:
    def test_finds_the_first_executable_file_among_the_paths(self, tmp_path: Path) -> None:
        # Given
        (tmp_path / "data").write_text("")
        binary = tmp_path / "bin" / "rinx"
        binary.parent.mkdir()
        binary.write_text("")
        binary.chmod(0o755)

        # When
        found = benchmark.first_executable(["", "missing", "data", "bin/rinx\n"], tmp_path)

        # Then
        assert found == binary

    def test_finds_nothing_when_nothing_is_executable(self, tmp_path: Path) -> None:
        # Given / When / Then
        assert benchmark.first_executable(["missing"], tmp_path) is None


class TestRunLspCheck:
    def test_counts_what_the_binary_printed(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given a binary that prints one warning
        binary = tmp_path / "rinx"
        monkeypatch.setattr(benchmark, "find_server_binary", lambda: binary)
        calls: list[list[str]] = []

        def run(command: list[str], **_: object) -> subprocess.CompletedProcess[str]:
            calls.append(command)
            return subprocess.CompletedProcess(command, 0, json.dumps(_check((2, "a"))), "")

        monkeypatch.setattr(benchmark.subprocess, "run", run)

        # When
        counts = benchmark.run_lsp_check()

        # Then
        assert counts is not None
        assert counts.warnings == 1
        assert calls[0][:3] == [str(binary), "lsp", "--check"]
        assert calls[0][3].endswith("Doc")

    def test_is_skipped_without_a_binary(self, monkeypatch: pytest.MonkeyPatch) -> None:
        # Given
        monkeypatch.setattr(benchmark, "find_server_binary", lambda: None)

        # When / Then
        assert benchmark.run_lsp_check() is None

    def test_is_none_when_the_check_fails(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given
        monkeypatch.setattr(benchmark, "find_server_binary", lambda: tmp_path / "rinx")
        monkeypatch.setattr(
            benchmark.subprocess,
            "run",
            lambda command, **_: subprocess.CompletedProcess(command, 1, "", "boom"),
        )

        # When / Then
        assert benchmark.run_lsp_check() is None


class TestFindServerBinary:
    def test_asks_cquery_for_the_exec_built_binary_and_finds_its_file(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given a target directory holding the file cquery names
        monkeypatch.setattr(benchmark, "TARGET_DIR", tmp_path)
        binary = tmp_path / "bazel-out" / "exec" / "rinx"
        binary.parent.mkdir(parents=True)
        binary.write_text("")
        binary.chmod(0o755)
        queries: list[list[str]] = []

        def run(command: list[str], **_: object) -> subprocess.CompletedProcess[str]:
            queries.append(command)
            return subprocess.CompletedProcess(command, 0, "bazel-out/exec/rinx\n", "")

        monkeypatch.setattr(benchmark.subprocess, "run", run)

        # When
        found = benchmark.find_server_binary()

        # Then
        assert found == binary
        assert queries[0][:2] == ["bazel", "cquery"]
        assert any("deps(//bench_warmup:site)" in part for part in queries[0])
