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
    },
)
