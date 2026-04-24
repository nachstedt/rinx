"""
rusty_sphinx_library rule.

Parses every .rst source file (Phase 1) into a .ast JSON file using the
rusty-sphinx worker binary and propagates them via RustySphinxInfo.
"""

load("//:providers.bzl", "RustySphinxInfo")

def _rusty_sphinx_library_impl(ctx):
    worker = ctx.executable._worker
    ast_files = []

    local_doc_names = [src.short_path.removesuffix(".rst") for src in ctx.files.srcs]
    
    allowed_doc_names_list = list(local_doc_names)
    for dep in ctx.attr.deps:
        allowed_doc_names_list.extend(dep[RustySphinxInfo].direct_doc_names)
    
    allowed_doc_names = depset(allowed_doc_names_list)

    for src in ctx.files.srcs:
        # Phase 1: parse
        ast_raw = ctx.actions.declare_file(src.basename.removesuffix(".rst") + ".ast.raw", sibling = src)

        ctx.actions.run(
            executable = worker,
            arguments = [
                "parse",
                "--input", src.path,
                "--output", ast_raw.path,
            ],
            inputs = [src],
            outputs = [ast_raw],
            mnemonic = "RustySphinxParse",
            progress_message = "Parsing %s" % src.short_path,
        )
        # Validate the AST's toctree entries (Phase 1.5)
        ast_out = ctx.actions.declare_file(src.basename.removesuffix(".rst") + ".ast", sibling = src)
        args = ctx.actions.args()
        args.add("validate_toctree")
        args.add("--input", ast_raw.path)
        args.add("--output", ast_out.path)
        args.add_all("--allowed", allowed_doc_names)

        ctx.actions.run(
            executable = worker,
            arguments = [args],
            inputs = [ast_raw],
            outputs = [ast_out],
            mnemonic = "RustySphinxValidate",
            progress_message = "Validating toctree in %s" % src.short_path,
        )
        ast_files.append(ast_out)

    # Collect .ast files from deps (other rusty_sphinx_library targets).
    transitive_asts = [dep[RustySphinxInfo].ast_files for dep in ctx.attr.deps]

    return [
        DefaultInfo(files = depset(ast_files)),
        RustySphinxInfo(
            ast_files = depset(ast_files, transitive = transitive_asts),
            direct_doc_names = local_doc_names,
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
            doc = "Other rusty_sphinx_library targets that are structurally included via `.. toctree::`. Not required for standard cross-references.",
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

Each .rst file becomes an independently cacheable .ast file. Declare
strict structural dependencies (i.e. targets included in a `.. toctree::`) in `deps`.
Standard cross-references/hyperlinks do not need to be declared in `deps` as they are
resolved late during the site rendering phase.
""",
)
