import io
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

import benchmark_entities


def write(path: Path, text: str):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


class TransclusionTargetsTest(unittest.TestCase):
    def test_included_document_is_found_relative_to_the_including_file(self):
        # Given a document in a subdirectory including a sibling
        with tempfile.TemporaryDirectory() as tmp:
            docs = Path(tmp) / "docs"
            write(docs / "guide" / "index.rst", ".. include:: header.rst\n")
            write(docs / "guide" / "header.rst", "Header\n======\n")

            # When
            included, outside = benchmark_entities.transclusion_targets(docs)

            # Then the path is recorded relative to the corpus package
            self.assertEqual(included, ["guide/header.rst"])
            self.assertEqual(outside, [])

    def test_a_leading_slash_resolves_against_the_source_root(self):
        # Given an absolute (source-root-relative) include written from a
        # subdirectory — the corpus package is the Sphinx source root
        with tempfile.TemporaryDirectory() as tmp:
            docs = Path(tmp) / "docs"
            write(docs / "guide" / "index.rst", ".. include:: /shared/header.rst\n")
            write(docs / "shared" / "header.rst", "Header\n======\n")

            # When
            included, _outside = benchmark_entities.transclusion_targets(docs)

            # Then it resolves from the root, not from the including document
            self.assertEqual(included, ["shared/header.rst"])

    def test_a_source_outside_the_root_is_paired_with_where_the_parser_looks(self):
        # Given a literalinclude escaping the source root
        with tempfile.TemporaryDirectory() as tmp:
            docs = Path(tmp) / "docs"
            write(docs / "index.rst", ".. literalinclude:: ../pharaoh.toml\n")
            write(Path(tmp) / "pharaoh.toml", "[tool]\n")

            # When
            included, outside = benchmark_entities.transclusion_targets(docs)

            # Then the real location is paired with the source-root-relative
            # path the parser clamps `..` to, which is where it must be copied
            self.assertEqual(outside, [("pharaoh.toml", Path(tmp) / "pharaoh.toml")])
            self.assertEqual(included, [])

    def test_a_csv_table_file_option_counts_as_a_parse_time_read(self):
        # Given a csv-table naming its data outside the source root
        with tempfile.TemporaryDirectory() as tmp:
            docs = Path(tmp) / "docs"
            write(docs / "index.rst", ".. csv-table:: Cars\n   :file: ../data/cars.csv\n")
            write(Path(tmp) / "data" / "cars.csv", "a,b\n")

            # When
            _included, outside = benchmark_entities.transclusion_targets(docs)

            # Then
            self.assertEqual(
                outside, [("data/cars.csv", Path(tmp) / "data" / "cars.csv")]
            )

    def test_a_source_that_does_not_exist_is_left_to_the_parser_to_report(self):
        # Given a literalinclude naming a file nobody shipped
        with tempfile.TemporaryDirectory() as tmp:
            docs = Path(tmp) / "docs"
            write(docs / "index.rst", ".. literalinclude:: ../gone.toml\n")

            # When
            included, outside = benchmark_entities.transclusion_targets(docs)

            # Then nothing is declared or copied — there is nothing to copy, and
            # `include.file-unreadable` is the honest outcome
            self.assertEqual((included, outside), ([], []))

    def test_a_templated_target_is_skipped(self):
        # Given a Pharaoh-templated include, which names no file at all
        with tempfile.TemporaryDirectory() as tmp:
            docs = Path(tmp) / "docs"
            write(docs / "index.rst", ".. include:: {{page}}\n")

            # When
            included, outside = benchmark_entities.transclusion_targets(docs)

            # Then nothing is declared — the parse will report it instead
            self.assertEqual((included, outside), ([], []))

    def test_a_jinja_include_is_declared_like_any_other_parse_time_read(self):
        # Given the two lines 24 of the corpus's documents open with
        with tempfile.TemporaryDirectory() as tmp:
            docs = Path(tmp) / "docs"
            write(
                docs / "basic_example" / "index.rst",
                '{% set page="index.rst" %}\n'
                '{% include "demo_page_header.rst" with context %}\n',
            )
            write(docs / "demo_page_header.rst", "Header\n")

            # When
            included, outside = benchmark_entities.transclusion_targets(docs)

            # Then the template is a file the parser reads, so it is declared
            # as parse_data and kept out of srcs — resolved from the *source
            # root*, as Jinja's own loader resolves a template name, not from
            # the subdirectory the including document sits in
            self.assertEqual(included, ["demo_page_header.rst"])
            self.assertEqual(outside, [])

    def test_the_corpus_library_asks_for_its_sources_to_be_templated(self):
        # Given / When
        text = benchmark_entities.render_corpus_build_file([], [])

        # Then — without it the preamble would parse as the prose it is, and
        # the shared header would never reach a page
        self.assertIn("jinja = True", text)

    def test_a_non_rst_source_inside_the_package_needs_no_special_handling(self):
        # Given a literalinclude of a Python file inside the package, which the
        # generated `parse_data` glob already covers
        with tempfile.TemporaryDirectory() as tmp:
            docs = Path(tmp) / "docs"
            write(docs / "index.rst", ".. literalinclude:: utils/helper.py\n")
            write(docs / "utils" / "helper.py", "x = 1\n")

            # When
            included, outside = benchmark_entities.transclusion_targets(docs)

            # Then it appears in neither list
            self.assertEqual((included, outside), ([], []))


