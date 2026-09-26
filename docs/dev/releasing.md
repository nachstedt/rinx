# Releasing rinx

Pushing a tag `vX.Y.Z` releases rinx. `.github/workflows/release.yaml` does the
rest:

1. **ci** runs the full CI (`rust.yml`) again on the tagged commit.
2. **release** runs `.github/workflows/release_prep.sh`, which refuses a tag
   that disagrees with `MODULE.bazel`, `Cargo.toml` or `CHANGELOG.md`. It then
   writes `rinx-vX.Y.Z.tar.gz` (the archive `.bcr/source.template.json` points
   at), attests it, and creates a **draft** GitHub release. The draft's notes
   are the changelog section followed by the `bazel_dep` line.
3. **publish-bcr** opens a draft pull request against the Bazel Central
   Registry from your fork (`publish.yaml`).
4. **crates** publishes every crate to crates.io (`cargo publish --workspace`).
5. **finalize** publishes the draft release, but only once both 3 and 4
   succeeded.

## Cutting a release

1. In one pull request:
   - Set the new version in `Cargo.toml`: `[workspace.package] version`, and
     the `version` of every entry under `[workspace.dependencies]`.
   - Set it in `MODULE.bazel`'s `module(version = ...)`.
   - Run `cargo build` so `Cargo.lock` follows, and
     `CARGO_BAZEL_REPIN=1 bazel build //:rinx` so `Cargo.bazel.lock` does.
   - In `CHANGELOG.md`, rename `## [Unreleased]`'s entries into a dated
     `## [X.Y.Z] - YYYY-MM-DD` section, and update the compare links at the
     bottom.
2. Merge it, then tag the merge commit and push the tag:

   ```bash
   git checkout main && git pull
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

3. Watch the **Release** workflow. When the registry pull request appears in
   `bazelbuild/bazel-central-registry`, mark it **Ready for review**. As the
   maintainer named in `.bcr/metadata.template.json`, that is your approval.
   A BCR maintainer merges it after its presubmit (`.bcr/presubmit.yml`)
   passes.

You can check a release's preparation locally before tagging: with a
throwaway local tag, `.github/workflows/release_prep.sh vX.Y.Z` prints the
release notes or the reason it would refuse.

## If a step fails

- **ci or release:** nothing was published. Fix the problem, delete the tag
  (`git push --delete origin vX.Y.Z`, `git tag -d vX.Y.Z`) and the draft
  release if there is one, and tag again.
- **publish-bcr:** rerun the failed job, or run **Publish to BCR** by hand
  from the Actions tab with the tag.
- **crates:** crates.io publishes are permanent. `cargo publish --workspace`
  skips nothing, so if some crates were published before the failure, publish
  the remaining ones by hand in dependency order (`cargo publish -p <crate>`)
  rather than rerunning the job. Then rerun **finalize**.

## One-time setup

- **BCR fork and token.** Fork `bazelbuild/bazel-central-registry` to
  `nachstedt/bazel-central-registry`. Create a *classic* personal access
  token with the `repo` and `workflow` scopes, and store it as the repository
  secret `BCR_PUBLISH_TOKEN`.
- **crates.io, first release.** Trusted publishing can only be enabled on a
  crate that already exists. So for the first release, create a crates.io API
  token (scopes `publish-new` and `publish-update`) and store it as the
  repository secret `CARGO_REGISTRY_TOKEN`. The **crates** job uses it when it
  is present.
- **crates.io, afterwards.** On crates.io, add a trusted publisher to every
  crate (repository `nachstedt/rinx`, workflow `release.yaml`), then delete
  the `CARGO_REGISTRY_TOKEN` secret. Every later release authenticates through
  GitHub's OIDC token, and no long-lived token is stored.
