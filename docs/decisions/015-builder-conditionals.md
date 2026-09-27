# 15. `.. if-builder::`: conditional content, spliced rather than wrapped

## Status

Accepted.

## Context

`.. if-builder::` is sphinx-simplepdf's directive for content that belongs to
one builder. Its body is contributed only when the argument names the builder
that is running:

```rst
.. if-builder:: simplepdf

   .. toctree::

      my_files
      specific_pdf_file

.. if-builder:: html

   .. toctree::

      my_files

   Other HTML specific content, which will not be part of the PDF.
```

ADR-013 left it open. Once the Jinja pass began splicing useblocks'
`demo_page_header.rst` into 24 of the benchmark corpus's 31 documents, the
header's own body turned out to sit behind `.. if-builder:: html`, and
`if-builder: 24` joined the unsupported-directive tally. An unknown directive
never parses its body, so on every one of those pages the header rendered as an
error block and everything inside it — prose and entities alike — was invisible
to the index.

The reference implementation is small enough to quote in full
(`sphinx_simplepdf/directives/ifbuilder.py`):

```python
class IfBuilderDirective(Directive):
    has_content = True
    required_arguments = 1
    optional_arguments = 0
    final_argument_whitespace = True
    option_spec: ClassVar[dict] = {}

    def run(self):
        builder = self.arguments[0]
        content_node = nodes.container()
        if self.env.app.builder.name.upper() == builder.upper():
            rst = ViewList()
            for line in self.content:
                rst.append(line, self.docname, self.lineno)
            node_collection_content = nodes.Element()
            node_collection_content.document = self.state.document
            nested_parse_with_titles(self.state, rst, node_collection_content)
            content_node += node_collection_content.children
        return [content_node]
```

One argument, no options, a case-insensitive comparison against the builder's
*name*, and — the detail that matters most — `nested_parse_with_titles` rather
than `nested_parse`, called only inside the `if`.

## Decision

`.. if-builder::` is a **transclusion, not a container**: it joins
`.. include::` as one of the two directives that contribute any number of nodes
to the enclosing block instead of exactly one. The pair is dispatched together
by `try_parse_splicing_directive`, ahead of the chain that wraps a single node.

The builder is `html`, named once as `BUILDER_NAME` in
`crates/parser/src/directives/if_builder.rs`. A matching body is parsed with the
document's own `adornment_order` threaded through, which is this codebase's
spelling of `nested_parse_with_titles`; a non-matching body is not parsed at
all, as upstream also does not parse it.

Three diagnostics, `if-builder.missing-builder`,
`if-builder.unknown-builder` and `if-builder.empty-body`.

## Consequences

- **The ast, analyzer and renderer crates are untouched.** With no node of its
  own there is nothing to add to `walk_directive`, `assign_index_ids`,
  `resolve_directive`, `index_nodes` or `collect_anonymous_targets`, and
  nothing to render.
- **An excluded block is free rather than merely hidden.** Its directives are
  never looked up, so a PDF-only block full of constructs this build does not
  implement reports nothing; its targets and toctree entries never exist, so a
  `.. toctree::` inside one needs no entry in the library's `deps`.
  `examples/if_builder.rst` keeps a standing proof of that: an excluded block
  there names a document that exists nowhere, and the site builds.
- **The name is reserved.** Like every supported extension directive it joins
  `BUILTIN_DIRECTIVE_NAMES`, so an entity schema can no longer declare a
  section called `if-builder`.
- The staleness upstream warns about — "if-builder may not be taken into
  account, if a Sphinx incremental build is performed. Be sure to always use a
  clean first build, after a builder switch" — cannot arise here. There is one
  builder, and the decision is taken inside the parse action, whose cache key
  already covers the document.

## Deviations

- **No `<div class="docutils container">`.** Upstream returns a
  `nodes.container()` whether or not the builder matched. Reproducing it would
  need an AST node, and a node would put the body *inside a directive* — where
  `build_document_outline` deliberately stops, since reStructuredText has no
  section inside a directive body. The headings the directive exists to admit
  would quietly stop being sections, which is precisely what upstream reached
  for `nested_parse_with_titles` to avoid, and what its documentation names as
  the reason to prefer `if-builder` over `.. only::`. The wrapper carries
  nothing the source expressed; the structure does. This is the one place the
  emitted HTML differs, and it is deliberate.
- **An unrecognised builder name is reported.** Upstream compares the argument
  against the running builder and excludes on any mismatch, so
  `.. if-builder:: htlm` is indistinguishable from a deliberate exclusion and
  deletes its content with nothing to grep for. This build keeps a table of the
  names that mean something — sphinx-simplepdf's own builder and Sphinx's
  built-ins — and reports anything outside it as
  `if-builder.unknown-builder`. Naming a real builder that is not this one stays
  silent, because that is the ordinary use of the directive.
- **A selected block with no content is reported** as
  `if-builder.empty-body`, where upstream contributes an empty container. Only
  the branch that was *taken* is diagnosed: emptiness on a branch nobody
  selected says nothing about whether the document is right.
- **A missing argument is a `Directive::Malformed`**, drawn as an error block
  quoting the source, where docutils reports its generic "1 argument(s)
  required". The build cannot decide the block either way, so hiding it would
  lose content on a guess.

## Not done

sphinx-simplepdf's two other directives, `.. ifinclude::` and
`.. pdfinclude::`, are still unknown names. Both are PDF-oriented and would be
close to no-ops on an HTML-only build, so they would mostly be reserving names.

Sphinx's own `.. only::` remains unimplemented (`docs/compatibility.rst`). It solves the
same problem with a boolean expression over build *tags*, which needs a small
expression language and a project-wide way to declare tags; sphinx-simplepdf's
documentation recommends `if-builder` over it on the structural grounds above.
