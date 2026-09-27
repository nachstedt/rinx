// Tests for assets/version_switcher.js's pure functions: `bazel test //tests/js:all`.
"use strict";

const assert = require("node:assert/strict");
const { test } = require("node:test");
const {
  readVersions,
  matchVersion,
  targetUrl,
  isRelease,
  bannerKind,
  bannerText,
} = require("./version_switcher.js");

const JSON_URL = "https://example.org/rinx/versions.json";

function versions() {
  return readVersions(
    [
      { version: "v1.0.0", name: "v1.0.0 (latest)", url: "https://example.org/rinx/v1.0.0/", preferred: true },
      { version: "main", url: "/rinx/main" },
      { version: "v0.1.0", url: "v0.1.0/" },
    ],
    JSON_URL,
  );
}

test("readVersions resolves each url against the file's address, ending in a slash", () => {
  // Given / When
  const read = versions();

  // Then
  assert.deepEqual(
    read.map((version) => version.url),
    [
      "https://example.org/rinx/v1.0.0/",
      "https://example.org/rinx/main/",
      "https://example.org/rinx/v0.1.0/",
    ],
  );
});

test("readVersions names an entry by its version when it has no name", () => {
  // Given / When
  const main = versions()[1];

  // Then
  assert.equal(main.name, "main");
  assert.equal(main.preferred, false);
});

test("readVersions skips malformed entries and non-list data", () => {
  // Given
  const data = [null, { version: 1, url: "/x/" }, { version: "a" }, { version: "ok", url: "/ok/" }];

  // When / Then
  assert.deepEqual(
    readVersions(data, JSON_URL).map((version) => version.version),
    ["ok"],
  );
  assert.deepEqual(readVersions({ versions: [] }, JSON_URL), []);
});

test("matchVersion finds the version a page lives under", () => {
  // Given
  const all = versions();

  // When
  const current = matchVersion(all, "https://example.org/rinx/main/docs/syntax.html#tables");

  // Then
  assert.equal(current.version, "main");
});

test("matchVersion never takes v1 for a prefix of v10", () => {
  // Given
  const all = readVersions(
    [
      { version: "v1", url: "/v1/" },
      { version: "v10", url: "/v10" },
    ],
    JSON_URL,
  );

  // When
  const current = matchVersion(all, "https://example.org/v10/index.html");

  // Then
  assert.equal(current.version, "v10");
});

test("matchVersion prefers the longest matching url", () => {
  // Given — a version nested inside another's directory
  const all = readVersions(
    [
      { version: "root", url: "/" },
      { version: "main", url: "/main/" },
    ],
    JSON_URL,
  );

  // When / Then
  assert.equal(matchVersion(all, "https://example.org/main/a.html").version, "main");
});

test("matchVersion returns null for an unlisted build", () => {
  // Given / When / Then
  assert.equal(matchVersion(versions(), "https://example.org/rinx/pr/42/docs/index.html"), null);
});

test("targetUrl keeps the page's path below the target version", () => {
  // Given
  const [latest, main] = versions();

  // When
  const url = targetUrl(main, latest, "https://example.org/rinx/main/docs/syntax.html");

  // Then
  assert.equal(url, "https://example.org/rinx/v1.0.0/docs/syntax.html");
});

test("targetUrl falls back to the version root for an unlisted page", () => {
  // Given
  const [latest] = versions();

  // When / Then
  assert.equal(targetUrl(null, latest, "https://example.org/rinx/pr/1/a.html"), latest.url);
});

test("isRelease tells release numbers from branch names", () => {
  // Given / When / Then
  assert.equal(isRelease({ version: "v0.1.0" }), true);
  assert.equal(isRelease({ version: "2.3" }), true);
  assert.equal(isRelease({ version: "main" }), false);
  assert.equal(isRelease({ version: "latest" }), false);
});

test("bannerKind says nothing on the preferred version", () => {
  // Given
  const all = versions();

  // When / Then
  assert.equal(bannerKind(all, all[0]), null);
});

test("bannerKind names the development version, an old release and an unlisted build", () => {
  // Given
  const all = versions();
  const [, main, old] = all;

  // When / Then
  assert.equal(bannerKind(all, main), "development");
  assert.equal(bannerKind(all, old), "old");
  assert.equal(bannerKind(all, null), "unlisted");
});

test("bannerKind says nothing when no version is preferred", () => {
  // Given — nowhere to send a reader
  const all = readVersions([{ version: "main", url: "/main/" }], JSON_URL);

  // When / Then
  assert.equal(bannerKind(all, null), null);
  assert.equal(bannerKind(all, all[0]), null);
});

test("bannerText names the version being read", () => {
  // Given
  const [, main, old] = versions();

  // When / Then
  assert.match(bannerText("old", old), /older release, v0\.1\.0/);
  assert.match(bannerText("development", main), /development version, main/);
  assert.match(bannerText("unlisted", null), /preview build/);
  assert.equal(bannerText(null, null), "");
});
