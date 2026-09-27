# Contributing to rinx

Thanks for your interest in rinx. Bug reports, feature requests and pull
requests are all welcome.

## Reporting a bug or asking for a feature

Open an [issue](https://github.com/nachstedt/rinx/issues/new/choose). For a
bug, the most useful thing to include is a minimal `.rst` document (and, if it
matters, the `BUILD.bazel` around it) that shows the problem, together with what
Sphinx does with the same input. Questions and ideas that are not yet concrete
belong in [Discussions](https://github.com/nachstedt/rinx/discussions).

Security problems should not be reported publicly; see
[`SECURITY.md`](SECURITY.md).

## Making a change

For anything larger than a small fix, please open an issue first so the design
can be agreed on before you invest time in it. Significant design decisions are
recorded as ADRs in [`docs/decisions/`](docs/decisions/).

### Building and testing

Day-to-day development uses plain Cargo; the Bazel build wraps the same binary.
The Rust toolchain version is pinned in `rust-toolchain.toml`.

```bash
cargo build
cargo test --workspace
cargo clippy --workspace --tests   # must be warning-free
cargo fmt

bazel build //examples:site        # the example project, end to end
bash tests/test_strict_deps.sh     # build-level tests live in tests/
```

Changing a Cargo dependency requires `CARGO_BAZEL_REPIN=1` on the next Bazel
build, which refreshes the checked-in `Cargo.bazel.lock`.

### What a pull request needs

- `cargo fmt`, `cargo clippy --workspace --tests` without warnings (fix them
  rather than adding `#[allow(...)]`), and `cargo test --workspace` pass.
- `bazel build //examples:site` still succeeds.
- New behaviour comes with tests. Tests are written Given-When-Then, next to
  the code they test.
- A new feature gets an example under [`examples/`](examples/) covering its
  variants, and new `.rst` files are listed in that directory's `BUILD.bazel`.
- If the change adds or fixes coverage of a reStructuredText or Sphinx
  construct, update [`docs/compatibility.rst`](docs/compatibility.rst).
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/)
  (`feat: ...`, `fix: ...`, `docs: ...`, `chore: ...`).

[`CLAUDE.md`](CLAUDE.md) describes the code layout in detail and
[`guidelines.md`](guidelines.md) the conventions the code follows.

How a release is cut is described in
[`docs/dev/releasing.md`](docs/dev/releasing.md).

## Licensing

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in rinx by you, as defined in the Apache-2.0 license, shall be
dual licensed under MIT OR Apache-2.0, without any additional terms or
conditions.

## Code of conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md).
