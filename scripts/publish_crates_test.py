import subprocess
import urllib.error
import urllib.request
from collections.abc import Callable, Sequence
from datetime import UTC, datetime
from email.message import Message
from typing import Self

import pytest

import publish_crates
from publish_crates import Metadata, Package


def package(
    name: str,
    deps: Sequence[str] = (),
    version: str = "0.1.0",
    dev_deps: Sequence[str] = (),
) -> Package:
    """A `cargo metadata` package entry naming its workspace dependencies."""
    return {
        "name": name,
        "version": version,
        "dependencies": [{"name": d, "kind": None} for d in deps]
        + [{"name": d, "kind": "dev"} for d in dev_deps]
        + [{"name": "serde", "kind": None}],
    }


class TestPublishOrder:
    def test_puts_every_crate_after_the_workspace_crates_it_depends_on(self) -> None:
        # Given a chain rinx -> rinx_parser -> rinx_ast, listed backwards
        metadata: Metadata = {
            "packages": [
                package("rinx", ["rinx_parser", "rinx_ast"]),
                package("rinx_parser", ["rinx_ast"]),
                package("rinx_ast"),
            ]
        }

        # When
        order = publish_crates.publish_order(metadata)

        # Then
        assert order == ["rinx_ast", "rinx_parser", "rinx"]

    def test_orders_dev_dependencies_too(self) -> None:
        # Given — a dev-dependency with a version must exist on crates.io
        # before the crate naming it can be published
        metadata: Metadata = {
            "packages": [
                package("rinx_renderer", dev_deps=["rinx_inventory"]),
                package("rinx_inventory"),
            ]
        }

        # When
        order = publish_crates.publish_order(metadata)

        # Then
        assert order == ["rinx_inventory", "rinx_renderer"]

    def test_breaks_ties_by_name_so_the_order_is_stable(self) -> None:
        # Given three independent crates
        metadata: Metadata = {"packages": [package("c"), package("a"), package("b")]}

        # When
        order = publish_crates.publish_order(metadata)

        # Then
        assert order == ["a", "b", "c"]

    def test_refuses_a_dependency_cycle(self) -> None:
        # Given
        metadata: Metadata = {"packages": [package("a", ["b"]), package("b", ["a"])]}

        # When / Then
        with pytest.raises(ValueError, match="dependency cycle among a, b"):
            publish_crates.publish_order(metadata)


class TestRateLimit:
    REFUSAL = (
        "error: failed to publish rinx_toctree v0.1.0 to registry at https://crates.io\n"
        "Caused by:\n"
        "  the remote server responded with an error (status 429 Too Many Requests): "
        "You have published too many new crates in a short period of time. "
        "Please try again after Sat, 26 Sep 2026 22:18:22 GMT and see "
        "https://crates.io/docs/rate-limits for more information on rate limits."
    )

    def test_recognizes_the_new_crates_refusal(self) -> None:
        # Given / When / Then
        assert publish_crates.is_rate_limited(self.REFUSAL)

    def test_does_not_mistake_another_error_for_a_rate_limit(self) -> None:
        # Given / When / Then
        assert not publish_crates.is_rate_limited("error: 403 Forbidden: invalid token")

    def test_waits_until_the_time_crates_io_names_plus_a_margin(self) -> None:
        # Given — ten minutes before the named time
        now = datetime(2026, 9, 26, 22, 8, 22, tzinfo=UTC)

        # When
        wait = publish_crates.seconds_until_retry(self.REFUSAL, now)

        # Then
        assert wait == 600 + publish_crates.RETRY_MARGIN_SECONDS

    def test_waits_a_default_interval_when_no_time_is_named(self) -> None:
        # Given
        now = datetime(2026, 9, 26, 22, 0, 0, tzinfo=UTC)

        # When
        wait = publish_crates.seconds_until_retry("429 Too Many Requests", now)

        # Then
        assert wait == publish_crates.DEFAULT_WAIT_SECONDS

    def test_never_waits_a_negative_time(self) -> None:
        # Given — the named time has already passed
        now = datetime(2026, 9, 26, 23, 0, 0, tzinfo=UTC)

        # When
        wait = publish_crates.seconds_until_retry(self.REFUSAL, now)

        # Then
        assert wait == publish_crates.RETRY_MARGIN_SECONDS


