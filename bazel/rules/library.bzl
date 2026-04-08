"""
rusty_sphinx_library rule.

Parses every .rst source file (Phase 1) into a .ast JSON file using the
rusty-sphinx worker binary and propagates them via RustySphinxInfo.
"""

load("//:providers.bzl", "RustySphinxInfo")

def _rusty_sphinx_library_impl(ctx):
    worker = ctx.executable._worker
    ast_files = []

    for src in ctx.files.srcs:
        # Declare a .ast output alongside each .rst source.
        ast_out = ctx.actions.declare_file(src.basename.removesuffix(".rst") + ".ast", sibling = src)

        ctx.actions.run(
            executable = worker,
            arguments = [
                "parse",
                "--input", src.path,
                "--output", ast_out.path,
            ],
            inputs = [src],
            outputs = [ast_out],
            mnemonic = "RustySphinxParse",
            progress_message = "Parsing %s" % src.short_path,
        )
        ast_files.append(ast_out)

    # Collect .ast files from deps (other rusty_sphinx_library targets).
    transitive = [dep[RustySphinxInfo].ast_files for dep in ctx.attr.deps]

    return [
        DefaultInfo(files = depset(ast_files)),
        RustySphinxInfo(
            ast_files = depset(ast_files, transitive = transitive),
        ),
    ]

rusty_sphinx_library = rule(
    implementation = _rusty_sphinx_library_impl,
    attrs = {
        "srcs": attr.label_list(
            allow_files = [".rst"],
            doc = "reStructuredText source files owned by this library.",
        ),
        "deps": attr.label_list(
            providers = [RustySphinxInfo],
            doc = "Other rusty_sphinx_library targets that are cross-referenced from these sources.",
        ),
        "_worker": attr.label(
            default = Label("@@//:rusty_sphinx_worker"),
            executable = True,
            cfg = "exec",
            doc = "The rusty-sphinx binary. Defaults to //:rusty_sphinx_worker in the consuming workspace.",
        ),
    },
    doc = """
Parses a set of reStructuredText files into AST files (Phase 1).

Each .rst file becomes an independently cacheable .ast file.  Declare
cross-project link dependencies in `deps`; omit them to allow fast local
preview builds that skip link verification.
""",
)
