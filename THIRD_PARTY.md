# Third-Party Notices

rinx links a small number of third-party crates. Most carry permissive
licences requiring nothing beyond the usual attribution; the entries below have
obligations worth stating explicitly.

## Syntax highlighting grammars and themes (`two-face`)

Syntax highlighting is performed by [`syntect`](https://crates.io/crates/syntect)
(MIT) over the grammar and theme set bundled by
[`two-face`](https://crates.io/crates/two-face) (MIT OR Apache-2.0). See
`docs/decisions/006-syntax-highlighting.md` for why this backend was chosen.

`two-face` re-distributes assets collected by
[`bat`](https://github.com/sharkdp/bat): **66 syntax definitions** and
**12 themes**, contributed by many authors under a mix of licences (MIT,
Apache-2.0, and others). Those licences require their notices to be reproduced
by anything that redistributes the assets.

The full, authoritative listing — every syntax and theme with its licence text —
is published by `two-face` and is reproduced here by reference:

<https://codeberg.org/CosmicHarper/two-face/src/tag/v0.5.2+bat-0.26.1/generated/acknowledgements_full.md>

The same listing is available at run time, without network access, from the
version actually compiled in:

```rust
let listing = two_face::acknowledgement::listing();
for license in listing.for_syntaxes() { /* … */ }
for license in listing.for_themes() { /* … */ }
```

Pin the URL above to the `two-face` version in `Cargo.toml` when that dependency
is upgraded, since the asset set changes with it.

## GitHub octicons (`octicons-pack`)

`.. dropdown::`'s `:icon:` option, and the chevron marking a dropdown's
open/closed state, are drawn with icons from
[`octicons-pack`](https://crates.io/crates/octicons-pack) (MIT) — a generated
redistribution of the [`@primer/octicons`](https://github.com/primer/octicons)
npm package (MIT), whose SVG data it embeds as string constants.

Two consequences worth knowing. The icon *set* is GitHub's, so its own MIT
notice travels with any page this build renders an icon into; and the set is
versioned by the crate rather than by us, so upgrading the dependency can change
which `:icon:` names a document may use — see `spec_gaps.md` for the names this
differs on from sphinx-design's own pinned copy.

## Everything else

The remaining dependencies are listed in each crate's `Cargo.toml` and resolved
in `Cargo.lock`. All are MIT, Apache-2.0, or dual-licensed under both.
