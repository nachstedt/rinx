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

    template_file = ctx.file.template
    config_file = ctx.file.config
    css_file = ctx.file.css

    index_out = ctx.actions.declare_file(ctx.label.name + ".project.index")
    index_args = ctx.actions.args()
    index_args.add("index")
    index_args.add("--output", index_out.path)

    # The index action reads the config for `root_doc`, which decides where
    # navigation, page order and section numbering start. `--inputs` is
    # variadic, so every other flag has to precede it.
    index_args.add("--config", config_file.path)
    index_args.add("--inputs")
    index_args.add_all(ast_list)

    ctx.actions.run(
        executable = worker,
        arguments = [index_args],
        inputs = ast_list + [config_file],
        outputs = [index_out],
        mnemonic = "RustySphinxIndex",
        progress_message = "Indexing %s docs" % len(ast_list),
    )

    # ── Phase 2.5: genindex ──────────────────────────────────────────────────
    # Always generated (matches Sphinx's on-by-default genindex.html), so the
    # sidebar's "Index" link is never dangling. Only needs the project-wide
    # index, not any per-doc .ast file.
    genindex_out = ctx.actions.declare_file(ctx.label.name + "_site_out/genindex.html")
    genindex_args = ctx.actions.args()
    genindex_args.add("genindex")
    genindex_args.add("--index", index_out.path)
    genindex_args.add("--output", genindex_out.path)
    genindex_args.add("--config", config_file.path)
    genindex_args.add("--template", template_file.path)

    ctx.actions.run(
        executable = worker,
        arguments = [genindex_args],
        inputs = [index_out, template_file, config_file],
        outputs = [genindex_out],
        mnemonic = "RustySphinxGenIndex",
        progress_message = "Generating genindex.html for %s" % ctx.label.name,
    )

    # ── Phase 3: render ───────────────────────────────────────────────────────
    html_files = []
    warnings_files = []
    for ast_file in ast_list:
        doc_path = ast_file.short_path.removesuffix(".ast")
        html_out = ctx.actions.declare_file(
            ctx.label.name + "_site_out/" + doc_path + ".html",
        )

        # Structured domain-object warnings sidecar. Declared outside
        # _site_out/ so it never lands in the published site bundle, and kept
        # out of the default outputs (see OutputGroupInfo below) — it exists
        # purely for the developer-only whitelist tooling in
        # scripts/benchmark.py.
        warnings_out = ctx.actions.declare_file(
            ctx.label.name + "_warnings/" + doc_path + ".warnings.json",
        )

        render_args = [
            "render",
            "--input", ast_file.path,
            "--index", index_out.path,
            "--doc-path", doc_path,
            "--output", html_out.path,
            "--config", config_file.path,
            "--template", template_file.path,
            "--warnings-output", warnings_out.path,
        ]
        if ctx.attr.strict_links:
            render_args.append("--strict-links")

        ctx.actions.run(
            executable = worker,
            arguments = render_args,
            inputs = [ast_file, index_out, template_file, config_file],
            outputs = [html_out, warnings_out],
            mnemonic = "RustySphinxRender",
            progress_message = "Rendering %s" % ast_file.short_path,
        )
        html_files.append(html_out)
        warnings_files.append(warnings_out)

    # ── Phase 4: Bundle Images ────────────────────────────────────────────────
    all_svg_dirs = depset(
        transitive = [dep[RustySphinxInfo].svg_dirs for dep in ctx.attr.deps],
    ).to_list()
    
    final_outputs = html_files + [genindex_out]

    if all_svg_dirs:
        images_out = ctx.actions.declare_directory(ctx.label.name + "_site_out/_images")
        
        args = ctx.actions.args()
        args.add(images_out.path)
        for svg_dir in all_svg_dirs:
            args.add(svg_dir.path)
            
        # Bazel passes $1 as the first arg. We don't use $0.
        # So $1 is images_out, and $2, $3... are svg_dirs.
        ctx.actions.run_shell(
            command = "OUT=\"$1\"; shift; mkdir -p \"$OUT\"; for dir in \"$@\"; do cp -R \"$dir\"/* \"$OUT\"/ 2>/dev/null || true; done",
            arguments = [args],
            inputs = all_svg_dirs,
            outputs = [images_out],
            mnemonic = "RustySphinxBundleImages",
            progress_message = "Bundling Site Images",
        )
        final_outputs.append(images_out)

        # ── Phase 4.5: Validate Images ────────────────────────────────────────
        # This action ensures that all PlantUML diagrams referenced in ASTs
        # have corresponding SVG files in the bundle.
        validation_sentinel = ctx.actions.declare_file(ctx.label.name + ".images.validated")
        val_args = ctx.actions.args()
        val_args.add("validate_images")
        val_args.add("--image-dir", images_out.path)
        val_args.add("--output", validation_sentinel.path)
        val_args.add("--inputs")
        val_args.add_all(ast_list)

        ctx.actions.run(
            executable = worker,
            arguments = [val_args],
            inputs = ast_list + [images_out],
            outputs = [validation_sentinel],
            mnemonic = "RustySphinxValidateImages",
            progress_message = "Validating diagram images for %s" % ctx.label.name,
        )
        final_outputs.append(validation_sentinel)

    # ── Phase 5: Copy CSS ─────────────────────────────────────────────────────
    css_out = ctx.actions.declare_file(ctx.label.name + "_site_out/default.css")
    ctx.actions.run_shell(
        command = "cp \"$1\" \"$2\"",
        arguments = [css_file.path, css_out.path],
        inputs = [css_file],
        outputs = [css_out],
        mnemonic = "RustySphinxCopyCSS",
        progress_message = "Copying default.css",
    )
    final_outputs.append(css_out)

    return [
        DefaultInfo(files = depset(final_outputs)),
        OutputGroupInfo(domain_warnings = depset(warnings_files)),
    ]

rusty_sphinx_site = rule(
    implementation = _rusty_sphinx_site_impl,
    attrs = {
        "deps": attr.label_list(
            providers = [RustySphinxInfo],
            doc = "rusty_sphinx_library targets to include in this site.",
        ),
        "template": attr.label(
            allow_single_file = [".html"],
            default = Label("//:templates/default.html"),
            doc = "The HTML template file used for page rendering. Passed as --template to the render action.",
        ),
        "config": attr.label(
            allow_single_file = [".toml"],
            default = Label("//:templates/default_config.toml"),
            doc = "The rusty_sphinx.toml configuration file for the site.",
        ),
        "css": attr.label(
            allow_single_file = [".css"],
            default = Label("//:assets/default.css"),
            doc = "The CSS stylesheet to include in the site output.",
        ),
        "strict_links": attr.bool(
            default = False,
            doc = "If True, a broken cross-reference (:ref:, hyperlink, :term:, or domain-object " +
                  "role) fails the render action for that page instead of only printing a " +
                  "warning. Off by default so existing sites are unaffected.",
        ),
        "_worker": attr.label(
            default = Label("//:rusty_sphinx_worker"),
            executable = True,
            cfg = "exec",
            doc = "The rusty-sphinx binary.",
        ),
    },
    doc = """
Assembles a complete documentation site from rusty_sphinx_library deps.

Performs Phase 2 (global index, one action) and Phase 3 (per-file HTML
render, one action per .ast file).  All actions are individually cacheable
by Bazel.
""",
)
