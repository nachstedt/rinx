r"""Publishes rinx's two sites, one version at a time, to the `gh-pages` branch.

The branch holds every published version side by side, each in its own
directory (docs/decisions/024-versioned-docs.md):

    main/           rebuilt on every merge to main
    vX.Y.Z/         one per release, never overwritten without --force
    pr/<N>/         one per open pull request
    latest/         redirects into the newest release
    docs/, …        redirects for the URLs published before versions existed
    versions.json   the default theme's version switcher reads this

Each version directory holds //docs:site at its root and //examples:site under
`example-site/`. Everything outside the version directories is derived from
them and regenerated on every run, so no run has to know what an earlier one
wrote.

Every workflow writing the branch calls this script, which fetches the branch,
applies one change, commits and pushes. A push that loses a race with another
writer is retried from a fresh fetch rather than rebased: the derived files are
regenerated from the tree anyway, so re-applying the change is always exact.

    python3 scripts/publish_pages.py publish --pages-dir _pages \
        --base-url https://nachstedt.github.io/rinx/ --slot main \
        --docs bazel-bin/docs/site_site_out --examples bazel-bin/examples/site_site_out
    python3 scripts/publish_pages.py remove --pages-dir _pages \
        --base-url https://nachstedt.github.io/rinx/ --slot pr/42
    python3 scripts/publish_pages.py squash --pages-dir _pages

`--pages-dir` is any clone of the repository whose `origin` can be pushed to;
its checkout is replaced. Stdlib-only, since the workflows run it with a bare
`python3`.
"""

from __future__ import annotations

import argparse
import html
import json
import os
import re
import shutil
import subprocess
import sys
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import NotRequired, TypedDict

BRANCH = "gh-pages"
# The local branch the checkout is moved to; never pushed under this name.
WORK_BRANCH = "publish-pages"
BOT_NAME = "github-actions[bot]"
BOT_EMAIL = "41898282+github-actions[bot]@users.noreply.github.com"
PUSH_ATTEMPTS = 5

# Where //examples:site goes inside a version directory, and each site's front
# page, which a version's root redirects to.
EXAMPLES_DIR = "example-site"
DOCS_ENTRY = "docs/index.html"
EXAMPLES_ENTRY = "examples/index.html"

LATEST_DIR = "latest"
PREVIEWS_DIR = "pr"

RELEASE_PATTERN = re.compile(r"v(\d+)\.(\d+)\.(\d+)")
PREVIEW_PATTERN = re.compile(r"pr/([1-9]\d*)")


class SlotKind(Enum):
    """What a version directory holds, which decides how it may be written."""

    MAIN = "main"
    RELEASE = "release"
    PREVIEW = "preview"


@dataclass(frozen=True)
class Slot:
    """A version directory on the branch: `main`, `vX.Y.Z` or `pr/<N>`."""

    kind: SlotKind
    path: str

    @classmethod
    def parse(cls, raw: str) -> Slot:
        """The slot `raw` names.

        Raises:
            ValueError: `raw` is none of the three shapes, so it could name a
                directory the derived files own, or one outside the branch.

        """
        if raw == "main":
            return cls(SlotKind.MAIN, raw)
        if RELEASE_PATTERN.fullmatch(raw):
            return cls(SlotKind.RELEASE, raw)
        if PREVIEW_PATTERN.fullmatch(raw):
            return cls(SlotKind.PREVIEW, raw)
        msg = f"'{raw}' is not a version slot: expected main, vX.Y.Z or pr/<number>"
        raise ValueError(msg)


class VersionEntry(TypedDict):
    """One entry of `versions.json`, in pydata-sphinx-theme's format."""

    version: str
    name: NotRequired[str]
    url: str
    preferred: NotRequired[bool]


class PublishError(Exception):
    """A refusal to change the branch, reported without a traceback."""


def release_key(name: str) -> tuple[int, int, int]:
    """The version numbers of a release directory's name, for ordering."""
    match = RELEASE_PATTERN.fullmatch(name)
    if match is None:
        msg = f"'{name}' is not a release"
        raise ValueError(msg)
    major, minor, patch = (int(part) for part in match.groups())
    return (major, minor, patch)


def list_releases(root: Path) -> list[str]:
    """The release directories on the branch, newest first."""
    names = [
        entry.name
        for entry in root.iterdir()
        if entry.is_dir() and RELEASE_PATTERN.fullmatch(entry.name)
    ]
    return sorted(names, key=release_key, reverse=True)


