import subprocess
import unittest
import urllib.error
from unittest import mock
from datetime import datetime, timezone

import publish_crates


def package(name, deps=(), version="0.1.0", dev_deps=()):
    """A `cargo metadata` package entry naming its workspace dependencies."""
    return {
        "name": name,
        "version": version,
        "dependencies": [{"name": d, "kind": None} for d in deps]
        + [{"name": d, "kind": "dev"} for d in dev_deps]
        + [{"name": "serde", "kind": None}],
    }


class PublishOrderTest(unittest.TestCase):
    def test_puts_every_crate_after_the_workspace_crates_it_depends_on(self):
        # Given a chain rinx -> rinx_parser -> rinx_ast, listed backwards
        metadata = {
            "packages": [
                package("rinx", ["rinx_parser", "rinx_ast"]),
                package("rinx_parser", ["rinx_ast"]),
                package("rinx_ast"),
            ]
        }

        # When
        order = publish_crates.publish_order(metadata)

        # Then
        self.assertEqual(order, ["rinx_ast", "rinx_parser", "rinx"])

    def test_orders_dev_dependencies_too(self):
        # Given — a dev-dependency with a version must exist on crates.io
        # before the crate naming it can be published
        metadata = {
            "packages": [
                package("rinx_renderer", dev_deps=["rinx_inventory"]),
                package("rinx_inventory"),
            ]
        }

        # When
        order = publish_crates.publish_order(metadata)

        # Then
        self.assertEqual(order, ["rinx_inventory", "rinx_renderer"])

    def test_breaks_ties_by_name_so_the_order_is_stable(self):
        # Given three independent crates
        metadata = {"packages": [package("c"), package("a"), package("b")]}

        # When
        order = publish_crates.publish_order(metadata)

        # Then
        self.assertEqual(order, ["a", "b", "c"])

    def test_refuses_a_dependency_cycle(self):
        # Given
        metadata = {"packages": [package("a", ["b"]), package("b", ["a"])]}

        # When / Then
        with self.assertRaises(ValueError):
            publish_crates.publish_order(metadata)


class RateLimitTest(unittest.TestCase):
    REFUSAL = (
        "error: failed to publish rinx_toctree v0.1.0 to registry at https://crates.io\n"
        "Caused by:\n"
        "  the remote server responded with an error (status 429 Too Many Requests): "
        "You have published too many new crates in a short period of time. "
        "Please try again after Sat, 26 Sep 2026 22:18:22 GMT and see "
        "https://crates.io/docs/rate-limits for more information on rate limits."
    )

    def test_recognizes_the_new_crates_refusal(self):
        # Given / When / Then
        self.assertTrue(publish_crates.is_rate_limited(self.REFUSAL))

    def test_does_not_mistake_another_error_for_a_rate_limit(self):
        # Given / When / Then
        self.assertFalse(
            publish_crates.is_rate_limited("error: 403 Forbidden: invalid token")
        )

    def test_waits_until_the_time_crates_io_names_plus_a_margin(self):
        # Given — ten minutes before the named time
        now = datetime(2026, 9, 26, 22, 8, 22, tzinfo=timezone.utc)

        # When
        wait = publish_crates.seconds_until_retry(self.REFUSAL, now)

        # Then
        self.assertEqual(wait, 600 + publish_crates.RETRY_MARGIN_SECONDS)

    def test_waits_a_default_interval_when_no_time_is_named(self):
        # Given
        now = datetime(2026, 9, 26, 22, 0, 0, tzinfo=timezone.utc)

        # When
        wait = publish_crates.seconds_until_retry("429 Too Many Requests", now)

        # Then
        self.assertEqual(wait, publish_crates.DEFAULT_WAIT_SECONDS)

    def test_never_waits_a_negative_time(self):
        # Given — the named time has already passed
        now = datetime(2026, 9, 26, 23, 0, 0, tzinfo=timezone.utc)

        # When
        wait = publish_crates.seconds_until_retry(self.REFUSAL, now)

        # Then
        self.assertEqual(wait, publish_crates.RETRY_MARGIN_SECONDS)