class ResolveFromDocumentTest(unittest.TestCase):
    def test_a_relative_path_resolves_against_the_documents_directory(self):
        # Given
        # When / Then
        self.assertEqual(
            benchmark_entities.resolve_from_document("header.rst", "guide/index.rst"),
            "guide/header.rst",
        )

    def test_a_leading_slash_means_the_source_root(self):
        # Given a source-root-relative path written from a subdirectory
        # When / Then
        self.assertEqual(
            benchmark_entities.resolve_from_document("/_images/x.png", "a/b/index.rst"),
            "_images/x.png",
        )

    def test_parent_components_pop_and_never_escape_the_root(self):
        # Given a path with more `..` than there is depth — the parser pops
        # rather than escaping, so the result is clamped at the root
        # When / Then
        self.assertEqual(
            benchmark_entities.resolve_from_document("../../x.toml", "index.rst"),
            "x.toml",
        )
        self.assertEqual(
            benchmark_entities.resolve_from_document("../shared/x.png", "a/b/i.rst"),
            "a/shared/x.png",
        )


class CopyOutsideSourcesTest(unittest.TestCase):
    def test_the_file_is_placed_where_the_parser_will_look_for_it(self):
        # Given a source outside the source root and the path the parser clamps
        # its reference to
        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp) / "docs"
            workspace.mkdir()
            source = Path(tmp) / "pharaoh.toml"
            source.write_text("[tool]\n")

            # When
            benchmark_entities.copy_outside_sources(workspace, [("pharaoh.toml", source)])

            # Then the corpus itself is untouched; only the disposable generated
            # workspace gains a file
            self.assertEqual((workspace / "pharaoh.toml").read_text(), "[tool]\n")

    def test_a_nested_destination_has_its_directories_created(self):
        # Given a destination inside a directory that does not exist yet
        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp) / "docs"
            workspace.mkdir()
            source = Path(tmp) / "cars.csv"
            source.write_text("a,b\n")

            # When
            benchmark_entities.copy_outside_sources(
                workspace, [("data/cars.csv", source)]
            )

            # Then
            self.assertEqual((workspace / "data" / "cars.csv").read_text(), "a,b\n")