class TestPublishAll:
    @staticmethod
    def run_publish_all(
        crates: list[tuple[str, str]], published: set[str], results: list[tuple[int, str]]
    ) -> tuple[bool, list[str], list[float]]:
        """Runs `publish_all` over fakes, returning what it published and slept."""
        attempts: list[str] = []
        sleeps: list[float] = []
        replies = iter(results)

        def publish(name: str) -> tuple[int, str]:
            attempts.append(name)
            return next(replies)

        ok = publish_crates.publish_all(
            crates,
            publish_crates.Effects(
                is_published=lambda name, _version: name in published,
                publish=publish,
                sleep=sleeps.append,
                now=lambda: datetime(2026, 9, 26, 22, 8, 22, tzinfo=UTC),
                log=lambda _message: None,
            ),
        )
        return ok, attempts, sleeps

    def test_skips_a_crate_already_on_crates_io(self) -> None:
        # Given — a rerun after the first two crates made it
        crates = [("a", "0.1.0"), ("b", "0.1.0"), ("c", "0.1.0")]

        # When
        ok, attempts, _ = self.run_publish_all(crates, published={"a", "b"}, results=[(0, "")])

        # Then
        assert ok
        assert attempts == ["c"]

    def test_waits_out_a_rate_limit_and_retries_the_same_crate(self) -> None:
        # Given
        crates = [("a", "0.1.0")]

        # When
        ok, attempts, sleeps = self.run_publish_all(
            crates,
            published=set(),
            results=[(101, TestRateLimit.REFUSAL), (0, "")],
        )

        # Then
        assert ok
        assert attempts == ["a", "a"]
        assert sleeps == [600 + publish_crates.RETRY_MARGIN_SECONDS]

    def test_stops_at_any_other_failure(self) -> None:
        # Given
        crates = [("a", "0.1.0"), ("b", "0.1.0")]

        # When
        ok, attempts, sleeps = self.run_publish_all(
            crates, published=set(), results=[(101, "error: 403 Forbidden")]
        )

        # Then — `b` is never attempted, and nothing waited
        assert not ok
        assert attempts == ["a"]
        assert sleeps == []


class FakeResponse:
    """What `urllib.request.urlopen` returns, reduced to what is read of it."""

    status = 200

    def __enter__(self) -> Self:
        return self

    def __exit__(self, *_exc: object) -> None:
        pass


class TestCratesIoHas:
    def test_reports_a_version_crates_io_holds(self, monkeypatch: pytest.MonkeyPatch) -> None:
        # Given
        requests: list[urllib.request.Request] = []

        def urlopen(request: urllib.request.Request) -> FakeResponse:
            requests.append(request)
            return FakeResponse()

        monkeypatch.setattr(urllib.request, "urlopen", urlopen)

        # When
        found = publish_crates.crates_io_has("rinx", "0.1.0")

        # Then — crates.io's API refuses requests without a User-Agent
        assert found
        [request] = requests
        assert request.full_url == "https://crates.io/api/v1/crates/rinx/0.1.0"
        assert request.get_header("User-agent") == publish_crates.USER_AGENT

    def test_reports_a_missing_version_as_absent(self, monkeypatch: pytest.MonkeyPatch) -> None:
        # Given
        monkeypatch.setattr(urllib.request, "urlopen", raising(http_error(404)))

        # When
        found = publish_crates.crates_io_has("rinx", "9.9.9")

        # Then
        assert not found

    def test_does_not_mistake_a_server_error_for_absence(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given — guessing "absent" would attempt a publish on every outage
        monkeypatch.setattr(urllib.request, "urlopen", raising(http_error(503)))

        # When / Then
        with pytest.raises(urllib.error.HTTPError, match="503"):
            publish_crates.crates_io_has("rinx", "0.1.0")


def http_error(code: int) -> urllib.error.HTTPError:
    return urllib.error.HTTPError("url", code, "Refused", Message(), None)


def raising(error: Exception) -> Callable[[urllib.request.Request], FakeResponse]:
    """An `urlopen` stand-in that fails every request with `error`."""

    def urlopen(_request: urllib.request.Request) -> FakeResponse:
        raise error

    return urlopen


class TestCargoPublish:
    def test_publishes_one_crate_unverified_and_returns_its_output(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Given
        commands: list[list[str]] = []

        def run(command: list[str], **_kwargs: object) -> subprocess.CompletedProcess[str]:
            commands.append(command)
            return subprocess.CompletedProcess(command, 101, stdout="out\n", stderr="err\n")

        monkeypatch.setattr(subprocess, "run", run)

        # When
        status, output = publish_crates.cargo_publish("rinx_ast")

        # Then
        assert status == 101
        assert output == "out\nerr\n"
        assert commands == [["cargo", "publish", "--no-verify", "-p", "rinx_ast"]]
