import json
import shutil
import subprocess
from pathlib import Path

import pytest

import publish_pages
from publish_pages import BenchmarkSite, Build, PublishError, Slot, SlotKind

BASE_URL = "https://example.org/rinx/"


def write(path: Path, text: str = "") -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


def fake_site(root: Path, entry: str, marker: str = "") -> Path:
    """A directory shaped like a `rinx_site` output: pages, index, inventory, assets."""
    write(root / entry, f"<p>{marker}</p>")
    write(root / "genindex.html", "<p>index</p>")
    write(root / "default.css", "body {}")
    write(root / "objects.inv", f"inventory {marker}")
    return root


@pytest.fixture
def builds(tmp_path: Path) -> tuple[Path, Path]:
    docs = fake_site(tmp_path / "docs_out", "docs/index.html", "docs")
    write(docs / "docs/syntax.html", "<p>syntax</p>")
    examples = fake_site(tmp_path / "examples_out", "examples/index.html", "examples")
    return docs, examples


@pytest.fixture
def benchmarks_dir(tmp_path: Path) -> Path:
    """Two benchmark sites as the scripts export them.

    One has its pages under a subdirectory and a report; the other has neither.
    """
    parent = tmp_path / "benchmarks_out"
    cpython = fake_site(parent / "cpython", "Doc/index.html", "cpython")
    write(cpython / "report.txt", "cpython report")
    write(cpython / "entry.txt", "Doc/index.html\n")
    demo = fake_site(parent / "sphinx-needs-demo", "index.html", "demo")
    write(demo / "entry.txt", "index.html\n")
    return parent


@pytest.fixture
def benchmarks(benchmarks_dir: Path) -> list[BenchmarkSite]:
    return publish_pages.read_benchmark_sites(benchmarks_dir)


@pytest.fixture
def root(tmp_path: Path) -> Path:
    path = tmp_path / "pages"
    path.mkdir()
    return path


def redirect_target(page: Path) -> str:
    """Where a page written by `redirect_page` sends its reader."""
    text = page.read_text(encoding="utf-8")
    return text.split('<link rel="canonical" href="')[1].split('"')[0]


class TestSlot:
    def test_parses_the_three_shapes(self) -> None:
        # Given / When / Then
        assert Slot.parse("main") == Slot(SlotKind.MAIN, "main")
        assert Slot.parse("v0.12.3") == Slot(SlotKind.RELEASE, "v0.12.3")
        assert Slot.parse("pr/42") == Slot(SlotKind.PREVIEW, "pr/42")

    @pytest.mark.parametrize(
        "raw", ["latest", "docs", "v1.2", "1.2.3", "pr/0", "pr/x", "pr/1/../..", "../main", ""]
    )
    def test_refuses_anything_else(self, raw: str) -> None:
        # Given a name the derived files own, or one escaping the branch
        # When / Then
        with pytest.raises(ValueError, match="not a version slot"):
            Slot.parse(raw)


class TestReadBenchmarkSites:
    def test_reads_each_site_by_its_directorys_name(self, benchmarks_dir: Path) -> None:
        # Given / When
        sites = publish_pages.read_benchmark_sites(benchmarks_dir)

        # Then
        assert sites == [
            BenchmarkSite("cpython", "Doc/index.html", benchmarks_dir / "cpython"),
            BenchmarkSite("sphinx-needs-demo", "index.html", benchmarks_dir / "sphinx-needs-demo"),
        ]

    def test_finds_none_without_a_directory(self, tmp_path: Path) -> None:
        # Given / When / Then
        assert publish_pages.read_benchmark_sites(None) == []
        assert publish_pages.read_benchmark_sites(tmp_path / "absent") == []

    @pytest.mark.parametrize(
        ("name", "entry"),
        [
            ("CPython", "index.html"),
            ("cpython", None),
            ("cpython", "\n"),
            ("cpython", "/abs/index.html"),
            ("cpython", "../index.html"),
            ("cpython", "Doc/index.html"),
        ],
    )
    def test_refuses_a_site_it_cannot_place(
        self, tmp_path: Path, name: str, entry: str | None
    ) -> None:
        # Given
        site = fake_site(tmp_path / "out" / name, "index.html")
        if entry is not None:
            write(site / "entry.txt", entry)

        # When / Then
        with pytest.raises(PublishError, match="is not a benchmark site"):
            publish_pages.read_benchmark_sites(tmp_path / "out")


