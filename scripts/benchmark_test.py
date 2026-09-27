import io
import json
from pathlib import Path

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
