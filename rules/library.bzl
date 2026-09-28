"""
rinx_library rule.

Parses every .rst source file (Phase 1) into a .ast JSON file using the
rinx worker binary and propagates them via RinxInfo.
"""

load("//:providers.bzl", "RinxInfo")

def _rinx_library_impl(ctx):
    worker = ctx.executable._worker

    # The entity schema is the vocabulary the *parser* works from: without it,
    # a `.. req::` is an unknown directive rather than an entity. It therefore
    # joins the parse action's inputs, and editing it re-parses every document
    # in this library. That cost is the price of parse-time diagnostics, and is
    # why the schema is its own attribute rather than part of `parse_data` —
    # it is configuration, not content a single directive reads.
    entity_schema = ctx.file.entity_schema
    schema_args = ["--entity-schema", entity_schema.path] if entity_schema else []
    schema_inputs = [entity_schema] if entity_schema else []

    # The Jinja pass is opt-in per library for the same reason diagrams are:
    # `{{` and `{%` are ordinary text in most projects, and a build that
    # rendered every document would turn a page *about* templating into a
    # syntax error. The templates themselves are ordinary `parse_data`.
    jinja_args = ["--jinja"] if ctx.attr.jinja else []
    if ctx.attr.jinja and ctx.attr.jinja_context:
        jinja_args.append("--jinja-context")
        jinja_args.extend(["%s=%s" % (name, value) for name, value in sorted(ctx.attr.jinja_context.items())])

    # Diagrams are opt-in, because Bazel cannot know before reading a document
    # whether it holds one — without the opt-in, every document of every
    # project would pay for a diagram pipeline most of them never use. A
    # library that leaves this off gets no diagram actions at all, and a
    # diagram written in it fails its parse on the directive's own line rather
    # than shipping a page with a picture nothing compiled.
    diagram_args = ["--diagrams"] if ctx.attr.diagrams else []
    ast_files = []
    doctest_plans = []
    embed_sidecars = []

    local_doc_names = [src.short_path.removesuffix(".rst") for src in ctx.files.srcs]
    
    allowed_doc_names_list = list(local_doc_names)
    for dep in ctx.attr.deps:
        allowed_doc_names_list.extend(dep[RinxInfo].direct_doc_names)
    
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
                "--default-domain", ctx.attr.default_domain,
            ] + schema_args + diagram_args + jinja_args,
            # `parse_data` files join the parse action's inputs because the
            # parser genuinely reads them: `.. csv-table::`'s `:file:`, and the
            # sources `.. include::`/`.. literalinclude::` splice into the
            # document. This is a separate mechanism from `deps` (toctree
            # structure) and from late-resolved cross-references: it declares
            # *bytes the parser reads*, not another library. A file left out is
            # simply absent from the sandbox, so the parse fails loudly instead
            # of silently reading the host filesystem.
            #
            # Note there is no cache firewall here, unlike `images`: an
            # included file's text becomes part of the .ast, so editing it
            # re-parses and re-renders every document that includes it. That is
            # correct — the page really did change.
            inputs = [src] + ctx.files.parse_data + schema_inputs,
            outputs = [ast_raw],
            mnemonic = "RinxParse",
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
            mnemonic = "RinxValidate",
            progress_message = "Validating toctree in %s" % src.short_path,
        )
        ast_files.append(ast_out)

        # Phase 1.7: Extract the doctest plan.
        #
        # Declared for every document but kept out of DefaultInfo, so it is
        # only ever built when a doctest test target (or an explicit
        # --output_groups request) asks for it. Bazel is demand-driven at
        # execution time, so a site that never runs doctests pays only the
        # analysis-phase cost of the action object, not the CPU.
        #
        # This action is the cache firewall: its output depends only on the
        # test code, so a prose edit re-runs this cheap AST walk but leaves the
        # bytes identical, and the Python test does not re-run.
        doctest_plan = ctx.actions.declare_file(
            src.basename.removesuffix(".rst") + ".doctests.json",
            sibling = src,
        )
        args_doctests = ctx.actions.args()
        args_doctests.add("extract_doctests")
        args_doctests.add("--input", ast_out.path)
        args_doctests.add("--output", doctest_plan.path)

        ctx.actions.run(
            executable = worker,
            arguments = [args_doctests],
            inputs = [ast_out],
            outputs = [doctest_plan],
            mnemonic = "RinxExtractDoctests",
            progress_message = "Extracting doctests from %s" % src.short_path,
        )
        doctest_plans.append(doctest_plan)

        # Phase 1.75: Embed the images this document asked to inline.
        #
        # Every declared image joins this action's inputs, because Bazel cannot
        # know at analysis time which of them a document actually embeds — that
        # is written inside the .rst. Choosing the coarser granularity here is
        # what keeps the *render* action fine-grained: it takes only this
        # sidecar, so an image edit re-runs this cheap AST walk for every
        # document but leaves the bytes identical for every document that does
        # not embed the changed file, and those pages do not re-render.
        #
        # That is the same cache firewall the doctest plans have, and the
        # reason this is a separate action rather than work done during the
        # render. See docs/decisions/007-image-assets.md.
        embeds_out = ctx.actions.declare_file(
            src.basename.removesuffix(".rst") + ".embeds.json",
            sibling = src,
        )
        args_embeds = ctx.actions.args()
        args_embeds.add("embed_assets")
        args_embeds.add("--input", ast_out.path)
        args_embeds.add("--output", embeds_out.path)

        ctx.actions.run(
            executable = worker,
            arguments = [args_embeds],
            inputs = [ast_out] + ctx.files.images,
            outputs = [embeds_out],
            mnemonic = "RinxEmbedAssets",
            progress_message = "Embedding assets for %s" % src.short_path,
        )
        embed_sidecars.append(embeds_out)

    # Collect .ast files from deps (other rinx_library targets).
    transitive_asts = [dep[RinxInfo].ast_files for dep in ctx.attr.deps]
    transitive_diagram_asts = [dep[RinxInfo].diagram_ast_files for dep in ctx.attr.deps]
    transitive_images = [dep[RinxInfo].image_files for dep in ctx.attr.deps]
    transitive_downloads = [dep[RinxInfo].download_files for dep in ctx.attr.deps]
    transitive_embeds = [dep[RinxInfo].embed_sidecars for dep in ctx.attr.deps]

    # `doctest_plans` carries this library's own plans, and is what
    # `rinx_doctest_tests` consumes. It is deliberately not in
    # DefaultInfo, so `bazel build` of a site never produces them; ask for them
    # by hand with `--output_groups=doctest_plans`.
    return [
        DefaultInfo(files = depset(ast_files)),
        OutputGroupInfo(doctest_plans = depset(doctest_plans)),
        RinxInfo(
            ast_files = depset(ast_files, transitive = transitive_asts),
            diagram_ast_files = depset(
                ast_files if ctx.attr.diagrams else [],
                transitive = transitive_diagram_asts,
            ),
            direct_doc_names = local_doc_names,
            image_files = depset(ctx.files.images, transitive = transitive_images),
            # No action of this library reads them: a download is served, not
            # parsed, rendered or embedded, so it only rides to the site.
            download_files = depset(ctx.files.downloads, transitive = transitive_downloads),
            embed_sidecars = depset(embed_sidecars, transitive = transitive_embeds),
        ),
    ]

