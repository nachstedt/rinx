import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import benchmark


class CollectDomainWarningsTest(unittest.TestCase):
    def test_flattens_sidecars_and_folds_in_doc_path(self):
        # Given two sidecar files under a bazel-bin-like tree
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "a").mkdir()
            (root / "a" / "os.warnings.json").write_text(
                json.dumps(
                    {
                        "doc_path": "Doc/library/os",
                        "warnings": [
                            {"kind": "domain_object_reference", "target": "os.PathLike"}
                        ],
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
            entries, file_count = benchmark.collect_domain_warnings(root)

            # Then
            self.assertEqual(file_count, 2)
            self.assertEqual(len(entries), 2)
            keys = {benchmark.warning_key(e) for e in entries}
            self.assertIn(
                ("Doc/library/os", "domain_object_reference", "os.PathLike"), keys
            )
            self.assertIn(
                ("Doc/library/xmlrpc.client", "object_type_mismatch", "Fault"), keys
            )

    def test_empty_tree_yields_no_entries_and_zero_files(self):
        # Given an empty directory
        with tempfile.TemporaryDirectory() as tmp:
            # When
            entries, file_count = benchmark.collect_domain_warnings(Path(tmp))

            # Then
            self.assertEqual(entries, [])
            self.assertEqual(file_count, 0)


class WarningKeyTest(unittest.TestCase):
    def test_key_is_doc_path_kind_target_and_ignores_types(self):
        # Given two entries differing only in their type payload
        a = {
            "doc_path": "d",
            "kind": "object_type_mismatch",
            "target": "Fault",
            "requested_type": "py:exception",
            "resolved_type": "py:class",
        }
        b = {
            "doc_path": "d",
            "kind": "object_type_mismatch",
            "target": "Fault",
            "requested_type": "py:class",
            "resolved_type": "py:class",
        }

        # When / Then — the type fields are not part of the identity
        self.assertEqual(benchmark.warning_key(a), benchmark.warning_key(b))
        self.assertEqual(
            benchmark.warning_key(a), ("d", "object_type_mismatch", "Fault")
        )


class PartitionWarningsTest(unittest.TestCase):
    def test_whitelisted_warnings_are_suppressed_and_unlisted_are_new(self):
        # Given actual warnings, one of which is whitelisted (and occurs twice)
        actual = [
            {"doc_path": "d1", "kind": "domain_object_reference", "target": "known"},
            {"doc_path": "d1", "kind": "domain_object_reference", "target": "known"},
            {"doc_path": "d2", "kind": "domain_object_reference", "target": "fresh"},
        ]
        whitelist = [
            {"doc_path": "d1", "kind": "domain_object_reference", "target": "known"}
        ]

        # When
        new_warnings, suppressed = benchmark.partition_warnings(actual, whitelist)

        # Then — both occurrences of 'known' suppressed, 'fresh' is new (once)
        self.assertEqual(suppressed, 2)
        self.assertEqual(len(new_warnings), 1)
        self.assertEqual(new_warnings[0]["target"], "fresh")

    def test_new_warnings_are_deduplicated_by_key(self):
        # Given the same un-whitelisted warning twice
        actual = [
            {"doc_path": "d", "kind": "domain_object_reference", "target": "x"},
            {"doc_path": "d", "kind": "domain_object_reference", "target": "x"},
        ]

        # When
        new_warnings, suppressed = benchmark.partition_warnings(actual, [])

        # Then
        self.assertEqual(suppressed, 0)
        self.assertEqual(len(new_warnings), 1)


class PruneWhitelistTest(unittest.TestCase):
    def test_stale_entries_removed_and_matching_kept_with_comments(self):
        # Given a whitelist where one entry matches an actual warning and one
        # does not
        whitelist = [
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
        actual = [
            {"doc_path": "d", "kind": "domain_object_reference", "target": "still-here"}
        ]

        # When
        kept, removed = benchmark.prune_whitelist(whitelist, actual)

        # Then
        self.assertEqual(len(kept), 1)
        self.assertEqual(kept[0]["target"], "still-here")
        self.assertEqual(kept[0]["comment"], "keep me")  # comment preserved
        self.assertEqual(len(removed), 1)
        self.assertEqual(removed[0]["target"], "gone")


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


class WhitelistIoTest(unittest.TestCase):
    def test_write_then_load_round_trips_entries(self):
        # Given entries and a temp path
        entries = [
            {
                "doc_path": "d",
                "kind": "object_type_mismatch",
                "target": "Fault",
                "comment": "expected",
            }
        ]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "whitelist.json"

            # When
            benchmark.write_whitelist(path, entries)
            loaded = benchmark.load_whitelist(path)

            # Then
            self.assertEqual(loaded, entries)

    def test_load_missing_file_returns_empty_list(self):
        # Given a path that doesn't exist
        with tempfile.TemporaryDirectory() as tmp:
            # When
            loaded = benchmark.load_whitelist(Path(tmp) / "nope.json")

            # Then
            self.assertEqual(loaded, [])


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
            benchmark.write_whitelist(
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
            self.assertEqual(len(benchmark.load_whitelist(wl)), 1)


if __name__ == "__main__":
    unittest.main()


class GenerateWarmupPackageTest(unittest.TestCase):
    def test_writes_a_single_document_site_outside_the_doc_glob(self):
        # Given a workspace root supplying the default template, and a target
        # directory standing in for the generated benchmark workspace
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            workspace = root / "workspace"
            (workspace / "templates").mkdir(parents=True)
            (workspace / "templates" / "default.html").write_text("<html>{{ body }}</html>")
            target = root / "corpus"
            target.mkdir()
            original_target = benchmark.TARGET_DIR
            benchmark.TARGET_DIR = target

            # When
            try:
                benchmark.generate_warmup_package(str(workspace))
            finally:
                benchmark.TARGET_DIR = original_target

            # Then — the package sits beside Doc/, so Doc's **/*.rst glob
            # cannot pick its document up
            warmup = target / benchmark.WARMUP_PACKAGE
            self.assertNotIn("Doc", warmup.relative_to(target).parts)

            # And it is a buildable one-document site
            self.assertEqual(list(warmup.glob("*.rst")), [warmup / "index.rst"])
            build = (warmup / "BUILD.bazel").read_text()
            self.assertIn('srcs = ["index.rst"]', build)
            self.assertIn("rusty_sphinx_site(", build)
            self.assertIn('deps = [":warmup_docs"]', build)

            # And it carries its own config and a copy of the template
            self.assertIn("project =", (warmup / "rusty_sphinx.toml").read_text())
            self.assertEqual(
                (warmup / "custom_template.html").read_text(), "<html>{{ body }}</html>"
            )


class TimedBazelBuildTest(unittest.TestCase):
    def _run(self, tmp, returncode, extra_flags=()):
        calls = []

        def fake_run(command, cwd, capture_output, text):
            calls.append((command, cwd))
            return subprocess.CompletedProcess(command, returncode, "out\n", "err\n")

        original_run = benchmark.subprocess.run
        original_target = benchmark.TARGET_DIR
        benchmark.subprocess.run = fake_run
        benchmark.TARGET_DIR = tmp
        try:
            result = benchmark.timed_bazel_build(
                "//Doc:site", "build.log", extra_flags=extra_flags
            )
        finally:
            benchmark.subprocess.run = original_run
            benchmark.TARGET_DIR = original_target
        return result, calls

    def test_builds_the_target_in_the_generated_workspace_and_times_it(self):
        # Given a build that succeeds
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)

            # When
            (succeeded, duration), calls = self._run(
                root, 0, extra_flags=["--profile=profile.json.gz"]
            )

            # Then
            self.assertTrue(succeeded)
            self.assertGreaterEqual(duration, 0.0)

            # And the invocation carried the shared config flags, the extra
            # flags and the target, run from the generated workspace
            command, cwd = calls[0]
            self.assertEqual(cwd, str(root))
            self.assertEqual(command[:2], ["bazel", "build"])
            for flag in benchmark.BUILD_CONFIG_FLAGS:
                self.assertIn(flag, command)
            self.assertIn("--profile=profile.json.gz", command)
            self.assertEqual(command[-1], "//Doc:site")

    def test_captured_output_is_written_to_the_log_even_when_the_build_fails(self):
        # Given a build that fails
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            out = io.StringIO()
            original_stdout = sys.stdout
            sys.stdout = out

            # When
            try:
                (succeeded, _duration), _calls = self._run(root, 1)
            finally:
                sys.stdout = original_stdout

            # Then the failure is reported, and both streams are on disk
            self.assertFalse(succeeded)
            self.assertEqual((root / "build.log").read_text(), "out\nerr\n")
