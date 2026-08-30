"""
rusty_sphinx_library rule.

Parses every .rst source file (Phase 1) into a .ast JSON file using the
rusty-sphinx worker binary and propagates them via RustySphinxInfo.
"""

load("//:providers.bzl", "RustySphinxInfo")

def _rusty_sphinx_library_impl(ctx):
    worker = ctx.executable._worker
    plantuml = ctx.executable._plantuml
    ast_files = []
    svg_dirs = []
    doctest_plans = []

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
                "--default-domain", ctx.attr.default_domain,
            ],
            # `csv_data` files join the parse action's inputs because
            # `.. csv-table::`'s `:file:` option reads them at parse time. This
            # is a third, separate mechanism from `deps` (toctree structure)
            # and from late-resolved cross-references: it declares *data* the
            # parser reads, not another library. A file left out of `csv_data`
            # is simply absent from the sandbox, so the parse fails loudly
            # instead of silently reading the host filesystem.
            inputs = [src] + ctx.files.csv_data,
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
            mnemonic = "RustySphinxExtractDoctests",
            progress_message = "Extracting doctests from %s" % src.short_path,
        )
        doctest_plans.append(doctest_plan)

        # Phase 1.8: Extract PlantUML diagrams
        puml_dir = ctx.actions.declare_directory(src.basename.removesuffix(".rst") + "_puml", sibling = src)
        args_puml = ctx.actions.args()
        args_puml.add("extract_diagrams")
        args_puml.add("--input", ast_out.path)
        args_puml.add("--outdir", puml_dir.path)
        
        ctx.actions.run(
            executable = worker,
            arguments = [args_puml],
            inputs = [ast_out],
            outputs = [puml_dir],
            mnemonic = "RustySphinxExtractPuml",
            progress_message = "Extracting diagram definitions from %s" % src.short_path,
        )

        # Phase 1.9: Compile PlantUML
        svg_dir = ctx.actions.declare_directory(src.basename.removesuffix(".rst") + "_svgs", sibling = src)
        
        # We wrap in a shell to pass an absolute output path ($PWD/...) and check for empty directories.
        # By passing plantuml in 'tools', Bazel safely aggregates the Java runfiles!
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
            progress_message = "Generating Diagrams for %s" % src.short_path,
        )
        svg_dirs.append(svg_dir)

    # Collect .ast files from deps (other rusty_sphinx_library targets).
    transitive_asts = [dep[RustySphinxInfo].ast_files for dep in ctx.attr.deps]
    transitive_svg_dirs = [dep[RustySphinxInfo].svg_dirs for dep in ctx.attr.deps]

    # `doctest_plans` carries this library's own plans, and is what
    # `rusty_sphinx_doctest_tests` consumes. It is deliberately not in
    # DefaultInfo, so `bazel build` of a site never produces them; ask for them
    # by hand with `--output_groups=doctest_plans`.
    return [
        DefaultInfo(files = depset(ast_files)),
        OutputGroupInfo(doctest_plans = depset(doctest_plans)),
        RustySphinxInfo(
            ast_files = depset(ast_files, transitive = transitive_asts),
            direct_doc_names = local_doc_names,
            svg_dirs = depset(svg_dirs, transitive = transitive_svg_dirs),
        ),
    ]

rusty_sphinx_library = rule(
    implementation = _rusty_sphinx_library_impl,
    attrs = {
        "srcs": attr.label_list(
            allow_files = [".rst"],
            doc = "reStructuredText source files owned by this library.",
        ),
        "csv_data": attr.label_list(
            allow_files = True,
            doc = "Data files this library's documents read via `.. csv-table::`'s `:file:` option. Paths in the document resolve relative to the document itself; declaring the file here is what puts it in the parse action's sandbox.",
        ),
        "deps": attr.label_list(
            providers = [RustySphinxInfo],
            doc = "Other rusty_sphinx_library targets that are structurally included via `.. toctree::`. Not required for standard cross-references.",
        ),
        "default_domain": attr.string(
            default = "py",
            values = ["py", "c"],
            doc = "The Sphinx domain (e.g. \"py\", \"c\") that bare, unprefixed directives and roles in this library's docs resolve to. This is a property of the library's content, not of any site that assembles it.",
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
            doc = "The PlantUML executable or wrapper.",
        ),
    },
    doc = """
Parses a set of reStructuredText files into AST files (Phase 1).

Each .rst file becomes an independently cacheable .ast file. Declare
strict structural dependencies (i.e. targets included in a `.. toctree::`) in `deps`.
Standard cross-references/hyperlinks do not need to be declared in `deps` as they are
resolved late during the site rendering phase.

Data files read by `.. csv-table:: :file:` go in `csv_data`, which is unrelated
to `deps`: it declares bytes the parser reads, not another library.
""",
)