class TestReleaseKey:
    def test_orders_numerically(self) -> None:
        # Given / When
        ordered = sorted(["v0.10.0", "v0.9.1", "v1.0.0", "v0.9.10"], key=publish_pages.release_key)

        # Then
        assert ordered == ["v0.9.1", "v0.9.10", "v0.10.0", "v1.0.0"]

    def test_refuses_a_non_release(self) -> None:
        with pytest.raises(ValueError, match="not a release"):
            publish_pages.release_key("main")


class TestListReleases:
    def test_lists_release_directories_newest_first(self, root: Path) -> None:
        # Given
        for name in ["v0.9.0", "v0.10.0", "main", "pr", "latest"]:
            (root / name).mkdir()
        write(root / "v9.9.9")  # a file, not a published version

        # When / Then
        assert publish_pages.list_releases(root) == ["v0.10.0", "v0.9.0"]


class TestVersionsManifest:
    def test_puts_the_preferred_newest_release_first_then_main(self) -> None:
        # Given / When
        manifest = publish_pages.versions_manifest(
            ["v0.2.0", "v0.1.0"], has_main=True, base_url=BASE_URL
        )

        # Then
        assert manifest == [
            {
                "version": "v0.2.0",
                "url": f"{BASE_URL}v0.2.0/",
                "name": "v0.2.0 (latest)",
                "preferred": True,
            },
            {"version": "main", "name": "main (development)", "url": f"{BASE_URL}main/"},
            {"version": "v0.1.0", "url": f"{BASE_URL}v0.1.0/"},
        ]

    def test_lists_main_alone_before_any_release(self) -> None:
        # Given / When
        manifest = publish_pages.versions_manifest([], has_main=True, base_url=BASE_URL)

        # Then — with nothing preferred, the switcher shows no banner
        assert manifest == [
            {"version": "main", "name": "main (development)", "url": f"{BASE_URL}main/"}
        ]

    def test_is_empty_without_versions(self) -> None:
        assert publish_pages.versions_manifest([], has_main=False, base_url=BASE_URL) == []


class TestRedirectPage:
    def test_keeps_the_fragment_and_works_without_scripts(self) -> None:
        # Given / When
        page = publish_pages.redirect_page("../v1.0.0/docs/a.html")

        # Then
        assert 'location.replace("../v1.0.0/docs/a.html" + location.hash)' in page
        assert '<meta http-equiv="refresh" content="0; url=../v1.0.0/docs/a.html">' in page
        assert '<a href="../v1.0.0/docs/a.html">' in page

    def test_escapes_the_target(self) -> None:
        # Given / When
        page = publish_pages.redirect_page('a"b<c>.html')

        # Then — in the markup and in the script, which a `</script>` would end
        assert 'href="a&quot;b&lt;c&gt;.html"' in page
        assert "<c>" not in page


class TestScriptString:
    def test_cannot_end_the_script_element(self) -> None:
        # Given / When
        literal = publish_pages.script_string("</script><b>&")

        # Then
        assert literal == '"\\u003c/script\\u003e\\u003cb\\u003e\\u0026"'
        assert json.loads(literal) == "</script><b>&"


class TestWalkFiles:
    def test_yields_every_file_relative_and_sorted(self, tmp_path: Path) -> None:
        # Given
        write(tmp_path / "b.txt")
        write(tmp_path / "a/z.txt")
        write(tmp_path / "a/y.txt")

        # When / Then
        assert list(publish_pages.walk_files(tmp_path)) == [
            Path("b.txt"),
            Path("a/y.txt"),
            Path("a/z.txt"),
        ]


class TestWriteRedirectTree:
    def test_mirrors_pages_as_relative_redirects(self, root: Path) -> None:
        # Given
        fake_site(root / "v1.0.0", "docs/index.html")

        # When
        publish_pages.write_redirect_tree(root / "v1.0.0", root / "latest")

        # Then
        assert redirect_target(root / "latest/docs/index.html") == "../../v1.0.0/docs/index.html"
        assert redirect_target(root / "latest/genindex.html") == "../v1.0.0/genindex.html"

    def test_copies_inventories_and_skips_assets(self, root: Path) -> None:
        # Given
        fake_site(root / "v1.0.0", "docs/index.html", "one")

        # When
        publish_pages.write_redirect_tree(root / "v1.0.0", root / "latest")

        # Then — a tool fetching objects.inv does not follow an HTML redirect
        assert (root / "latest/objects.inv").read_text(encoding="utf-8") == "inventory one"
        assert not (root / "latest/default.css").exists()


