# Vendored font

`DejaVuSans.ttf` (DejaVu Fonts 2.34) is checked in because the chart backend
needs font *metrics* to lay a label out, and it must find them without asking
the machine it is building on.

`plotters` is built here with `default-features = false` and the `ab_glyph`
feature precisely so that no system font lookup happens: the `ttf`/`font-kit`
path would resolve whatever fonts the build host happens to have installed,
which would make a rendered page — a Bazel cache artefact — differ between
machines. `ab_glyph` embeds no font of its own, so without this file every
draw fails with `FontError(FontUnavailable)`.

Licence: `LICENSE`, the Bitstream Vera licence. It permits redistribution
(including as part of a larger software package) provided the notice travels
with the font, which is what that file is for.
