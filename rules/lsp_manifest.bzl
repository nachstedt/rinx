"""
Helpers for the language server's manifest (ADR-038 §2).

`rinx_library` and `rinx_site` each describe what they know as data the
server reads, through these two encodings, so both name a label and a file
the same way.
"""

def lsp_label(label):
    """A label as an author writes it: `//pkg:name` in the main repository.

    `str(label)` spells a main-repository label `@@//pkg:name` under bzlmod,
    which is no label anyone typed.
    """
    if not label.workspace_name:
        return "//%s:%s" % (label.package, label.name)
    return str(label)

def lsp_file(file):
    """A file the server reads, by where it is found.

    A source file is named by its path in its repository, which is where an
    author opens it; a generated one by its path under the execution root,
    the only place it exists.
    """
    if file.is_source:
        return {"source": file.short_path}
    return {"generated": file.path}
