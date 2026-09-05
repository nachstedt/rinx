"""
RustySphinxInfo provider.

Carries the depset of compiled .ast files produced by rusty_sphinx_library
and consumed by rusty_sphinx_site for indexing and rendering.
"""

RustySphinxInfo = provider(
    doc = "Carries compiled .ast files from a rusty_sphinx_library target.",
    fields = {
        "ast_files": "A depset of compiled .ast File objects.",
        "direct_doc_names": "A list of string names for documents directly explicitly exported by just this library.",
        "svg_dirs": "A depset of TreeArtifacts containing rendered PlantUML SVGs.",
        "image_files": "A depset of authored image File objects, declared via rusty_sphinx_library's `images` attribute. Distinct from `svg_dirs`, which holds diagrams this build generated: these are files the author wrote and referenced with `.. image::`/`.. figure::`.",
        "embed_sidecars": "A depset of per-document .embeds.json File objects, mapping each `:loading: embed` image to its data: URI. Consumed by the render action of the document it belongs to.",
    },
)
