// The version switcher of rinx's default template: a menu of every published
// version of a site, and a banner on any page that is not the preferred one.
//
// The build never learns which version it is (docs/decisions/024-versioned-docs.md):
// a page finds itself in `versions.json` by its own address, so a commit built
// for `main/` and for a pull-request preview renders identical bytes. That file
// uses pydata-sphinx-theme's format — a list of `{version, name?, url,
// preferred?}` — so any tool that writes one for that theme writes one for this.
//
// The functions above `start` are pure and tested by version_switcher.test.js
// under `node --test`; everything touching the page is in `start` and below.
"use strict";

// The entries of a parsed `versions.json` that can be offered, with each `url`
// resolved against the file's own address and ending in a slash, so that
// `…/v1/` is never taken for a prefix of `…/v10/`. Anything malformed is
// skipped rather than failing the whole menu.
function readVersions(data, jsonUrl) {
  if (!Array.isArray(data)) {
    return [];
  }
  const versions = [];
  for (const entry of data) {
    if (!entry || typeof entry.version !== "string" || typeof entry.url !== "string") {
      continue;
    }
    let url;
    try {
      url = new URL(entry.url, jsonUrl).href;
    } catch {
      continue;
    }
    versions.push({
      version: entry.version,
      name: typeof entry.name === "string" ? entry.name : entry.version,
      url: url.endsWith("/") ? url : url + "/",
      preferred: entry.preferred === true,
    });
  }
  return versions;
}

// The version whose `url` is the longest prefix of `href`, or null for a page
// no entry covers — a build that was published without being listed, such as
// a pull-request preview.
function matchVersion(versions, href) {
  let best = null;
  for (const version of versions) {
    if (href.startsWith(version.url) && (!best || version.url.length > best.url.length)) {
      best = version;
    }
  }
  return best;
}

// Where the page at `href` lives in `target`: the same path below the
// version's root, or the root itself when `href` belongs to no listed version.
function targetUrl(current, target, href) {
  if (!current) {
    return target.url;
  }
  return target.url + href.slice(current.url.length);
}

// Whether a version names a release (`1.2`, `v0.3.0`) rather than a branch.
function isRelease(version) {
  return /^v?\d+(\.\d+)*/.test(version.version);
}

// Which banner the page needs, or null for none: nothing is said on the
// preferred version, nor when there is no preferred version to send a reader to.
function bannerKind(versions, current) {
  const preferred = versions.find((version) => version.preferred);
  if (!preferred || current === preferred) {
    return null;
  }
  if (!current) {
    return "unlisted";
  }
  return isRelease(current) ? "old" : "development";
}

// The banner's sentence, before its link to the preferred version.
function bannerText(kind, current) {
  switch (kind) {
    case "unlisted":
      return "This is a preview build, not a published version of this documentation.";
    case "old":
      return `This is the documentation of an older release, ${current.name}.`;
    case "development":
      return `This is the documentation of the unreleased development version, ${current.name}.`;
    default:
      return "";
  }
}

// Opens `url`, or the version root `fallback` when that page does not exist
// there — a page added after an old release has no counterpart in it.
async function navigate(url, fallback) {
  try {
    const response = await fetch(url, { method: "HEAD" });
    window.location.assign(response.ok ? url : fallback);
  } catch {
    window.location.assign(fallback);
  }
}

function renderMenu(container, versions, current, href) {
  const select = document.createElement("select");
  select.setAttribute("aria-label", "Documentation version");
  if (!current) {
    const unlisted = new Option("Preview", "", true, true);
    unlisted.disabled = true;
    select.add(unlisted);
  }
  versions.forEach((version, position) => {
    select.add(new Option(version.name, String(position), false, version === current));
  });
  select.addEventListener("change", () => {
    const target = versions[Number(select.value)];
    navigate(targetUrl(current, target, href), target.url);
  });
  container.append(select);
  container.hidden = false;
}

function renderBanner(versions, current, href) {
  const kind = bannerKind(versions, current);
  const main = document.querySelector("main");
  if (!kind || !main) {
    return;
  }
  const preferred = versions.find((version) => version.preferred);
  const banner = document.createElement("div");
  banner.className = `version-banner version-banner-${kind}`;
  banner.setAttribute("role", "note");
  const link = document.createElement("a");
  link.href = targetUrl(current, preferred, href);
  link.textContent = `Go to the latest release, ${preferred.name}.`;
  link.addEventListener("click", (event) => {
    event.preventDefault();
    navigate(link.href, preferred.url);
  });
  banner.append(bannerText(kind, current), " ", link);
  main.prepend(banner);
}

// Fetches the version list and draws the menu and banner. Silent when the
// list cannot be had — a local build opened from disk, an editor preview —
// since a page without its switcher is still a whole page.
async function start() {
  const container = document.querySelector(".version-switcher[data-json-url]");
  if (!container) {
    return;
  }
  const jsonUrl = new URL(container.dataset.jsonUrl, window.location.href).href;
  let versions;
  try {
    const response = await fetch(jsonUrl, { cache: "no-cache" });
    if (!response.ok) {
      return;
    }
    versions = readVersions(await response.json(), jsonUrl);
  } catch {
    return;
  }
  if (versions.length === 0) {
    return;
  }
  const href = window.location.href;
  const current = matchVersion(versions, href);
  renderMenu(container, versions, current, href);
  renderBanner(versions, current, href);
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = { readVersions, matchVersion, targetUrl, isRelease, bannerKind, bannerText };
} else {
  start();
}