def versions_manifest(releases: list[str], *, has_main: bool, base_url: str) -> list[VersionEntry]:
    """The switcher's `versions.json`: the newest release, then main, then the rest.

    The newest release is the preferred one, and pull-request previews are
    never listed, which is what makes the switcher call them previews.
    """
    entries: list[VersionEntry] = []
    for position, release in enumerate(releases):
        entry: VersionEntry = {"version": release, "url": f"{base_url}{release}/"}
        if position == 0:
            entry["name"] = f"{release} (latest)"
            entry["preferred"] = True
        entries.append(entry)
    if has_main:
        main: VersionEntry = {
            "version": "main",
            "name": "main (development)",
            "url": f"{base_url}main/",
        }
        entries.insert(1 if releases else 0, main)
    return entries


def redirect_page(target: str) -> str:
    """A page sending its reader to `target`, keeping the `#fragment` they asked for.

    A meta refresh alone would drop the fragment, so a script replaces the
    location first; the refresh and the link cover a reader without scripts.
    """
    attribute = html.escape(target, quote=True)
    return (
        "<!doctype html>\n"
        '<meta charset="utf-8">\n'
        "<title>Redirecting…</title>\n"
        f'<link rel="canonical" href="{attribute}">\n'
        f"<script>location.replace({script_string(target)} + location.hash)</script>\n"
        f'<meta http-equiv="refresh" content="0; url={attribute}">\n'
        f'<p>This page has moved to <a href="{attribute}">{html.escape(target)}</a>.</p>\n'
    )


def script_string(text: str) -> str:
    """`text` as a JavaScript string literal that cannot end the `<script>` holding it."""
    literal = json.dumps(text)
    return literal.replace("<", "\\u003c").replace(">", "\\u003e").replace("&", "\\u0026")


def walk_files(root: Path) -> Iterator[Path]:
    """Every file below `root`, relative to it, in a stable order."""
    for directory, _, files in sorted(os.walk(root)):
        for name in sorted(files):
            yield (Path(directory) / name).relative_to(root)


def write_redirect_tree(source: Path, dest: Path) -> None:
    """Mirrors `source`'s pages into `dest` as redirects to them.

    Each redirect is relative, so the branch works under any host and path. An
    `objects.inv` is copied rather than redirected, since a tool fetching one
    does not follow an HTML redirect; its entries are relative too, so they
    resolve to the redirects here and from there to the real pages.
    """
    for relative in walk_files(source):
        if relative.suffix == ".html":
            page = dest / relative
            target = os.path.relpath(source / relative, page.parent)
            page.parent.mkdir(parents=True, exist_ok=True)
            page.write_text(redirect_page(Path(target).as_posix()), encoding="utf-8")
        elif relative.name == "objects.inv":
            (dest / relative).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source / relative, dest / relative)


def copy_site(source: Path, dest: Path) -> None:
    """Copies every file below `source` to `dest`, contents only.

    Bazel's outputs are read-only, and `shutil.copytree` would carry that over,
    leaving a checkout whose next change cannot delete what this one wrote.
    """
    for relative in walk_files(source):
        (dest / relative).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source / relative, dest / relative)


def place_build(root: Path, slot: Slot, docs: Path, examples: Path, *, force: bool) -> None:
    """Replaces `slot`'s directory with the two built sites.

    Raises:
        PublishError: `slot` is a release that is already published, since a
            reader of an old release must be able to trust it has not moved.

    """
    target = root / slot.path
    if slot.kind is SlotKind.RELEASE and target.exists() and not force:
        msg = f"{slot.path} is already published; pass --force to replace a release"
        raise PublishError(msg)
    for site in (docs, examples):
        # Every rinx site writes an inventory at its root.
        if not (site / "objects.inv").is_file():
            msg = f"{site} is not a built rinx site: it has no objects.inv"
            raise PublishError(msg)
    if target.exists():
        shutil.rmtree(target)
    copy_site(docs, target)
    copy_site(examples, target / EXAMPLES_DIR)
    # A version's root is where the switcher falls back to when a page does not
    # exist in the version switched to.
    (target / "index.html").write_text(redirect_page(DOCS_ENTRY), encoding="utf-8")
    (target / EXAMPLES_DIR / "index.html").write_text(
        redirect_page(EXAMPLES_ENTRY), encoding="utf-8"
    )