class TestCopySite:
    def test_leaves_a_copy_of_read_only_outputs_removable(self, tmp_path: Path) -> None:
        # Given a site as Bazel leaves it: read-only files and directories
        source = fake_site(tmp_path / "out", "docs/index.html")
        for path in [*source.rglob("*"), source]:
            path.chmod(0o555 if path.is_dir() else 0o444)

        # When
        publish_pages.copy_site(source, tmp_path / "copy")

        # Then — a later change can replace it
        assert (tmp_path / "copy/docs/index.html").is_file()
        shutil.rmtree(tmp_path / "copy")
        for path in [*source.rglob("*"), source]:
            path.chmod(0o755 if path.is_dir() else 0o644)


class TestPlaceBuild:
    def test_puts_docs_at_the_slot_root_and_examples_below_it(
        self, root: Path, builds: tuple[Path, Path]
    ) -> None:
        # Given
        docs, examples = builds

        # When
        publish_pages.place_build(root, Slot.parse("main"), Build(docs, examples), force=False)

        # Then
        assert (root / "main/docs/syntax.html").is_file()
        assert (root / "main/objects.inv").is_file()
        assert (root / "main/example-site/examples/index.html").is_file()
        assert redirect_target(root / "main/index.html") == "docs/index.html"
        assert redirect_target(root / "main/example-site/index.html") == "examples/index.html"

    def test_replaces_what_the_slot_held(self, root: Path, builds: tuple[Path, Path]) -> None:
        # Given a page the new build no longer has
        docs, examples = builds
        write(root / "pr/7/docs/removed.html")

        # When
        publish_pages.place_build(root, Slot.parse("pr/7"), Build(docs, examples), force=False)

        # Then
        assert not (root / "pr/7/docs/removed.html").exists()
        assert (root / "pr/7/docs/syntax.html").is_file()

    def test_refuses_to_replace_a_published_release(
        self, root: Path, builds: tuple[Path, Path]
    ) -> None:
        # Given
        docs, examples = builds
        write(root / "v1.0.0/docs/index.html", "original")

        # When / Then
        with pytest.raises(PublishError, match="already published"):
            publish_pages.place_build(
                root, Slot.parse("v1.0.0"), Build(docs, examples), force=False
            )
        assert (root / "v1.0.0/docs/index.html").read_text(encoding="utf-8") == "original"

    def test_replaces_a_release_when_forced(self, root: Path, builds: tuple[Path, Path]) -> None:
        # Given
        docs, examples = builds
        write(root / "v1.0.0/docs/index.html", "original")

        # When
        publish_pages.place_build(root, Slot.parse("v1.0.0"), Build(docs, examples), force=True)

        # Then
        assert (root / "v1.0.0/docs/index.html").read_text(encoding="utf-8") == "<p>docs</p>"

    def test_refuses_a_directory_that_is_not_a_site(
        self, root: Path, builds: tuple[Path, Path], tmp_path: Path
    ) -> None:
        # Given — e.g. the wrong bazel-bin path
        docs, _ = builds
        empty = tmp_path / "empty"
        empty.mkdir()

        # When / Then
        with pytest.raises(PublishError, match=r"no objects\.inv"):
            publish_pages.place_build(root, Slot.parse("main"), Build(docs, empty), force=False)
        assert not (root / "main").exists()