class RenderCorpusBuildFileTest(unittest.TestCase):
    def test_included_documents_are_excluded_from_srcs_and_declared_as_data(self):
        # Given one included document
        # When
        text = benchmark_entities.render_corpus_build_file(["header.rst"], [])

        # Then it is kept out of srcs — or it would be published as a page of
        # its own as well as being spliced in — and declared as parse_data
        srcs, parse_data = text.split("parse_data = ")
        self.assertIn('"header.rst"', srcs.split("exclude = ")[1])
        self.assertIn('"header.rst"', parse_data)

    def test_a_source_copied_in_from_outside_is_declared_as_parse_data(self):
        # Given a file copied to where the parser looks for it
        # When
        text = benchmark_entities.render_corpus_build_file([], ["pharaoh.toml"])

        # Then it still has to reach the parse action's sandbox
        self.assertIn('"pharaoh.toml"', text.split("parse_data = ")[1])

    def test_every_glob_excludes_bazels_output_symlinks(self):
        # Given a corpus whose root package *is* the workspace root, so the
        # convenience symlinks sit inside every glob's reach
        # When
        text = benchmark_entities.render_corpus_build_file([], [])

        # Then no glob can follow one into the previous run's outputs — which
        # would make the site's own output an input to the action writing it
        globs = text.count("glob(")
        excludes = text.count(f'{json.dumps(benchmark_entities.OUTPUT_SYMLINK_GLOB)}')
        self.assertEqual(globs, 3)
        self.assertEqual(excludes, 3)

    def test_the_schema_is_declared_on_both_rules(self):
        # Given any corpus — the library parses against the schema and the site
        # indexes against it; a mismatch is `entity.schema-mismatch`
        # When
        text = benchmark_entities.render_corpus_build_file([], [])

        # Then
        self.assertEqual(text.count(f'entity_schema = "{benchmark_entities.SCHEMA_NAME}"'), 2)


class DiscardStaleCorpusOutputsTest(unittest.TestCase):
    def _discard(self, bazel_bin: Path, workspace: Path):
        def fake_run(command, cwd, capture_output, text):
            return subprocess.CompletedProcess(command, 0, f"{bazel_bin}\n", "")

        original = benchmark_entities.subprocess.run
        benchmark_entities.subprocess.run = fake_run
        try:
            benchmark_entities.discard_stale_corpus_outputs(workspace)
        finally:
            benchmark_entities.subprocess.run = original

    def test_corpus_outputs_go_but_the_warm_toolchain_stays(self):
        # Given a bazel-bin holding corpus outputs beside the fetched
        # repositories and the generated packages
        with tempfile.TemporaryDirectory() as tmp:
            bazel_bin = Path(tmp) / "bazel-bin"
            for name in ("external", *benchmark_entities.GENERATED_PACKAGES, "coffee"):
                (bazel_bin / name).mkdir(parents=True)
            (bazel_bin / "index.ast").parent.mkdir(parents=True, exist_ok=True)
            (bazel_bin / "index.ast").write_text("{}")

            # When
            self._discard(bazel_bin, Path(tmp))

            # Then the corpus outputs are gone — a document dropped from the
            # corpus must not keep being counted — while the compiled
            # rusty-sphinx binary under `external` stays warm
            self.assertFalse((bazel_bin / "coffee").exists())
            self.assertFalse((bazel_bin / "index.ast").exists())
            self.assertTrue((bazel_bin / "external").exists())
            for name in benchmark_entities.GENERATED_PACKAGES:
                self.assertTrue((bazel_bin / name).exists())

    def test_a_workspace_bazel_cannot_answer_for_is_left_alone(self):
        # Given a `bazel info` that fails (no output base yet)
        with tempfile.TemporaryDirectory() as tmp:
            def fake_run(command, cwd, capture_output, text):
                return subprocess.CompletedProcess(command, 1, "", "no such workspace")

            original = benchmark_entities.subprocess.run
            benchmark_entities.subprocess.run = fake_run
            try:
                # When / Then — it returns rather than raising
                benchmark_entities.discard_stale_corpus_outputs(Path(tmp))
            finally:
                benchmark_entities.subprocess.run = original