def remove_slot(root: Path, slot: Slot, *, force: bool) -> None:
    """Deletes `slot`'s directory, and a `pr/` left empty by it.

    Raises:
        PublishError: `slot` is a release and `force` was not given.

    """
    if slot.kind is SlotKind.RELEASE and not force:
        msg = f"refusing to remove release {slot.path} without --force"
        raise PublishError(msg)
    target = root / slot.path
    if target.exists():
        shutil.rmtree(target)
    parent = target.parent
    if parent != root and parent.exists() and not any(parent.iterdir()):
        parent.rmdir()


def is_version_directory(entry: Path) -> bool:
    """Whether `entry`, at the branch's root, holds published builds rather than derived files."""
    return entry.is_dir() and (
        entry.name in {"main", PREVIEWS_DIR} or RELEASE_PATTERN.fullmatch(entry.name) is not None
    )


def remove_derived_files(root: Path) -> None:
    """Deletes everything at the branch's root but the version directories and `.git`."""
    for entry in root.iterdir():
        if entry.name == ".git" or is_version_directory(entry):
            continue
        if entry.is_dir():
            shutil.rmtree(entry)
        else:
            entry.unlink()


def refresh_derived_files(root: Path, base_url: str) -> None:
    """Regenerates everything at the branch's root from its version directories.

    The newest release is what `latest/`, the site root and the URLs from
    before versions existed lead to — or `main/` until a release is published.
    """
    remove_derived_files(root)

    releases = list_releases(root)
    has_main = (root / "main").is_dir()
    front = releases[0] if releases else ("main" if has_main else None)
    if front is not None:
        if releases:
            write_redirect_tree(root / front, root / LATEST_DIR)
        # The pre-version layout was one version's tree at the root.
        write_redirect_tree(root / front, root)
        (root / "index.html").write_text(redirect_page(f"{front}/{DOCS_ENTRY}"), encoding="utf-8")

    manifest = versions_manifest(releases, has_main=has_main, base_url=base_url)
    (root / "versions.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    # Search engines should send readers to a release, not to work in progress.
    (root / "robots.txt").write_text(
        "User-agent: *\nDisallow: /main/\nDisallow: /pr/\n", encoding="utf-8"
    )
    # Without it, GitHub Pages runs Jekyll, which drops every `_`-prefixed path.
    (root / ".nojekyll").write_text("", encoding="utf-8")


def normalize_base_url(raw: str) -> str:
    """`raw` with exactly one trailing slash, since version urls are appended to it."""
    if not raw.startswith(("https://", "http://")):
        msg = f"--base-url must be an http(s):// URL, got '{raw}'"
        raise PublishError(msg)
    return raw.rstrip("/") + "/"


# ── The branch ────────────────────────────────────────────────────────────────


def git(repo: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    """Runs git in `repo` as the workflows' bot, capturing its output."""
    return subprocess.run(
        ["git", "-c", f"user.name={BOT_NAME}", "-c", f"user.email={BOT_EMAIL}", *args],
        cwd=repo,
        check=check,
        capture_output=True,
        text=True,
    )


def check_out_branch(repo: Path) -> str | None:
    """Replaces `repo`'s checkout with the branch's tip, or an empty tree before its first push.

    Returns the tip's commit, or None when the branch does not exist yet. Only
    the tip is fetched: the branch is storage, and its history is squashed away.
    """
    fetched = git(repo, "fetch", "--depth=1", "origin", BRANCH, check=False)
    if fetched.returncode == 0:
        tip = git(repo, "rev-parse", "FETCH_HEAD").stdout.strip()
        git(repo, "checkout", "--force", "-B", WORK_BRANCH, tip)
        git(repo, "clean", "-fdxq")
        return tip
    if "couldn't find remote ref" not in fetched.stderr:
        raise subprocess.CalledProcessError(
            fetched.returncode, fetched.args, fetched.stdout, fetched.stderr
        )
    start_orphan(repo, WORK_BRANCH)
    git(repo, "rm", "-rfq", "--ignore-unmatch", ".")
    git(repo, "clean", "-fdxq")
    return None


def start_orphan(repo: Path, branch: str) -> None:
    """Makes `branch` a branch with no history yet, keeping the index and checkout.

    A branch of that name left by an earlier attempt in the same clone is
    deleted first. HEAD is detached before that when it names a commit, since
    git refuses to delete the branch checked out — and left alone when it names
    none, as after an attempt that committed nothing.
    """
    if git(repo, "rev-parse", "--verify", "--quiet", "HEAD", check=False).returncode == 0:
        git(repo, "checkout", "--quiet", "--force", "--detach")
    git(repo, "branch", "--delete", "--force", branch, check=False)
    git(repo, "symbolic-ref", "HEAD", f"refs/heads/{branch}")


def commit_and_push(repo: Path, message: str) -> bool:
    """Commits the checkout and pushes it to the branch.

    Returns whether the push was accepted; nothing to commit counts as
    accepted. A rejection means another writer got there first.
    """
    git(repo, "add", "--all")
    if git(repo, "diff", "--cached", "--quiet", check=False).returncode == 0:
        print("Nothing changed.")
        return True
    git(repo, "commit", "--quiet", "--message", message)
    pushed = git(repo, "push", "origin", f"HEAD:refs/heads/{BRANCH}", check=False)
    return pushed.returncode == 0


def publish_change(repo: Path, change: Callable[[Path], object], message: str) -> None:
    """Applies `change` to the branch's tree and pushes it, retrying lost races.

    Raises:
        PublishError: every attempt lost its race.

    """
    for attempt in range(1, PUSH_ATTEMPTS + 1):
        check_out_branch(repo)
        change(repo)
        if commit_and_push(repo, message):
            return
        print(f"Push rejected (attempt {attempt} of {PUSH_ATTEMPTS}); retrying.")
    msg = f"gave up after {PUSH_ATTEMPTS} rejected pushes to {BRANCH}"
    raise PublishError(msg)


def squash_branch(repo: Path) -> None:
    """Replaces the branch's history with one commit of its current tree.

    Pushed with a lease on the tip it was made from, so a publish landing
    meanwhile is never overwritten; the squash just fails and waits for the
    next run.
    """
    tip = check_out_branch(repo)
    if tip is None:
        print(f"No {BRANCH} branch yet.")
        return
    start_orphan(repo, f"{WORK_BRANCH}-squashed")
    git(repo, "commit", "--quiet", "--allow-empty", "--message", f"Squash {BRANCH}")
    git(
        repo,
        "push",
        f"--force-with-lease=refs/heads/{BRANCH}:{tip}",
        "origin",
        f"HEAD:refs/heads/{BRANCH}",
    )


# ── Command line ──────────────────────────────────────────────────────────────


def parse_args(argv: list[str]) -> argparse.Namespace:
    """The command line: `publish`, `remove` or `squash`."""
    parser = argparse.ArgumentParser(
        description="Publish rinx's sites, one version at a time, to the gh-pages branch."
    )
    commands = parser.add_subparsers(dest="command", required=True)

    def add_common(command: argparse.ArgumentParser) -> None:
        command.add_argument("--pages-dir", type=Path, required=True)

    publish = commands.add_parser("publish", help="publish the two sites into a version slot")
    add_common(publish)
    publish.add_argument("--base-url", required=True)
    publish.add_argument("--slot", type=Slot.parse, required=True)
    publish.add_argument("--docs", type=Path, required=True)
    publish.add_argument("--examples", type=Path, required=True)
    publish.add_argument("--force", action="store_true")

    remove = commands.add_parser("remove", help="remove a version slot")
    add_common(remove)
    remove.add_argument("--base-url", required=True)
    remove.add_argument("--slot", type=Slot.parse, required=True)
    remove.add_argument("--force", action="store_true")

    squash = commands.add_parser("squash", help="squash the branch into one commit")
    add_common(squash)
    return parser.parse_args(argv)


def run(args: argparse.Namespace) -> None:
    """Carries out the parsed command."""
    repo: Path = args.pages_dir
    if args.command == "squash":
        squash_branch(repo)
        return

    base_url = normalize_base_url(args.base_url)
    slot: Slot = args.slot
    if args.command == "publish":
        change = publishing(
            slot, args.docs.resolve(), args.examples.resolve(), base_url, force=args.force
        )
        message = f"Publish {slot.path}"
    else:
        change = removing(slot, base_url, force=args.force)
        message = f"Remove {slot.path}"
    publish_change(repo, change, message)


def publishing(
    slot: Slot, docs: Path, examples: Path, base_url: str, *, force: bool
) -> Callable[[Path], object]:
    """The change publishing the two sites into `slot`."""

    def change(root: Path) -> None:
        place_build(root, slot, docs, examples, force=force)
        refresh_derived_files(root, base_url)

    return change


def removing(slot: Slot, base_url: str, *, force: bool) -> Callable[[Path], object]:
    """The change removing `slot`."""

    def change(root: Path) -> None:
        remove_slot(root, slot, force=force)
        refresh_derived_files(root, base_url)

    return change


def main(argv: list[str] | None = None) -> int:
    """Runs the command line, reporting a refusal as one line."""
    try:
        run(parse_args(sys.argv[1:] if argv is None else argv))
    except PublishError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