class PublishAllTest(unittest.TestCase):
    def run_publish_all(self, crates, published, results):
        """Runs `publish_all` over fakes, returning what it published and slept."""
        attempts, sleeps = [], []
        replies = iter(results)

        def publish(name):
            attempts.append(name)
            return next(replies)

        ok = publish_crates.publish_all(
            crates,
            is_published=lambda name, version: name in published,
            publish=publish,
            sleep=sleeps.append,
            now=lambda: datetime(2026, 9, 26, 22, 8, 22, tzinfo=timezone.utc),
            log=lambda message: None,
        )
        return ok, attempts, sleeps

    def test_skips_a_crate_already_on_crates_io(self):
        # Given — a rerun after the first two crates made it
        crates = [("a", "0.1.0"), ("b", "0.1.0"), ("c", "0.1.0")]

        # When
        ok, attempts, _ = self.run_publish_all(
            crates, published={"a", "b"}, results=[(0, "")]
        )

        # Then
        self.assertTrue(ok)
        self.assertEqual(attempts, ["c"])

    def test_waits_out_a_rate_limit_and_retries_the_same_crate(self):
        # Given
        crates = [("a", "0.1.0")]

        # When
        ok, attempts, sleeps = self.run_publish_all(
            crates,
            published=set(),
            results=[(101, RateLimitTest.REFUSAL), (0, "")],
        )

        # Then
        self.assertTrue(ok)
        self.assertEqual(attempts, ["a", "a"])
        self.assertEqual(sleeps, [600 + publish_crates.RETRY_MARGIN_SECONDS])

    def test_stops_at_any_other_failure(self):
        # Given
        crates = [("a", "0.1.0"), ("b", "0.1.0")]

        # When
        ok, attempts, sleeps = self.run_publish_all(
            crates, published=set(), results=[(101, "error: 403 Forbidden")]
        )

        # Then — `b` is never attempted, and nothing waited
        self.assertFalse(ok)
        self.assertEqual(attempts, ["a"])
        self.assertEqual(sleeps, [])


class CratesIoHasTest(unittest.TestCase):
    def test_reports_a_version_crates_io_holds(self):
        # Given
        response = mock.MagicMock(status=200)
        response.__enter__.return_value = response

        # When
        with mock.patch("urllib.request.urlopen", return_value=response) as urlopen:
            found = publish_crates.crates_io_has("rinx", "0.1.0")

        # Then — crates.io's API refuses requests without a User-Agent
        self.assertTrue(found)
        request = urlopen.call_args.args[0]
        self.assertEqual(request.full_url, "https://crates.io/api/v1/crates/rinx/0.1.0")
        self.assertEqual(request.get_header("User-agent"), publish_crates.USER_AGENT)

    def test_reports_a_missing_version_as_absent(self):
        # Given
        missing = urllib.error.HTTPError("url", 404, "Not Found", None, None)

        # When
        with mock.patch("urllib.request.urlopen", side_effect=missing):
            found = publish_crates.crates_io_has("rinx", "9.9.9")

        # Then
        self.assertFalse(found)

    def test_does_not_mistake_a_server_error_for_absence(self):
        # Given — guessing "absent" would attempt a publish on every outage
        outage = urllib.error.HTTPError("url", 503, "Unavailable", None, None)

        # When / Then
        with mock.patch("urllib.request.urlopen", side_effect=outage):
            with self.assertRaises(urllib.error.HTTPError):
                publish_crates.crates_io_has("rinx", "0.1.0")


class CargoPublishTest(unittest.TestCase):
    def test_publishes_one_crate_unverified_and_returns_its_output(self):
        # Given
        done = subprocess.CompletedProcess([], 101, stdout="out\n", stderr="err\n")

        # When
        with mock.patch("subprocess.run", return_value=done) as run:
            status, output = publish_crates.cargo_publish("rinx_ast")

        # Then
        self.assertEqual(status, 101)
        self.assertEqual(output, "out\nerr\n")
        self.assertEqual(
            run.call_args.args[0],
            ["cargo", "publish", "--no-verify", "-p", "rinx_ast"],
        )


if __name__ == "__main__":
    unittest.main()
