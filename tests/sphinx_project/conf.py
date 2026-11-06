"""A Sphinx project `tests/test_init_from_sphinx.sh` migrates into a rinx build.

Every setting here is one `rinx init --from-sphinx` carries over, and every
file below is one a document reads, shows or links.
"""

project = "Sphinx Fixture"
release = "1.0"
exclude_patterns = ["_build", "drafts", "includes"]
rst_prolog = """
.. |project| replace:: Sphinx Fixture
"""
extensions = ["sphinx_needs"]
needs_types = [
    {
        "directive": "req",
        "title": "Requirement",
        "prefix": "R_",
        "color": "#BFD8D2",
        "style": "node",
    }
]