class TestPlaceBenchmarks:
    def test_puts_each_benchmark_under_the_slots_benchmarks_directory(
        self, root: Path, builds: tuple[Path, Path], benchmarks: list[BenchmarkSite]
    ) -> None:
        # Given
        docs, examples = builds

        # When
        publish_pages.place_build(
            root, Slot.parse("pr/7"), Build(docs, examples, benchmarks), force=False
        )

        # Then
        assert (root / "pr/7/benchmarks/cpython/Doc/index.html").is_file()
        assert (root / "pr/7/benchmarks/sphinx-needs-demo/index.html").is_file()
        assert (root / "pr/7/docs/syntax.html").is_file()

    def test_writes_a_landing_page_linking_each_entry_and_report(
        self, root: Path, builds: tuple[Path, Path], benchmarks: list[BenchmarkSite]
    ) -> None:
        # Given
        docs, examples = builds

        # When
        publish_pages.place_build(
            root, Slot.parse("main"), Build(docs, examples, benchmarks), force=False
        )

        # Then — only the benchmark that exported a report links one
        landing = (root / "main/benchmarks/index.html").read_text(encoding="utf-8")
        assert 'href="cpython/Doc/index.html"' in landing
        assert 'href="cpython/report.txt"' in landing
        assert 'href="sphinx-needs-demo/index.html"' in landing
        assert 'href="sphinx-needs-demo/report.txt"' not in landing

    def test_writes_no_benchmarks_directory_without_benchmarks(
        self, root: Path, builds: tuple[Path, Path]
    ) -> None:
        # Given
        docs, examples = builds

        # When
        publish_pages.place_build(root, Slot.parse("main"), Build(docs, examples), force=False)

        # Then
        assert not (root / "main/benchmarks").exists()

    def test_refuses_a_benchmark_that_is_not_a_site(
        self, root: Path, builds: tuple[Path, Path], tmp_path: Path
    ) -> None:
        # Given
        docs, examples = builds
        empty = tmp_path / "empty"
        empty.mkdir()

        # When / Then
        with pytest.raises(PublishError, match=r"no objects\.inv"):
            publish_pages.place_build(
                root,
                Slot.parse("main"),
                Build(docs, examples, [BenchmarkSite("cpython", "index.html", empty)]),
                force=False,
            )
        assert not (root / "main").exists()

    def test_refuses_two_benchmarks_of_one_name(
        self, root: Path, builds: tuple[Path, Path], benchmarks: list[BenchmarkSite]
    ) -> None:
        # Given
        docs, examples = builds

        # When / Then
        with pytest.raises(PublishError, match=r"cpython.*twice"):
            publish_pages.place_build(
                root,
                Slot.parse("main"),
                Build(docs, examples, [benchmarks[0], benchmarks[0]]),
                force=False,
            )


class TestRemoveSlot:
    def test_removes_a_preview_and_the_emptied_pr_directory(self, root: Path) -> None:
        # Given
        write(root / "pr/7/index.html")

        # When
        publish_pages.remove_slot(root, Slot.parse("pr/7"), force=False)

        # Then
        assert not (root / "pr").exists()

    def test_keeps_the_other_previews(self, root: Path) -> None:
        # Given
        write(root / "pr/7/index.html")
        write(root / "pr/8/index.html")

        # When
        publish_pages.remove_slot(root, Slot.parse("pr/7"), force=False)

        # Then
        assert (root / "pr/8/index.html").is_file()

    def test_is_quiet_about_a_slot_already_gone(self, root: Path) -> None:
        # Given a pull request closed without a preview ever published
        # When / Then
        publish_pages.remove_slot(root, Slot.parse("pr/9"), force=False)

    def test_refuses_to_remove_a_release_unless_forced(self, root: Path) -> None:
        # Given
        write(root / "v1.0.0/index.html")

        # When / Then
        with pytest.raises(PublishError, match="without --force"):
            publish_pages.remove_slot(root, Slot.parse("v1.0.0"), force=False)
        publish_pages.remove_slot(root, Slot.parse("v1.0.0"), force=True)
        assert not (root / "v1.0.0").exists()


class TestIsVersionDirectory:
    def test_tells_version_directories_from_derived_ones(self, root: Path) -> None:
        # Given
        for name in ["main", "pr", "v1.0.0", "latest", "docs"]:
            (root / name).mkdir()
        write(root / "v2.0.0")

        # When
        kept = [
            name
            for name in ["main", "pr", "v1.0.0", "latest", "docs", "v2.0.0"]
            if publish_pages.is_version_directory(root / name)
        ]

        # Then
        assert kept == ["main", "pr", "v1.0.0"]


class TestRemoveDerivedFiles:
    def test_keeps_only_the_version_directories_and_git(self, root: Path) -> None:
        # Given
        for name in ["main/a.html", "pr/1/a.html", "v1.0.0/a.html", ".git/HEAD"]:
            write(root / name)
        for name in ["latest/a.html", "docs/a.html", "stale.html", "versions.json"]:
            write(root / name)

        # When
        publish_pages.remove_derived_files(root)

        # Then
        assert sorted(entry.name for entry in root.iterdir()) == [".git", "main", "pr", "v1.0.0"]