class ParseBuildWarningsTest(unittest.TestCase):
    def test_a_warning_line_becomes_a_whitelistable_entry(self):
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
        self.assertEqual(
            entries,
            [
                {
                    "doc_path": "docs/req.rst",
                    "kind": "entity.invalid-attribute-value",
                    "target": '`:approved:`: expected `true` or `false`, found "False"',
                }
            ],
        )

    def test_the_same_warning_from_two_actions_is_counted_once(self):
        # Given a document that is parsed and rendered in separate actions,
        # each printing the same diagnostic
        log = (
            "warning: docs/a.rst:1:1: entity.unknown-target: `:req:` `R_1`\n"
            "warning: docs/a.rst:1:1: entity.unknown-target: `:req:` `R_1`\n"
        )

        # When
        entries = benchmark_entities.parse_build_warnings(log)

        # Then
        self.assertEqual(len(entries), 1)

    def test_ordinary_bazel_output_is_ignored(self):
        # Given a log with no diagnostics in it
        log = "INFO: Build completed successfully, 42 total actions\nERROR: nope\n"

        # When
        entries = benchmark_entities.parse_build_warnings(log)

        # Then
        self.assertEqual(entries, [])


class WarningSectionsTest(unittest.TestCase):
    def test_sections_are_built_from_the_codes_seen_most_frequent_first(self):
        # Given warnings carrying three different codes
        warnings = [
            {"kind": "entity.unknown-target"},
            {"kind": "entity.unknown-attribute"},
            {"kind": "entity.unknown-attribute"},
        ]

        # When
        sections = benchmark_entities.warning_sections(warnings)

        # Then — derived from the data, so a code added on the Rust side shows
        # up without this file being touched
        self.assertEqual(
            sections,
            [
                ("entity.unknown-attribute", "entity.unknown-attribute"),
                ("entity.unknown-target", "entity.unknown-target"),
            ],
        )


class FormatEntityWarningTest(unittest.TestCase):
    def test_the_code_is_dropped_when_the_section_already_states_it(self):
        # Given one warning
        entry = {"doc_path": "docs/a.rst", "kind": "entity.unknown-attribute", "target": "`:foo:`"}

        # When / Then
        self.assertEqual(
            benchmark_entities.format_entity_warning(entry),
            "docs/a.rst: entity.unknown-attribute: `:foo:`",
        )
        self.assertEqual(
            benchmark_entities.format_entity_warning(entry, include_kind=False),
            "docs/a.rst: `:foo:`",
        )


class TallyAstTest(unittest.TestCase):
    def test_entities_and_unknown_directives_are_counted_separately(self):
        # Given an .ast holding one parsed entity and one unrecognized directive
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "doc.ast"
            path.write_text(json.dumps({
                "nodes": [
                    {"Directive": {"Entity": {"type_name": "req", "id": "R_1"}}},
                    {"Directive": {"Entity": {"type_name": "req", "id": "R_2"}}},
                    {"Directive": {"Unknown": {"name": "needtable"}}},
                ]
            }))

            # When
            unknown, entities = benchmark_entities.tally_ast([path])

            # Then a type with instances is the positive signal, and a type the
            # schema failed to declare would appear as an unknown directive
            self.assertEqual(entities, {"req": 2})
            self.assertEqual(unknown, {"needtable": 1})

    def test_an_unreadable_ast_does_not_abort_the_analysis(self):
        # Given a truncated file among the outputs
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "broken.ast"
            path.write_text("{ not json")

            # When
            unknown, entities = benchmark_entities.tally_ast([path])

            # Then
            self.assertEqual((unknown, entities), ({}, {}))


class WriteSchemaReportTest(unittest.TestCase):
    def test_entries_are_grouped_by_category_largest_first(self):
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
        self.assertLess(text.index("variants (2)"), text.index("constraints (1)"))
        self.assertIn("  - field a", text)

    def test_an_empty_report_says_so(self):
        # Given a configuration that converted completely
        out = io.StringIO()

        # When
        benchmark_entities.write_schema_report(out, [])

        # Then
        self.assertIn("None found.", out.getvalue())


if __name__ == "__main__":
    unittest.main()
