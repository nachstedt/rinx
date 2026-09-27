"""Publishes the workspace's crates to crates.io, resumably.

Run by the `crates` job of .github/workflows/release.yaml, from the tagged
commit, instead of `cargo publish --workspace`, which cannot finish a release
that failed halfway: it stops at the first crate already published, and a
version can never be published twice.

So this script:

- derives the publish order from `cargo metadata`, so a crate added later
  needs no list updated anywhere;
- skips every crate whose version is already on crates.io, so rerunning the
  job resumes where the last run stopped;
- waits out crates.io's rate limit on *new* crates (a burst of five, then one
  every ten minutes) until the time crates.io names, and retries;
- publishes with `--no-verify`. The `ci` job has already built every crate from
  the exact tarball crates.io receives (`cargo package --workspace`) on the same
  commit, and verifying again would compile the workspace fourteen times over,
  longer than the half hour a trusted-publishing token lives.

Runnable by hand too, from a clean checkout of the tag, with a crates.io token
in `CARGO_REGISTRY_TOKEN` or from `cargo login`:

    python3 scripts/publish_crates.py
"""

import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from datetime import UTC, datetime
from email.utils import parsedate_to_datetime
from http import HTTPStatus
from typing import TypedDict

# crates.io's refusal names the moment the next new crate is allowed; a little
# after it is safe from clock skew between the runner and crates.io.
RETRY_MARGIN_SECONDS = 30
# Used when a refusal names no time: crates.io's refill interval.
DEFAULT_WAIT_SECONDS = 600

USER_AGENT = "rinx-release (https://github.com/nachstedt/rinx)"


class Dependency(TypedDict):
    """One entry of a package's `dependencies` in `cargo metadata`."""

    name: str
    # None for a normal dependency, else "dev" or "build".
    kind: str | None


class Package(TypedDict):
    """The part of a `cargo metadata` package this script reads."""

    name: str
    version: str
    dependencies: list[Dependency]


class Metadata(TypedDict):
    """The part of `cargo metadata --no-deps` output this script reads."""

    packages: list[Package]


def publish_order(metadata: Metadata) -> list[str]:
    """The workspace packages' names, each after every workspace crate it needs.

    Dev-dependencies count too: one declared with a version stays in the
    published manifest, so crates.io needs it to exist first. Ties are broken
    by name, so the order is the same on every run.
    """
    packages = {p["name"]: p for p in metadata["packages"]}
    needs = {
        name: {d["name"] for d in p["dependencies"] if d["name"] in packages}
        for name, p in packages.items()
    }
    order: list[str] = []
    placed: set[str] = set()
    while len(order) < len(packages):
        ready = sorted(n for n in packages if n not in placed and needs[n] <= placed)
        if not ready:
            stuck = sorted(n for n in packages if n not in placed)
            message = f"dependency cycle among {', '.join(stuck)}"
            raise ValueError(message)
        order.extend(ready)
        placed.update(ready)
    return order


def is_rate_limited(output: str) -> bool:
    """Whether `cargo publish` failed on crates.io's rate limit."""
    return "429" in output or "too many new crates" in output.lower()


def seconds_until_retry(output: str, now: datetime) -> int:
    """How long to wait after a rate-limit refusal before trying again."""
    match = re.search(r"try again after ([^.]*?GMT)", output)
    if not match:
        return DEFAULT_WAIT_SECONDS
    retry_at = parsedate_to_datetime(match.group(1))
    remaining = (retry_at - now).total_seconds()
    return max(0, int(remaining)) + RETRY_MARGIN_SECONDS


@dataclass(frozen=True)
class Effects:
    """The side effects `publish_all` performs, injected so it is testable without crates.io."""

    is_published: Callable[[str, str], bool]
    # Publishes one crate by name, returning its exit status and output.
    publish: Callable[[str], tuple[int, str]]
    sleep: Callable[[float], None]
    now: Callable[[], datetime]
    log: Callable[[str], None]


def publish_all(crates: Iterable[tuple[str, str]], effects: Effects) -> bool:
    """Publishes each `(name, version)` in order, returning whether all made it."""
    log = effects.log
    for name, version in crates:
        if effects.is_published(name, version):
            log(f"skip {name} {version}: already on crates.io")
            continue
        while True:
            log(f"publishing {name} {version}")
            status, output = effects.publish(name)
            if status == 0:
                log(f"published {name} {version}")
                break
            if is_rate_limited(output):
                wait = seconds_until_retry(output, effects.now())
                log(f"{name}: crates.io rate limit, retrying in {wait}s")
                effects.sleep(wait)
                continue
            log(f"failed to publish {name} {version}:\n{output}")
            return False
    return True


def crates_io_has(name: str, version: str) -> bool:
    """Whether crates.io already holds `name` at `version`."""
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}/{version}",
        headers={"User-Agent": USER_AGENT},
    )
    try:
        # The URL is the fixed https crates.io API, not caller-supplied.
        with urllib.request.urlopen(request) as response:  # noqa: S310
            return response.status == HTTPStatus.OK
    except urllib.error.HTTPError as error:
        if error.code == HTTPStatus.NOT_FOUND:
            return False
        raise


def cargo_publish(name: str) -> tuple[int, str]:
    """Runs `cargo publish` for one crate, returning its status and output."""
    result = subprocess.run(
        ["cargo", "publish", "--no-verify", "-p", name],
        capture_output=True,
        text=True,
        check=False,
    )
    return result.returncode, result.stdout + result.stderr


def main() -> int:
    """Publish every workspace crate not yet on crates.io; exit non-zero on a failure."""
    metadata: Metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    )
    versions = {p["name"]: p["version"] for p in metadata["packages"]}
    crates = [(name, versions[name]) for name in publish_order(metadata)]
    ok = publish_all(
        crates,
        Effects(
            is_published=crates_io_has,
            publish=cargo_publish,
            sleep=time.sleep,
            now=lambda: datetime.now(UTC),
            log=lambda message: print(message, flush=True),
        ),
    )
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