class TestRefreshDerivedFiles:
    def publish(self, root: Path, builds: tuple[Path, Path], slot: str) -> None:
        docs, examples = builds
        publish_pages.place_build(root, Slot.parse(slot), Build(docs, examples), force=False)
        publish_pages.refresh_derived_files(root, BASE_URL)

    def test_leads_everything_to_the_newest_release(
        self, root: Path, builds: tuple[Path, Path]
    ) -> None:
        # Given
        for slot in ["v0.1.0", "v0.2.0", "main", "pr/3"]:
            self.publish(root, builds, slot)

        # Then — the front page, latest/ and the URLs from before versions
        assert redirect_target(root / "index.html") == "v0.2.0/docs/index.html"
        assert redirect_target(root / "latest/docs/syntax.html") == "../../v0.2.0/docs/syntax.html"
        assert redirect_target(root / "docs/syntax.html") == "../v0.2.0/docs/syntax.html"
        assert redirect_target(root / "example-site/examples/index.html") == (
            "../../v0.2.0/example-site/examples/index.html"
        )
        assert (root / "objects.inv").is_file()
        assert (root / "latest/objects.inv").is_file()

    def test_writes_the_manifest_robots_and_nojekyll(
        self, root: Path, builds: tuple[Path, Path]
    ) -> None:
        # Given
        for slot in ["v0.1.0", "main", "pr/3"]:
            self.publish(root, builds, slot)

        # When
        manifest = json.loads((root / "versions.json").read_text(encoding="utf-8"))

        # Then — previews are never listed
        assert [entry["version"] for entry in manifest] == ["v0.1.0", "main"]
        assert "Disallow: /pr/" in (root / "robots.txt").read_text(encoding="utf-8")
        assert (root / ".nojekyll").is_file()

    def test_never_mirrors_a_slots_benchmarks(
        self, root: Path, builds: tuple[Path, Path], benchmarks: list[BenchmarkSite]
    ) -> None:
        # Given a release carrying benchmarks
        docs, examples = builds
        publish_pages.place_build(
            root, Slot.parse("v0.1.0"), Build(docs, examples, benchmarks), force=False
        )

        # When
        publish_pages.refresh_derived_files(root, BASE_URL)

        # Then — neither latest/ nor the pre-version root redirects into them
        assert (root / "latest/docs/syntax.html").is_file()
        assert not (root / "latest/benchmarks").exists()
        assert not (root / "benchmarks").exists()

    def test_keeps_benchmarks_only_on_the_newest_release(
        self, root: Path, builds: tuple[Path, Path], benchmarks: list[BenchmarkSite]
    ) -> None:
        # Given two releases, main and a preview, all published with benchmarks
        docs, examples = builds
        for slot in ["v0.1.0", "v0.2.0", "main", "pr/3"]:
            publish_pages.place_build(
                root, Slot.parse(slot), Build(docs, examples, benchmarks), force=False
            )

        # When
        publish_pages.refresh_derived_files(root, BASE_URL)

        # Then — the older release keeps its documentation, but not its benchmarks
        assert not (root / "v0.1.0/benchmarks").exists()
        assert (root / "v0.1.0/docs/syntax.html").is_file()
        for slot in ["v0.2.0", "main", "pr/3"]:
            assert (root / slot / "benchmarks/index.html").is_file(), slot

    def test_a_backfilled_older_release_gets_no_benchmarks(
        self, root: Path, builds: tuple[Path, Path], benchmarks: list[BenchmarkSite]
    ) -> None:
        # Given a newer release already published
        docs, examples = builds
        self.publish(root, builds, "v0.2.0")

        # When an older one is published by hand, with benchmarks
        publish_pages.place_build(
            root, Slot.parse("v0.1.0"), Build(docs, examples, benchmarks), force=False
        )
        publish_pages.refresh_derived_files(root, BASE_URL)

        # Then
        assert not (root / "v0.1.0/benchmarks").exists()

    def test_keeps_search_engines_out_of_the_benchmarks(
        self, root: Path, builds: tuple[Path, Path]
    ) -> None:
        # Given
        self.publish(root, builds, "v0.1.0")

        # Then
        assert "Disallow: /*/benchmarks/" in (root / "robots.txt").read_text(encoding="utf-8")

    def test_leads_to_main_before_the_first_release(
        self, root: Path, builds: tuple[Path, Path]
    ) -> None:
        # Given
        self.publish(root, builds, "main")

        # Then
        assert redirect_target(root / "index.html") == "main/docs/index.html"
        assert redirect_target(root / "docs/syntax.html") == "../main/docs/syntax.html"
        assert not (root / "latest").exists()

    def test_drops_redirects_to_pages_that_no_longer_exist(
        self, root: Path, builds: tuple[Path, Path]
    ) -> None:
        # Given a redirect from an earlier run
        self.publish(root, builds, "main")
        write(root / "docs/gone.html")

        # When
        publish_pages.refresh_derived_files(root, BASE_URL)

        # Then
        assert not (root / "docs/gone.html").exists()

    def test_writes_only_the_shared_files_with_nothing_published(self, root: Path) -> None:
        # Given / When
        publish_pages.refresh_derived_files(root, BASE_URL)

        # Then
        assert sorted(entry.name for entry in root.iterdir()) == [
            ".nojekyll",
            "robots.txt",
            "versions.json",
        ]


