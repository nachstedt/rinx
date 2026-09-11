"""
RustySphinxInfo provider.

Carries the depset of compiled .ast files produced by rusty_sphinx_library
and consumed by rusty_sphinx_site for indexing and rendering.
"""

RustySphinxInfo = provider(
    doc = "Carries compiled .ast files from a rusty_sphinx_library target.",
    fields = {
        "ast_files": "A depset of compiled .ast File objects.",
        "diagram_ast_files": "A depset of the .ast File objects whose library set `diagrams = True` — a subset of `ast_files`. The site creates diagram actions for exactly these documents, which is what keeps a library that draws nothing free of the diagram pipeline's cost.",
        "direct_doc_names": "A list of string names for documents directly explicitly exported by just this library.",
        "image_files": "A depset of authored image File objects, declared via rusty_sphinx_library's `images` attribute. These are files the author wrote and referenced with `.. image::`/`.. figure::`, as opposed to the diagrams the site rule compiles, which never pass through a library at all.",
        "embed_sidecars": "A depset of per-document .embeds.json File objects, mapping each `:loading: embed` image to its data: URI. Consumed by the render action of the document it belongs to.",
    },
)
