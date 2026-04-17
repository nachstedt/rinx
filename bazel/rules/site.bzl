"""
rusty_sphinx_site rule.

Collects all .ast files from its deps (Phase 2: index) then renders each
one to HTML (Phase 3: render) using the rusty-sphinx worker binary.
"""

load("//:providers.bzl", "RustySphinxInfo")

def _rusty_sphinx_site_impl(ctx):
    worker = ctx.executable._worker

    # ── Phase 2: index ────────────────────────────────────────────────────────
    # Collect every .ast file transitively from all deps.
    all_ast_files = depset(
        transitive = [dep[RustySphinxInfo].ast_files for dep in ctx.attr.deps],
    )
    ast_list = all_ast_files.to_list()

    index_out = ctx.actions.declare_file(ctx.label.name + ".project.index")
    index_args = ctx.actions.args()
    index_args.add("index")
    index_args.add("--output", index_out.path)
    index_args.add("--inputs")
    index_args.add_all(ast_list)

    ctx.actions.run(
        executable = worker,
        arguments = [index_args],
        inputs = ast_list,
        outputs = [index_out],
        mnemonic = "RustySphinxIndex",
        progress_message = "Indexing %s docs" % len(ast_list),
    )

    # ── Phase 3: render ───────────────────────────────────────────────────────
    html_files = []
    for ast_file in ast_list:
        # Namespace HTML outputs under the site's target name to prevent action conflicts
        html_out = ctx.actions.declare_file(
            ctx.label.name + "_site_out/" + ast_file.short_path.removesuffix(".ast") + ".html",
        )
        ctx.actions.run(
            executable = worker,
            arguments = [
                "render",
                "--input", ast_file.path,
                "--index", index_out.path,
                "--output", html_out.path,
            ],
            inputs = [ast_file, index_out],
            outputs = [html_out],
            mnemonic = "RustySphinxRender",
            progress_message = "Rendering %s" % ast_file.short_path,
        )
        html_files.append(html_out)

    return [DefaultInfo(files = depset(html_files))]

rusty_sphinx_site = rule(
    implementation = _rusty_sphinx_site_impl,
    attrs = {
        "deps": attr.label_list(
            providers = [RustySphinxInfo],
            doc = "rusty_sphinx_library targets to include in this site.",
        ),
        "_worker": attr.label(
            default = Label("@@//:rusty_sphinx_worker"),
            executable = True,
            cfg = "exec",
            doc = "The rusty-sphinx binary. Defaults to //:rusty_sphinx_worker in the consuming workspace.",
        ),
    },
    doc = """
Assembles a complete documentation site from rusty_sphinx_library deps.

Performs Phase 2 (global index, one action) and Phase 3 (per-file HTML
render, one action per .ast file).  All actions are individually cacheable
by Bazel.
""",
)