class TestDropSupersededBenchmarks:
    def test_keeps_the_newest_releases_and_leaves_other_slots_alone(self, root: Path) -> None:
        # Given
        for slot in ["v0.1.0", "v0.2.0", "main"]:
            write(root / slot / "benchmarks/index.html")

        # When
        publish_pages.drop_superseded_benchmarks(root, ["v0.2.0", "v0.1.0"])

        # Then
        assert not (root / "v0.1.0/benchmarks").exists()
        assert (root / "v0.2.0/benchmarks/index.html").is_file()
        assert (root / "main/benchmarks/index.html").is_file()

    def test_is_quiet_about_a_release_without_benchmarks(self, root: Path) -> None:
        # Given releases from before benchmarks were published
        (root / "v0.1.0").mkdir()

        # When / Then
        publish_pages.drop_superseded_benchmarks(root, ["v0.2.0", "v0.1.0"])


class TestNormalizeBaseUrl:
    def test_ends_in_exactly_one_slash(self) -> None:
        assert publish_pages.normalize_base_url("https://example.org/rinx") == BASE_URL
        assert publish_pages.normalize_base_url("https://example.org/rinx//") == BASE_URL

    def test_refuses_a_non_http_url(self) -> None:
        with pytest.raises(PublishError, match="http"):
            publish_pages.normalize_base_url("example.org/rinx")


# ── Against real git repositories ─────────────────────────────────────────────


def run_git(repo: Path, *args: str) -> str:
    return publish_pages.git(repo, *args).stdout.strip()


@pytest.fixture
def remote(tmp_path: Path) -> Path:
    """A bare repository standing in for GitHub, holding a main branch."""
    bare = tmp_path / "remote.git"
    subprocess.run(["git", "init", "--quiet", "--bare", "-b", "main", str(bare)], check=True)
    seed = tmp_path / "seed"
    subprocess.run(["git", "clone", "--quiet", str(bare), str(seed)], check=True)
    write(seed / "README.md", "rinx")
    run_git(seed, "add", ".")
    run_git(seed, "commit", "--quiet", "-m", "init")
    run_git(seed, "push", "--quiet", "origin", "HEAD:main")
    return bare


def clone(remote: Path, path: Path) -> Path:
    subprocess.run(["git", "clone", "--quiet", str(remote), str(path)], check=True)
    return path


def branch_files(remote: Path) -> list[str]:
    listing = run_git(remote, "ls-tree", "-r", "--name-only", publish_pages.BRANCH)
    return listing.splitlines()


class TestCheckOutBranch:
    def test_starts_an_empty_tree_before_the_first_push(self, remote: Path, tmp_path: Path) -> None:
        # Given
        repo = clone(remote, tmp_path / "pages")

        # When
        tip = publish_pages.check_out_branch(repo)

        # Then — none of main's files leak onto the branch
        assert tip is None
        assert [entry.name for entry in repo.iterdir()] == [".git"]

    def test_checks_out_the_tip_and_drops_leftovers(self, remote: Path, tmp_path: Path) -> None:
        # Given a branch holding one file, and a stray file in the checkout
        repo = clone(remote, tmp_path / "pages")
        publish_pages.publish_change(repo, lambda root: write(root / "a.txt"), "first")
        write(repo / "stray.txt")

        # When
        tip = publish_pages.check_out_branch(repo)

        # Then
        assert tip == run_git(remote, "rev-parse", publish_pages.BRANCH)
        assert sorted(entry.name for entry in repo.iterdir()) == [".git", "a.txt"]

    def test_reports_a_fetch_failing_for_another_reason(self, tmp_path: Path) -> None:
        # Given a clone whose origin does not exist
        repo = tmp_path / "pages"
        subprocess.run(["git", "init", "--quiet", str(repo)], check=True)
        run_git(repo, "remote", "add", "origin", str(tmp_path / "missing.git"))

        # When / Then
        with pytest.raises(subprocess.CalledProcessError):
            publish_pages.check_out_branch(repo)


@pytest.fixture(autouse=True)
def no_retry_wait(monkeypatch: pytest.MonkeyPatch) -> list[float]:
    """Records the waits between retries instead of sleeping through them."""
    waits: list[float] = []
    monkeypatch.setattr(publish_pages.time, "sleep", waits.append)
    return waits


