"""
rusty_sphinx_site rule.

Collects all .ast files from its deps (Phase 2: index) then renders each
one to HTML (Phase 3: render) using the rusty-sphinx worker binary.
"""

load("//:providers.bzl", "RustySphinxInfo")

def _compile_diagrams(ctx, plantuml, puml_dir, doc_path):
    """Compiles one document's diagram sources to SVG, returning the SVG dir.

    Per document, and taking only that document's sources as input, so a
    diagram that did not change leaves this action's key untouched and no JVM
    starts. Wrapped in a shell to pass an absolute output path ($PWD/...) and
    to handle an opted-in document that happens to draw nothing; passing
    plantuml in `tools` lets Bazel aggregate its Java runfiles.
    """
    svg_dir = ctx.actions.declare_directory(ctx.label.name + "_svgs/" + doc_path)
    command_script = """
set -e
shopt -s nullglob
files=("{puml_dir}/"*.puml)
mkdir -p "$PWD/{svg_dir}"
if [ ${{#files[@]}} -gt 0 ]; then
  "{plantuml}" -tsvg -nometadata -o "$PWD/{svg_dir}" "${{files[@]}}"
fi
""".format(
        plantuml = plantuml.path,
        svg_dir = svg_dir.path,
        puml_dir = puml_dir.path,
    )
    ctx.actions.run_shell(
        tools = [plantuml],
        command = command_script,
        inputs = [puml_dir],
        outputs = [svg_dir],
        mnemonic = "PlantUMLCompile",
        progress_message = "Generating diagrams for %s" % doc_path,
    )
    return svg_dir

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

    # The same schema the libraries were parsed against. The index action needs
    # the relations to derive back-links, and each render action needs the
    # labels and the relation vocabulary its presentation reads.
    entity_schema = ctx.file.entity_schema
    schema_inputs = [entity_schema] if entity_schema else []

    # Per-type presentation. The *name* a schema refers to is the file's
    # basename; the path stays on the command line, because a sandboxed build
    # relocates files and a path baked into config would break (ADR-001).
    entity_templates = ctx.files.entity_templates
    css_file = ctx.file.css

    index_out = ctx.actions.declare_file(ctx.label.name + ".project.index")
    index_args = ctx.actions.args()
    index_args.add("index")
    index_args.add("--output", index_out.path)

    # The index action reads the config for `root_doc`, which decides where
    # navigation, page order and section numbering start. `--inputs` is
    # variadic, so every other flag has to precede it.
    index_args.add("--config", config_file.path)
    if entity_schema:
        index_args.add("--entity-schema", entity_schema.path)
    index_args.add("--inputs")
    index_args.add_all(ast_list)

    ctx.actions.run(
        executable = worker,
        arguments = [index_args],
        inputs = ast_list + [config_file] + schema_inputs,
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

    # Documents whose library set `diagrams = True`. Only these get a diagram
    # output and a compile action; every other document renders exactly as it
    # would in a project that had never heard of diagrams. That is the whole of
    # "pay only for what you use" here, because Bazel cannot know before
    # reading a document whether it holds a diagram — the library has to say.
    diagram_asts = {
        f: True
        for f in depset(
            transitive = [dep[RustySphinxInfo].diagram_ast_files for dep in ctx.attr.deps],
        ).to_list()
    }
    plantuml = ctx.executable._plantuml
    svg_dirs = []
    puml_dirs = []

    # ── Phase 3: render ───────────────────────────────────────────────────────
    # Each page takes only its own document's embedded-asset sidecar, never the
    # site's images — that is what keeps an image edit from re-rendering every
    # page (see rules/library.bzl's embed action).
    embeds_by_doc = {
        sidecar.short_path.removesuffix(".embeds.json"): sidecar
        for sidecar in depset(
            transitive = [dep[RustySphinxInfo].embed_sidecars for dep in ctx.attr.deps],
        ).to_list()
    }

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
        ] + ([
            "--entity-schema", entity_schema.path,
        ] if entity_schema else []) + [
            "--template", template_file.path,
            "--warnings-output", warnings_out.path,
        ]
        if ctx.attr.strict_links:
            render_args.append("--strict-links")

        render_inputs = [ast_file, index_out, template_file, config_file] + schema_inputs + entity_templates
        render_outputs = [html_out, warnings_out]

        # The render writes each diagram's PlantUML text itself: it already
        # expands every diagram to get the hash its `<img>` names, so a
        # separate expansion action would only repeat that work. The page and
        # the file it names therefore come from one process.
        #
        # The cache firewall is this directory. Any edit anywhere re-runs every
        # render — the index is an input — but for a document whose diagrams
        # did not change the .puml bytes are identical, so the compile below
        # keeps its action key and no JVM starts.
        puml_dir = None
        if ast_file in diagram_asts:
            puml_dir = ctx.actions.declare_directory(ctx.label.name + "_puml/" + doc_path)
            render_args.extend(["--diagram-outdir", puml_dir.path])
            render_outputs.append(puml_dir)
        embeds_file = embeds_by_doc.get(doc_path)
        if embeds_file:
            render_args.extend(["--embeds", embeds_file.path])
            render_inputs.append(embeds_file)

        # Last on the command line: this flag is variadic, so anything after it
        # would be swallowed as another template path.
        if entity_templates:
            render_args.append("--entity-templates")
            render_args.extend([t.path for t in entity_templates])

        ctx.actions.run(
            executable = worker,
            arguments = render_args,
            inputs = render_inputs,
            outputs = render_outputs,
            mnemonic = "RustySphinxRender",
            progress_message = "Rendering %s" % ast_file.short_path,
        )
        html_files.append(html_out)
        warnings_files.append(warnings_out)

        if puml_dir:
            puml_dirs.append(puml_dir)
            svg_dirs.append(_compile_diagrams(ctx, plantuml, puml_dir, doc_path))

    # ── Phase 4: Bundle Images ────────────────────────────────────────────────
    # The diagrams this site just compiled, plus the pictures its authors wrote.
    all_svg_dirs = svg_dirs

    # Pictures the authors wrote, as opposed to the diagrams above that this
    # build generated. Both are served from the one `_images/` directory, and
    # an authored image keeps its source-root-relative path inside it
    # (`team_a/logo.svg`, not `logo.svg`). Sphinx instead flattens every image
    # to its basename and renames collisions with a global counter, which is
    # order-dependent across libraries and hostile to per-action caching;
    # keeping the path makes collisions impossible without any global pass.
    # See docs/decisions/007-image-assets.md.
    all_image_files = depset(
        transitive = [dep[RustySphinxInfo].image_files for dep in ctx.attr.deps],
    ).to_list()

    final_outputs = html_files + [genindex_out]

    if all_svg_dirs or all_image_files:
        images_out = ctx.actions.declare_directory(ctx.label.name + "_site_out/_images")

        args = ctx.actions.args()
        args.add(images_out.path)
        args.add(str(len(all_svg_dirs)))
        for svg_dir in all_svg_dirs:
            args.add(svg_dir.path)
        for image in all_image_files:
            # Both halves: where to read it from now, and where it must land
            # inside `_images/`.
            args.add(image.path)
            args.add(image.short_path)

        # $1 is images_out and $2 the number of diagram directories that
        # follow; the rest are (source, site-relative destination) pairs.
        ctx.actions.run_shell(
            command = """
set -euo pipefail
OUT="$1"; shift
DIR_COUNT="$1"; shift
mkdir -p "$OUT"
while [ "$DIR_COUNT" -gt 0 ]; do
  cp -R "$1"/* "$OUT"/ 2>/dev/null || true
  shift
  DIR_COUNT=$((DIR_COUNT - 1))
done
while [ "$#" -gt 0 ]; do
  DEST="$OUT/$2"
  mkdir -p "$(dirname "$DEST")"
  cp "$1" "$DEST"
  shift 2
done
""",
            arguments = [args],
            inputs = all_svg_dirs + all_image_files,
            outputs = [images_out],
            mnemonic = "RustySphinxBundleImages",
            progress_message = "Bundling Site Images",
        )
        final_outputs.append(images_out)

        # ── Phase 4.5: Validate Images ────────────────────────────────────────
        # Checks that every picture the documents refer to actually reached the
        # bundle: a PlantUML diagram that failed to compile, and an authored
        # image left out of its library's `images` attribute alike. Both would
        # otherwise ship as a page that looks finished and renders a broken
        # picture.
        validation_sentinel = ctx.actions.declare_file(ctx.label.name + ".images.validated")
        val_args = ctx.actions.args()
        val_args.add("validate_images")
        val_args.add("--image-dir", images_out.path)
        val_args.add("--output", validation_sentinel.path)

        # Each diagram source a render wrote must have its compiled SVG. The
        # render already expanded every template, so nothing is re-derived
        # here: the files are the record of what the pages name.
        if puml_dirs:
            val_args.add("--diagram-dirs")
            val_args.add_all(puml_dirs, expand_directories = False)

        # Last: this flag is variadic, so anything after it would be swallowed
        # as another .ast path.
        val_args.add("--inputs")
        val_args.add_all(ast_list)

        ctx.actions.run(
            executable = worker,
            arguments = [val_args],
            inputs = ast_list + [images_out] + puml_dirs,
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
        OutputGroupInfo(
            domain_warnings = depset(warnings_files),
            # The expanded PlantUML source of every diagram, one directory per
            # document — sphinx-needs' `needs_build_needumls`, as an output
            # group rather than a config flag.
            #
            # A non-default group, so building a site never materializes them;
            # ask with `--output_groups=diagram_sources`. This is also the
            # answer to `:save:`, which cannot work as sphinx-needs spells it:
            # a sandboxed action may only write files declared at analysis
            # time, and the path in a `:save:` is written inside the document.
            diagram_sources = depset(puml_dirs),
        ),
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
        "entity_templates": attr.label_list(
            allow_files = [".html"],
            doc = "Per-type entity presentation templates, rendered with MiniJinja. A schema's `template = \"name.html\"` refers to a file by its basename; listing it here is what puts it in the render action's sandbox. Omit for the built-in rendering, which every type gets without configuring anything. A template a type names but that is absent here fails the build rather than silently falling back.",
        ),
        "entity_schema": attr.label(
            allow_single_file = [".toml"],
            doc = "The project's entity meta-model. Must be the same file every `rusty_sphinx_library` in `deps` names, since the documents were parsed against it; a mismatch is reported as `entity.schema-mismatch` while the index is built. Read here by the index action, which derives back-links from the declared relations, and by each render action, which reads the labels and presentation.",
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
        "_plantuml": attr.label(
            default = Label("//:plantuml_tool"),
            executable = True,
            cfg = "exec",
            doc = "The PlantUML executable or wrapper. On the site rather than on rusty_sphinx_library, because a diagram's text is not known until the project index exists.",
        ),
    },
    doc = """
Assembles a complete documentation site from rusty_sphinx_library deps.

Performs Phase 2 (global index, one action) and Phase 3 (per-file HTML
render, one action per .ast file).  All actions are individually cacheable
by Bazel.
""",
)
