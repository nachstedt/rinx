# 24. Versioned documentation and pull-request previews

## Status

Accepted.

## Context

`.github/workflows/pages.yml` builds `//docs:site` and `//examples:site` once
CI has passed on `main`, and publishes them with `actions/deploy-pages`. That
deployment mode replaces the whole site on every deploy, so the site holds
exactly one build. Two things follow:

- a change's rendered pages cannot be seen until it is merged, although
  rendering *is* the product, and
- once `main` moves on, a release's documentation is gone — a user pinned to
  `v0.1.0` reads about features their version does not have.

What other projects do:

- **Read the Docs** is the norm for Sphinx projects: versions, a switcher and
  pull-request previews out of the box. But its build runs in its own
  environment, so Bazel would build from scratch there, without CI's disk
  cache and under RTD's time limits.
- **`mike`** (MkDocs) keeps one directory per version on a `gh-pages`
  branch, plus a `versions.json` and a `latest` alias. Together with
  `pr-preview-action` for previews, it is the common pattern that needs no
  external service.
- **Netlify / Cloudflare Pages** give every deployment its own URL.
  Netlify's credit-based free tier is too tight for a deploy per push.
  Cloudflare's would do, but it adds an account, a token and a second URL,
  and it still needs the version directories assembled by us.

## Decision

### 1. GitHub Pages, served from a `gh-pages` branch laid out like `mike`

```
/                       redirect to latest/docs/index.html
/versions.json          the switcher's data (§4)
/robots.txt             Disallow: /pr/ and /main/ — search engines index releases only
/.nojekyll              without it, Pages' Jekyll drops `_images/`
/latest/                a copy of the newest release
/vX.Y.Z/                one per release, immutable
/main/                  rebuilt on every merge to main
/pr/<N>/                one per open pull request, removed when it closes
/docs/…, /example-site/…  redirect stubs for the URLs published before this
```

Every version directory has today's shape: `//docs:site` at its root and
`//examples:site` under `example-site/`. `latest/` is a copy rather than a
redirect, because a redirect page is not an `objects.inv` (ADR-023): another
site's `inventories` entry should be able to name
`…/latest/docs/objects.inv`.

The branch is storage, not history. A scheduled job squashes it into a single
orphan commit monthly, so pull-request churn does not accumulate. One build is
about 7 MB, almost all of it the example site.

### 2. One script writes the branch

`scripts/publish_pages.py` (stdlib-only, tested like the other scripts) is the
only writer. It publishes or removes one prefix and regenerates
`versions.json` and the redirects. It refuses to overwrite an existing
`vX.Y.Z/` without `--force`, and it retries a rejected push after a rebase.
Every workflow that calls it shares one `concurrency: gh-pages` group with
`cancel-in-progress: false`, so writes are serialized.

| Trigger | Writes |
|---|---|
| CI succeeded on `main` (`pages.yml`) | `main/` |
| the release workflow's `finalize` succeeded | `vX.Y.Z/`, `latest/`, `versions.json` |
| `workflow_dispatch` with a tag | the same, to backfill a release (first: `v0.1.0`) |
| CI succeeded on a pull request | `pr/<N>/`, and a sticky comment linking it |
| a pull request closed | removes `pr/<N>/` |
| monthly | squashes the branch |

### 3. Previews are built unprivileged and published privileged

This is GitHub's recommended split for pull requests from forks. `ci.yml`
already builds both sites on a pull request, with a read-only token. It now
also uploads them, with the pull request's number, as an artifact. A separate
workflow triggered by `workflow_run` downloads that artifact and publishes it.
It holds write permissions, but it never checks out or runs the pull request's
code; it only copies static files. The cleanup workflow uses
`pull_request_target` and is safe for the same reason.

### 4. The version switcher is a feature of rinx's default theme

This is not deploy-time glue, because every rinx user publishing more than one
version needs the same thing. It follows pydata-sphinx-theme's design, which is
the de-facto standard, including its `versions.json` format:

```toml
[version_switcher]
json_url = "https://nachstedt.github.io/rinx/versions.json"
```

```json
[{"version": "v0.2.0", "name": "v0.2.0 (latest)", "url": "https://nachstedt.github.io/rinx/v0.2.0/", "preferred": true},
 {"version": "main", "name": "main (development)", "url": "https://nachstedt.github.io/rinx/main/"}]
```

A URL is not a file path, so this respects ADR-001's rule for `rinx.toml`.
With it set, `templates/default.html` renders a placeholder in the sidebar
header and loads `version_switcher.js`, which `rinx_site` copies next to
`default.css`. Without it, the page is byte-identical to today's.

**The build never learns which version it is.** The script identifies the
current version as the `versions.json` entry whose `url` is a prefix of the
page's address, and a page matching none is an unlisted build — a preview.
pydata-sphinx-theme instead stamps a `version_match` into every page. Here
that would make a commit's `main/` and `pr/<N>/` builds differ in every page,
so neither could reuse the other's cached render action.

The script:

- offers every listed version, keeping the current page's path when switching
  and falling back to the version's root when that page does not exist there;
- shows a banner on anything other than the preferred release: the development
  version, an older release, or an unlisted build;
- shows nothing when `versions.json` cannot be fetched, which is the case for
  a local `bazel build`'s output and in the VS Code preview.

## Consequences

- Once the new workflow is live, the Pages source has to be switched by hand
  to *Deploy from a branch: `gh-pages` /*. The `github-pages` environment is
  no longer used.
- Published releases are permanent. A fix to an old release's docs is an
  explicit `--force` backfill, never a side effect.
- Previews share the origin of the real site. The site sets no cookies and
  has no login, so a pull request's HTML can do nothing there that it could
  not do on any other page.
- Each preview is an ordinary directory, so it can be linked, archived and
  reviewed like the released site. The price is the branch's size, which the
  monthly squash bounds.