class TestRetryWait:
    def test_grows_with_the_attempt_and_stays_random(self) -> None:
        # Given / When
        first = [publish_pages.retry_wait(1) for _ in range(200)]
        fifth = [publish_pages.retry_wait(5) for _ in range(200)]

        # Then
        assert all(0 <= wait <= publish_pages.RETRY_WAIT_SECONDS for wait in first)
        assert all(0 <= wait <= 5 * publish_pages.RETRY_WAIT_SECONDS for wait in fifth)
        assert max(fifth) > publish_pages.RETRY_WAIT_SECONDS
        assert len(set(first)) > 1


class TestPublishChange:
    def test_pushes_the_change_to_the_branch(self, remote: Path, tmp_path: Path) -> None:
        # Given
        repo = clone(remote, tmp_path / "pages")

        # When
        publish_pages.publish_change(repo, lambda root: write(root / "a.txt"), "first")
        publish_pages.publish_change(repo, lambda root: write(root / "b.txt"), "second")

        # Then — the second builds on the first; main is untouched
        assert branch_files(remote) == ["a.txt", "b.txt"]
        assert run_git(remote, "log", "--format=%s", publish_pages.BRANCH) == "second\nfirst"
        assert run_git(remote, "log", "--format=%an", "-1", publish_pages.BRANCH) == (
            publish_pages.BOT_NAME
        )
        assert run_git(remote, "ls-tree", "--name-only", "main") == "README.md"

    def test_pushes_nothing_when_nothing_changed(self, remote: Path, tmp_path: Path) -> None:
        # Given
        repo = clone(remote, tmp_path / "pages")
        publish_pages.publish_change(repo, lambda root: write(root / "a.txt"), "first")

        # When
        publish_pages.publish_change(repo, lambda root: write(root / "a.txt"), "again")

        # Then
        assert run_git(remote, "log", "--format=%s", publish_pages.BRANCH) == "first"

    def test_reapplies_the_change_after_losing_a_race(
        self, remote: Path, tmp_path: Path, no_retry_wait: list[float]
    ) -> None:
        # Given another writer pushing between this one's fetch and push, once
        repo = clone(remote, tmp_path / "pages")
        other = clone(remote, tmp_path / "other")
        publish_pages.publish_change(repo, lambda root: write(root / "base.txt"), "base")
        calls: list[int] = []

        def change(root: Path) -> None:
            if not calls:
                publish_pages.publish_change(
                    other, lambda other_root: write(other_root / "theirs.txt"), "theirs"
                )
            calls.append(1)
            write(root / "mine.txt")

        # When
        publish_pages.publish_change(repo, change, "mine")

        # Then — neither write is lost, and the retry waited first
        assert len(calls) == 2
        assert len(no_retry_wait) == 1
        assert branch_files(remote) == ["base.txt", "mine.txt", "theirs.txt"]

    def test_retries_from_an_empty_branch_too(self, remote: Path, tmp_path: Path) -> None:
        # Given another writer creating the branch first
        repo = clone(remote, tmp_path / "pages")
        other = clone(remote, tmp_path / "other")
        calls: list[int] = []

        def change(root: Path) -> None:
            if not calls:
                publish_pages.publish_change(
                    other, lambda other_root: write(other_root / "theirs.txt"), "theirs"
                )
            calls.append(1)
            write(root / "mine.txt")

        # When
        publish_pages.publish_change(repo, change, "mine")

        # Then
        assert branch_files(remote) == ["mine.txt", "theirs.txt"]

    def test_gives_up_after_every_attempt_lost(
        self, remote: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given a push that is always rejected
        repo = clone(remote, tmp_path / "pages")
        monkeypatch.setattr(publish_pages, "commit_and_push", lambda _repo, _message: False)

        # When / Then
        with pytest.raises(PublishError, match="gave up"):
            publish_pages.publish_change(repo, lambda root: write(root / "a.txt"), "first")


class TestSquashBranch:
    def test_replaces_the_history_with_one_commit_of_the_same_tree(
        self, remote: Path, tmp_path: Path
    ) -> None:
        # Given
        repo = clone(remote, tmp_path / "pages")
        publish_pages.publish_change(repo, lambda root: write(root / "a.txt"), "first")
        publish_pages.publish_change(repo, lambda root: write(root / "b.txt"), "second")
        tree = run_git(remote, "rev-parse", f"{publish_pages.BRANCH}^{{tree}}")

        # When
        publish_pages.squash_branch(repo)

        # Then
        assert run_git(remote, "rev-list", "--count", publish_pages.BRANCH) == "1"
        assert run_git(remote, "rev-parse", f"{publish_pages.BRANCH}^{{tree}}") == tree

    def test_does_nothing_without_a_branch(
        self, remote: Path, tmp_path: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        # Given
        repo = clone(remote, tmp_path / "pages")

        # When
        publish_pages.squash_branch(repo)

        # Then
        assert "No gh-pages branch yet" in capsys.readouterr().out

    def test_never_overwrites_a_publish_that_landed_meanwhile(
        self, remote: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given a publish landing after the squash fetched the tip
        repo = clone(remote, tmp_path / "pages")
        other = clone(remote, tmp_path / "other")
        publish_pages.publish_change(repo, lambda root: write(root / "a.txt"), "first")
        check_out = publish_pages.check_out_branch

        def check_out_then_race(path: Path) -> str | None:
            tip = check_out(path)
            if path == repo:
                publish_pages.publish_change(
                    other, lambda other_root: write(other_root / "b.txt"), "second"
                )
            return tip

        monkeypatch.setattr(publish_pages, "check_out_branch", check_out_then_race)

        # When / Then
        with pytest.raises(subprocess.CalledProcessError):
            publish_pages.squash_branch(repo)
        assert branch_files(remote) == ["a.txt", "b.txt"]


class TestMain:
    def test_publishes_and_removes_through_the_command_line(
        self, remote: Path, tmp_path: Path, builds: tuple[Path, Path]
    ) -> None:
        # Given
        repo = clone(remote, tmp_path / "pages")
        docs, examples = builds
        common = ["--pages-dir", str(repo), "--base-url", BASE_URL]
        sites = ["--docs", str(docs), "--examples", str(examples)]

        # When
        published = publish_pages.main(["publish", *common, "--slot", "pr/5", *sites])

        # Then
        assert published == 0
        assert "pr/5/docs/syntax.html" in branch_files(remote)

        # When
        removed = publish_pages.main(["remove", *common, "--slot", "pr/5"])

        # Then
        assert removed == 0
        assert not any(name.startswith("pr/") for name in branch_files(remote))

    def test_publishes_benchmarks_given_on_the_command_line(
        self,
        remote: Path,
        tmp_path: Path,
        builds: tuple[Path, Path],
        benchmarks_dir: Path,
    ) -> None:
        # Given
        repo = clone(remote, tmp_path / "pages")
        docs, examples = builds
        args = [
            "publish",
            "--pages-dir",
            str(repo),
            "--base-url",
            BASE_URL,
            "--slot",
            "main",
            "--docs",
            str(docs),
            "--examples",
            str(examples),
            "--benchmarks-dir",
            str(benchmarks_dir),
        ]

        # When
        status = publish_pages.main(args)

        # Then
        assert status == 0
        files = branch_files(remote)
        assert "main/benchmarks/index.html" in files
        assert "main/benchmarks/cpython/Doc/index.html" in files
        assert "main/benchmarks/sphinx-needs-demo/index.html" in files

    def test_reports_a_refusal_as_one_line(
        self,
        remote: Path,
        tmp_path: Path,
        builds: tuple[Path, Path],
        capsys: pytest.CaptureFixture[str],
    ) -> None:
        # Given a release already published
        repo = clone(remote, tmp_path / "pages")
        docs, examples = builds
        args = [
            "publish",
            "--pages-dir",
            str(repo),
            "--base-url",
            BASE_URL,
            "--slot",
            "v1.0.0",
            "--docs",
            str(docs),
            "--examples",
            str(examples),
        ]
        assert publish_pages.main(args) == 0

        # When
        status = publish_pages.main(args)

        # Then
        assert status == 1
        assert capsys.readouterr().err == (
            "error: v1.0.0 is already published; pass --force to replace a release\n"
        )

    def test_squashes_through_the_command_line(self, remote: Path, tmp_path: Path) -> None:
        # Given
        repo = clone(remote, tmp_path / "pages")
        publish_pages.publish_change(repo, lambda root: write(root / "a.txt"), "first")
        publish_pages.publish_change(repo, lambda root: write(root / "b.txt"), "second")

        # When
        status = publish_pages.main(["squash", "--pages-dir", str(repo)])

        # Then
        assert status == 0
        assert run_git(remote, "rev-list", "--count", publish_pages.BRANCH) == "1"
