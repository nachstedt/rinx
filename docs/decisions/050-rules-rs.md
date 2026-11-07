# 50. rules_rs builds the Cargo workspace

## Status

Accepted.

## Context

rinx's rules build the `rinx` binary from source in every build using them:
`rinx_library` and `rinx_site` default to `Label("//:rinx")`, a
`rust_binary`. Until 0.2.0 that went through rules_rust's crate_universe,
which keeps a lockfile of its own, `Cargo.bazel.lock`, beside `Cargo.lock`.
It had to be checked in, since crate_universe refuses to repin across a
module boundary, and every Cargo change needed `CARGO_BAZEL_REPIN=1`, which
Dependabot cannot run.

The review of rinx 0.1.0 in the Bazel Central Registry
(bazelbuild/bazel-central-registry#10748) suggested rules_rs instead, and
pointed out that Bazel is moving towards ignoring toolchains a non-root
module registers (`--incompatible_restrict_non_root_registrations`). rinx
registered `@rust_toolchains//:all` for every build depending on it.

## Decision

### rules_rs, from `Cargo.lock` alone

`crate.from_cargo` reads `Cargo.toml` and `Cargo.lock` directly, so
`Cargo.bazel.lock` is gone and a Cargo change needs no Bazel step beyond the
`MODULE.bazel.lock` update the next build writes. The `BUILD.bazel` files load
rules_rs's `rust_library`/`rust_binary`, which re-export rules_rust's.

### Declare a version, register nothing

`MODULE.bazel` declares `toolchains.toolchain(edition = "2024", version =
"1.98.1")`, the `rust-version` Cargo.toml asks for (rules_rs's default is
older), and registers no toolchain. rules_rs registers its default toolchains
for every build depending on it (hermeticbuild/rules_rs#325) and, across
modules, picks the highest version any of them declares. A root module's own
declaration overrides that, so a user writing Rust keeps control of theirs.
A depending build needs no C toolchain of its own beyond the one Bazel
detects, which `e2e/smoke` checks on Linux and macOS.

### A hub named for rinx, holding only external crates

The hub is `rinx_crates`, not `crates`: rules_rs merges hubs of the same name
across modules, and a depending build may have a `crates` hub of its own.
Each `all_crate_deps` passes `cargo_only = True`. Unlike crate_universe's,
rules_rs's version also returns the workspace's own crates, labelled by
package directory (`//crates/inventory`), while rinx's targets are named
after their crates (`//crates/inventory:rinx_inventory`). The internal
dependencies stay listed explicitly in each `BUILD.bazel`, as before.

## Alternatives

- **Keep crate_universe**, with only the toolchain registration made a dev
  dependency. That answers the registration point but keeps the second
  lockfile and the repin step.
- **Ship prebuilt binaries** behind a `rinx_toolchain_type`, the pattern
  protobuf and rules_img use, with the source build a toolchain a user can
  register instead. A depending build would then need no Rust at all and
  would not compile some 300 crates. It needs release-time builds for every
  platform and checksums in the published module, so it is left for a later
  release.

## Consequences

- Bazel 8.7 or later is required (rules_rs's `bazel_compatibility`).
- Crates resolve for the four triples `MODULE.bazel` lists, now including
  `aarch64-unknown-linux-gnu`. Windows stays out, as `.bcr/presubmit.yml`
  says.
- A new Rust version is a one-line change of the declared version, with
  `Cargo.toml`'s `rust-version` beside it.
