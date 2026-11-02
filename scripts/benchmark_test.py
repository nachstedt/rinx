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


class TestCountConstructs:
    def test_each_list_is_summed_over_the_projects(self) -> None:
        # Given two projects' tallies, each construct with its places
        def project(count: int) -> dict[str, object]:
            empty: dict[str, object] = {}
            return {
                "root": "/Doc",
                "documents": 1,
                "constructs": {
                    "unknown_directives": {"automodule": {"count": count, "examples": []}},
                    "malformed_directives": {"csv-table": {"count": 1, "examples": []}},
                    "unsupported_options": empty,
                    "unknown_roles": {"gh": {"count": 2, "examples": []}},
                    "unsupported_roles": empty,
                    "ignored_roles": {"issue": {"count": 3, "examples": []}},
                    "toctree_options": {"maxdepth": 1, "titlesonly": 1},
                    "diagnostics": {"directive.unknown": count},
                },
            }

        # When
        counts = benchmark.count_constructs({"analysis": [project(1), project(2)]})

        # Then
        assert counts.unknown_directives == {"automodule": 3}
        assert counts.malformed_directives == {"csv-table": 2}
        assert counts.unknown_roles == {"gh": 4}
        assert counts.ignored_roles == {"issue": 6}
        assert counts.toctree_options == {"maxdepth": 2, "titlesonly": 2}
        assert counts.parser_diagnostics == {"directive.unknown": 3}

    def test_a_check_without_an_analysis_counts_nothing(self) -> None:
        # Given / When
        counts = benchmark.count_constructs({})

        # Then
        assert counts == benchmark.ConstructCounts()


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

    def test_the_library_names_the_parser_config(self) -> None:
        # Given / When
        text = benchmark.render_corpus_build_file()

        # Then
        library = text.split("rinx_library(")[1].split("rinx_site(")[0]
        assert 'parse_config = "rinx_parser.toml",' in library


class TestRenderParserConfig:
    def test_ignores_the_roles_of_cpythons_own_extensions(self) -> None:
        # Given / When
        text = benchmark.render_parser_config()

        # Then
        assert 'ignored_roles = ["issue", "gh", "opcode", "pdbcmd", "monitoring-event"]\n' in text


class TestSummaryRows:
    def test_every_count_is_listed_then_the_whitelist(self) -> None:
        # Given a tally and a domain summary
        constructs = benchmark.ConstructCounts()
        constructs.unknown_directives.update({"automodule": 3, "autoclass": 1})
        constructs.unknown_roles.update({"gh": 5})
        constructs.parser_diagnostics.update({"interpreted.unknown-role": 2})
        whitelist = benchmark_common.WhitelistSummary(
            per_kind={}, suppressed=7, whitelist_size=4, stale_count=0, stale_pruned=False
        )
        count = benchmark_common.SectionCount
        domain = benchmark.DomainSummary(
            unresolved=count(5, 9), ambiguous=count(0, 0), mismatch=count(1, 2), whitelist=whitelist
        )

        # When
        rows = benchmark.summary_rows(constructs, domain)

        # Then
        values = {row.label: row.value for row in rows}
        assert values["Unsupported directives:"] == "2 distinct (4 occurrences)"
        assert values["Unknown roles:"] == "1 distinct (5 occurrences)"
        assert values["Parser diagnostics:"] == "1 distinct"
        assert values["Unresolved domain refs:"] == "5 distinct (9 occurrences)"
        assert rows[-1].label == "Suppressed by whitelist:"

    def test_no_tally_lists_only_the_domain_counts(self) -> None:
        # Given a domain summary, and no language server run
        whitelist = benchmark_common.WhitelistSummary(
            per_kind={}, suppressed=0, whitelist_size=0, stale_count=0, stale_pruned=False
        )
        count = benchmark_common.SectionCount
        domain = benchmark.DomainSummary(
            unresolved=count(0, 0), ambiguous=count(0, 0), mismatch=count(0, 0), whitelist=whitelist
        )

        # When
        rows = benchmark.summary_rows(None, domain)

        # Then
        assert "Unsupported directives:" not in {row.label for row in rows}


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


class TestRunLspCheck:
    def test_counts_what_the_binary_printed(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given a binary that prints one warning
        binary = tmp_path / "rinx"
        monkeypatch.setattr(benchmark_common, "find_rinx_binary", lambda _: binary)
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
        monkeypatch.setattr(benchmark_common, "find_rinx_binary", lambda _: None)

        # When / Then
        assert benchmark.run_lsp_check() is None

    def test_is_none_when_the_check_fails(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given
        monkeypatch.setattr(benchmark_common, "find_rinx_binary", lambda _: tmp_path / "rinx")
        monkeypatch.setattr(
            benchmark.subprocess,
            "run",
            lambda command, **_: subprocess.CompletedProcess(command, 1, "", "boom"),
        )

        # When / Then
        assert benchmark.run_lsp_check() is None