rinx_library = rule(
    implementation = _rinx_library_impl,
    attrs = {
        "srcs": attr.label_list(
            allow_files = [".rst"],
            doc = "reStructuredText source files owned by this library.",
        ),
        "parse_data": attr.label_list(
            allow_files = True,
            doc = "Files this library's documents read *at parse time*: the data behind `.. csv-table::`'s `:file:` option, and the sources spliced in by `.. include::` and `.. literalinclude::`. Paths in the document resolve relative to the file the directive was written in (the document, or an including fragment), or to the source root with a leading `/`; declaring the file here is what puts it in the parse action's sandbox. An `.rst` included this way must NOT also appear in `srcs` — that would additionally publish it as a page of its own.",
        ),
        "images": attr.label_list(
            allow_files = True,
            doc = "Image files this library's documents show via `.. image::`/`.. figure::`. Paths in the document resolve relative to the document itself, or to the source root with a leading `/`. Declaring the file here is what gets it bundled into the site's `_images/` directory and, for a `:loading: embed` image, into the build action that inlines it; an undeclared image fails the site's image validation.",
        ),
        "downloads": attr.label_list(
            allow_files = True,
            doc = "Files this library's documents link with the `:download:` role. Paths in the document resolve relative to the document itself, or to the source root with a leading `/`. Declaring the file here is what copies it into the site's `_downloads/` directory, at its source-root-relative path; an undeclared one fails the site's asset validation. No parse or render action takes these as inputs, so editing a downloadable file re-renders no page.",
        ),
        "deps": attr.label_list(
            providers = [RinxInfo],
            doc = "Other rinx_library targets that are structurally included via `.. toctree::`. Not required for standard cross-references.",
        ),
        "diagrams": attr.bool(
            default = False,
            doc = "Whether this library's documents may hold PlantUML diagrams — `.. plantuml::`/`.. uml::`, `.. entity-diagram::`/`.. needuml::`, `.. entity-arch::`/`.. needarch::` and the generated `.. entity-flow::`/`.. needflow::` and `.. entity-sequence::`/`.. needsequence::`. Off by default so a library that draws nothing pays nothing: the site creates diagram actions only for documents of libraries that set this. A diagram written in a library without it fails the parse, naming this attribute. To keep the cost narrow, put diagram documents in a library of their own.",
        ),
        "entity_schema": attr.label(
            allow_single_file = [".toml"],
            doc = "The project's entity meta-model: the types, attributes, sections, relations and roles this library's documents may use. Read at *parse* time, since it is what makes `.. req::` a directive rather than an unknown name, so editing it re-parses every document here. Every library in a site and the site itself must name the same file — a mismatch is reported as `entity.schema-mismatch` when the index is built. Omit it for a project that declares no entities, which then behaves exactly as before this feature existed.",
        ),
        "jinja": attr.bool(
            default = False,
            doc = "Whether this library's `.rst` sources are rendered as Jinja templates before they are parsed — the transform a Sphinx project gets by connecting the `source-read` event in its `conf.py`. Off by default, because a document is free to contain `{{` and `{%` as text and most projects mean nothing by them. The templates an `{% include %}` reads are declared in `parse_data`, like every other file the parser reads, and must not also appear in `srcs`.",
        ),
        "jinja_context": attr.string_dict(
            doc = "Names a Jinja-rendered source may read, the way a Sphinx project's `html_context` binds them. Values only; a name bound by neither this nor a `{% set %}` is reported rather than silently rendered as the empty string. Meaningless without `jinja = True`.",
        ),
        "default_domain": attr.string(
            default = "py",
            values = ["py", "c"],
            doc = "The Sphinx domain (e.g. \"py\", \"c\") that bare, unprefixed directives and roles in this library's docs resolve to. This is a property of the library's content, not of any site that assembles it.",
        ),
        "_worker": attr.label(
            default = Label("//:rinx"),
            executable = True,
            cfg = "exec",
            doc = "The rinx binary.",
        ),
    },
    doc = """
Parses a set of reStructuredText files into AST files (Phase 1).

Diagrams are opt-in with `diagrams = True`; see that attribute. Note this rule
does *not* compile them itself. A `.. needuml::` is expanded
against the project's entity graph, which only exists once every document has
been indexed, so diagram compilation belongs to `rinx_site` — and a
plain `.. plantuml::` goes the same way, because two compile paths would mean
two hash populations and the compiled set could drift from the validated one.

Each .rst file becomes an independently cacheable .ast file. Declare
strict structural dependencies (i.e. targets included in a `.. toctree::`) in `deps`.
Standard cross-references/hyperlinks do not need to be declared in `deps` as they are
resolved late during the site rendering phase.

Sources can be rendered as Jinja templates before parsing with `jinja = True`
— what a Sphinx project gets from a `source-read` hook in its `conf.py`. The
templates an `{% include %}` names are declared in `parse_data` and must not
also appear in `srcs`.

Files read while parsing go in `parse_data` — the data behind
`.. csv-table:: :file:`, the templates a Jinja `{% include %}` reads, and the
sources `.. include::`/`.. literalinclude::` splice in. That is unrelated to `deps`: it declares bytes the parser reads, not
another library. A `.. toctree::` written *inside* an included fragment still
needs its documents in `deps`, because toctree validation runs on the merged
AST. Pictures
shown by `.. image::`/`.. figure::` go in `images`, which is unrelated to both:
those bytes are read by neither the parser nor another library — they are
copied into the site and, when a document asks to embed one, inlined into it.
Files linked with `:download:` go in `downloads`, which is like `images` minus
the embedding: they are copied into the site's `_downloads/` and nothing else.
""",
)
