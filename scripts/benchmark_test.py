import io
import json
import tempfile
import unittest
from pathlib import Path

import benchmark
import benchmark_common


class FormatWarningTest(unittest.TestCase):
    def test_mismatch_shows_requested_and_resolved(self):
        entry = {
            "doc_path": "d",
            "kind": "object_type_mismatch",
            "target": "Fault",
            "requested_type": "py:exception",
            "resolved_type": "py:class",
        }
        self.assertEqual(
            benchmark.format_warning(entry),
            "d: object_type_mismatch 'Fault' (py:exception -> py:class)",
        )

    def test_broken_reference_shows_requested_missed_type(self):
        # Given a broken reference: requested type present, nothing resolved
        entry = {
            "doc_path": "d",
            "kind": "domain_object_reference",
            "target": "os.PathLike",
            "requested_type": "py:function",
        }
        # Then the missed type is surfaced
        self.assertEqual(
            benchmark.format_warning(entry),
            "d: domain_object_reference 'os.PathLike' (referenced as py:function)",
        )

    def test_ambiguous_reference_lists_its_candidates(self):
        # Given an ambiguous reference: the fix is choosing between the
        # matches, so they belong in the one-line rendering
        entry = {
            "doc_path": "d",
            "kind": "ambiguous_domain_object_reference",
            "target": "close",
            "requested_type": "py:method",
            "candidates": ["tarfile.tarfile.close", "zipfile.zipfile.close"],
        }
        self.assertEqual(
            benchmark.format_warning(entry),
            "d: ambiguous_domain_object_reference 'close' (referenced as py:method)"
            " matching tarfile.tarfile.close, zipfile.zipfile.close",
        )

    def test_entry_without_types_has_no_detail(self):
        entry = {"doc_path": "d", "kind": "domain_object_reference", "target": "x"}
        self.assertEqual(
            benchmark.format_warning(entry), "d: domain_object_reference 'x'"
        )


class ReportDomainWarningsTest(unittest.TestCase):
    def _write_sidecar(self, root, doc_path, warnings):
        (root / f"{doc_path}.warnings.json").write_text(
            json.dumps({"doc_path": doc_path, "warnings": warnings})
        )

    def test_writes_detail_to_handle_and_returns_summary_counts(self):
        # Given a corpus with two unresolved refs (one twice), one ambiguous
        # ref and one mismatch
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._write_sidecar(
                root,
                "a",
                [
                    {"kind": "domain_object_reference", "target": "x", "requested_type": "py:class"},
                    {"kind": "domain_object_reference", "target": "x", "requested_type": "py:class"},
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
            self.assertEqual(summary["unresolved_distinct"], 2)
            self.assertEqual(summary["unresolved_occurrences"], 3)
            self.assertEqual(summary["ambiguous_distinct"], 1)
            self.assertEqual(summary["ambiguous_occurrences"], 1)
            self.assertEqual(summary["mismatch_distinct"], 1)
            self.assertEqual(summary["mismatch_occurrences"], 1)
            self.assertEqual(summary["suppressed"], 0)

            # And the detail went to the handle, split into one section per
            # kind — each calls for a different fix
            text = out.getvalue()
            self.assertIn("Unresolved Domain-Object References (2):", text)
            self.assertIn("Ambiguous Domain-Object References (1):", text)
            self.assertIn("Domain-Object Type Mismatches (1):", text)

    def test_stale_entries_are_pruned_only_on_a_trustworthy_build(self):
        # Given a whitelist entry that matches no actual warning, and an empty
        # corpus (no sidecars) — pruning must be skipped
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
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
            self.assertEqual(summary["stale_count"], 1)
            self.assertFalse(summary["stale_pruned"])
            self.assertEqual(len(benchmark_common.load_whitelist(wl)), 1)


if __name__ == "__main__":
    unittest.main()
