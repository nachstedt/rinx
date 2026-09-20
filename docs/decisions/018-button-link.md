# 18. `.. button-link::`: one of a pair, shaped for the other

## Status

Accepted.

## Context

`.. button-link::` is sphinx-design's button-shaped external link: a link drawn
as a button, whose argument is the URL and whose content is the label.

```rst
.. button-link:: https://gitpod.io/#https://github.com/useblocks/sphinx-needs-demo
   :color: primary
   :shadow:

   Open this playground in Gitpod Online Editor!
```

After ADR-017 it was the largest remaining unsupported construct in the entity
benchmark corpus — `button-link: 3`, tied with `needextend`, which ADR-016
argues has no obvious phase. All three uses have exactly the shape above, and
all three sit inside `.. grid-item::`s, so they reach the parser at all only
because ADR-011's grid parses its body.

The reference implementation is `sphinx_design/badges_buttons.py`. Two classes
matter: `_ButtonDirective`, which holds the whole option spec and builds the
element, and `ButtonLinkDirective`, which contributes only `get_target`
(docutils' `directives.uri`) and `create_ref_node` (a `nodes.reference` with a
`refuri`). Its sibling `ButtonRefDirective` contributes the other two:
`ws_re`-normalized whitespace and a `pending_xref` resolved against the
project.

Three details of that implementation are worth stating, because none of them
appears in sphinx-design's documentation:

- **The content is inline markup, and optional.** `run_with_defaults` calls
  `self.state.inline_text` on the joined content lines, and falls back to
  `nodes.inline(target, target)` — the URL as its own label — when there is
  none.
- **`:outline:` produces nothing without `:color:`.** The colour classes are
  appended only inside `if "color" in self.options`, so an `:outline:` written
  alone adds no class at all.
- **The inner `<span>` is always present**, label or no label, and
  sphinx-design's stylesheet is written against that shape.

## Decision

Support `.. button-link::` in full, and **only** it — but store its target as
an enum from the first commit, so `.. button-ref::` can be added later without
touching a variant, a renderer arm or an already-written `.ast` file.

`ButtonTarget` has one variant today, `Url(String)`. `renderer`'s
`blocks/button_link.rs` reads it through a single `href` function, which is the
only thing the two directives render differently: everything else — the nine
options, the class order, the `<span>`, the `<p>`, the escaping — is already
shared.

The parser is `directives/button_link.rs`, following `dropdown.rs`'s spine
(`unindent_body_lines` → `scan_option_lines` → `read_options` →
`report_unknown_options`) and differing in exactly the two places the construct
does: the label is inline-parsed through an accumulated `SourceMap`, so a role
on its third line reports at that line, and a missing argument degrades to
`Directive::Malformed` rather than to a button pointing nowhere.

The four value-less options are one `BTreeSet<ButtonFlag>` rather than four
`bool` fields. `clippy::struct_excessive_bools` is what prompted it and the
model is better for it: `directives.flag` accepts no value at all, so each of
them carries only the fact that it was written, and one vocabulary now serves
the parser reading names, the node answering `has()`, and the renderer drawing.

## Consequences

- The three corpus uses parse and render with **no new `button-link.*`
  warnings**: they write only `:color: primary` and `:shadow:`.
- `button-link` joins `BUILTIN_DIRECTIVE_NAMES`, so an entity schema can no
  longer declare a section by that name.
- `button-ref` does **not** join it. Until the directive exists, a project may
  declare a `button-ref` section and would break when it arrives. Accepted:
  reserving names for unbuilt directives inverts the rule `dispatch.rs`'s own
  doc comment states, and one schema name is a smaller cost than a list of
  names nothing implements.
- It is the one sphinx-design directive here with no block body, so unlike
  `dropdown`, `grid` and `grid-item` it needs no arm in `index_nodes`,
  `assign_index_ids`, `collect_anonymous_targets` or `walk_nodes`. Only
  `substitutions.rs` gained one, for the label's inline content.

## Deviations from sphinx-design

1. **docutils' `reference external` classes are not emitted.** A docutils
   `visit_reference` appends them to every `<a>` it writes; `inline/hyperlink.rs`
   writes a bare `<a href>` for every other link in this build, and two link
   renderings that disagree would be the bug. This is the opposite call from the
   trailing `docutils` class a `.. grid::` keeps (ADR-011), and for a stated
   reason: sphinx-design's own components carry that class deliberately and a
   page's stylesheet may be written against it, while `reference external` is
   docutils' generic link marking, which nothing in this build emits anywhere.

2. **`:ref-type:` is refused by name** as `button-link.unsupported-option`,
   naming `.. button-ref::` as where it belongs. sphinx-design accepts it here
   only because both directives share one option spec, and silently does nothing
   with it — the `entity-flow.unsupported-option` precedent applies: an option
   that cannot work should say so.

3. **`:outline:` with no `:color:` is reported** as
   `button-link.unusable-outline`, where sphinx-design is silent. The
   `uml.unusable-scale` precedent: an option that provably draws nothing gets a
   name, since the author cannot otherwise tell it apart from a styling problem.

4. **A reference role in the label is reported** as
   `button-link.nested-reference` and rendered as its text alone. The label is
   already inside the button's `<a>`, and nesting a second one is invalid HTML.
   sphinx-design hits the same wall from the other side — `ButtonRefContentStash`
   declines to stash content holding an xref, with the comment that "a link
   nested inside a button link would be invalid HTML anyway" — and flattens
   silently. The reference is **kept** in the node rather than dropped, the rule
   ADR-011 set for a misplaced grid child: content that was written is never lost
   quietly.

5. **The argument does not continue onto the next line.** docutils would fold a
   following line into the argument, since the directive declares
   `final_argument_whitespace`. No directive in this build continues an argument
   across lines except a domain object's signatures, so the line is read as the
   label instead — wrong, but visible to the author, where a dropped line would
   not be. Whitespace *within* one argument line is still removed, as
   `directives.uri` removes it.

## Not done

- **`.. button-ref::`**, the sibling. Its href is cheap — the target lookup in
  `render_inline_reference` is reusable as it stands — but a *contentless*
  button-ref labels itself with its target's title, and `ProjectIndex.targets`
  maps a name to a document path with no title beside it. A titles-by-target
  lookup over `document_outlines` is the real work in that directive, and this
  one needs none of it. `:ref-type:`'s four values would also have to be
  answered: `ref` and `any` map onto machinery this build has, `doc` means the
  argument is a docname, and `myst` is meaningless here.
- **The `bdg-*` badge roles** from the same upstream module. Roles, not this
  directive.
- **`sd_custom_directives` and `SdDirective`'s per-directive option defaults**,
  both `conf.py` features — unsupported for `.. dropdown::` and `.. grid::` too.
- **`.. grid-item-card::`, `.. card::`, `.. tab-set::`**, the rest of
  sphinx-design.
