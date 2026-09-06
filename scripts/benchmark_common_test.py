import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import benchmark_common


class CollectWarningSidecarsTest(unittest.TestCase):
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
            entries, file_count = benchmark_common.collect_warning_sidecars(root)

            # Then
            self.assertEqual(file_count, 2)
            self.assertEqual(len(entries), 2)
            keys = {benchmark_common.warning_key(e) for e in entries}
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
            entries, file_count = benchmark_common.collect_warning_sidecars(Path(tmp))

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
        self.assertEqual(benchmark_common.warning_key(a), benchmark_common.warning_key(b))
        self.assertEqual(
            benchmark_common.warning_key(a), ("d", "object_type_mismatch", "Fault")
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
        new_warnings, suppressed = benchmark_common.partition_warnings(actual, whitelist)

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
        new_warnings, suppressed = benchmark_common.partition_warnings(actual, [])

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
        kept, removed = benchmark_common.prune_whitelist(whitelist, actual)

        # Then
        self.assertEqual(len(kept), 1)
        self.assertEqual(kept[0]["target"], "still-here")
        self.assertEqual(kept[0]["comment"], "keep me")  # comment preserved
        self.assertEqual(len(removed), 1)
        self.assertEqual(removed[0]["target"], "gone")


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
            benchmark_common.write_whitelist(path, entries)
            loaded = benchmark_common.load_whitelist(path)

            # Then
            self.assertEqual(loaded, entries)

    def test_load_missing_file_returns_empty_list(self):
        # Given a path that doesn't exist
        with tempfile.TemporaryDirectory() as tmp:
            # When
            loaded = benchmark_common.load_whitelist(Path(tmp) / "nope.json")

            # Then
            self.assertEqual(loaded, [])


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

            # When
            benchmark_common.generate_warmup_package(target, str(workspace))

            # Then — the package sits beside Doc/, so Doc's **/*.rst glob
            # cannot pick its document up
            warmup = target / benchmark_common.WARMUP_PACKAGE
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

        original_run = benchmark_common.subprocess.run
        benchmark_common.subprocess.run = fake_run
        try:
            result = benchmark_common.timed_bazel_build(
                tmp, "//Doc:site", "build.log", extra_flags=extra_flags
            )
        finally:
            benchmark_common.subprocess.run = original_run
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
            for flag in benchmark_common.BUILD_CONFIG_FLAGS:
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


if __name__ == "__main__":
    unittest.main()
