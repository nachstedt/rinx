# 32. Module synopsis and the Python Module Index

## Status

Accepted.

## Context

`.. py:module::` takes three options describing the module: `:synopsis:`
(one sentence saying what it is for), `:platform:` and `:deprecated:`. rinx
parsed all three and printed them as paragraphs under the module's
definition. Issue #209 found the consequence: CPython's `Doc/library/abc.rst`
writes

```rst
.. module:: abc
   :synopsis: Abstract base classes according to :pep:`3119`.
```

and the page showed `` :pep:`3119` `` as source text.

Sphinx 9.1 does something else entirely. `PyModule.run` records the three
options in the Python domain and prints none of them — its own comment says
"the platform and synopsis aren't printed; in fact, they are only used in the
modindex currently". The option is read as `lambda x: x`, so it is a raw
string and never inline-parsed. It surfaces in two places, both as escaped
plain text:

- **The Python Module Index**, `py-modindex.html`: every module, grouped by
  first letter with submodules under their package, and a column showing the
  synopsis, platform and a "Deprecated" qualifier
  (`PythonModuleIndex.generate`, `themes/basic/domainindex.html`).
- **The tooltip of a link to the module**: a resolved `:mod:` (or `:any:`/
  `:obj:` landing on a module) carries `title="name: synopsis (deprecated)
  (platform)"` (`PythonDomain._make_module_refnode`).

Sphinx writes the index page only when the project documents at least one
module, and a project can switch it off with `html_domain_indices`.

## Decision

1. **The options are no longer printed on the module's page**, which closes
   #209 the way Sphinx does. They stay plain strings: inline-parsing the
   synopsis would render markup Sphinx shows as source, and would put a
   `:pep:` in it into the general index, which Sphinx does not.
2. **The index records each module** as a `ModuleEntry` (document, synopsis,
   platform, deprecated) in `ProjectIndex::modules`, keyed like
   `domain_objects` so a resolved `:mod:` finds its entry, and merged per
   document like every other index field, so the live preview sees it too.
   The display spelling comes from `domain_object_spelling` rather than a
   second copy.

   On the AST, `PyModule` carries a `ModuleOptions` — the two values plus a
   `ModuleFlag` set, as `ToctreeOptions` does — and now honours
   `:no-index:`/`:noindex:` (current module set, but no target and no index
   entry of either kind) and `:no-index-entry:`, both of which Sphinx's
   `PyModule` reads. Without `:no-index:` the index would be wrong for
   CPython: `email.compat32-message.rst` redescribes `email.message` with it,
   and the last description would have won the entry.
   `:no-contents-entry:` and `:no-typesetting:` are accepted and dropped, as
   `PyModule.run` never reads them. The options are read through the shared
   `scan_option_lines`, so an option outside `PyModule.option_spec` is now
   reported as `directive.unknown-option` instead of leaking into the body,
   and CPython's 16 synopses wrapped over two lines are one value — joined
   with a space, as every option read there is, where docutils keeps the
   `\n` (only a tooltip can show the difference).
3. **A resolved `:mod:` link carries Sphinx's tooltip**, built by one function
   from that entry. It needs no page and is therefore unconditional.
4. **The module index page is opt-in, per site**, through a `rinx_site`
   attribute named after Sphinx's setting:

   ```python
   rinx_site(
       name = "site",
       domain_indices = ["py-modindex"],  # default: []
   )
   ```

   Sphinx decides whether to write the page after reading every document.
   Bazel cannot: an action's outputs are declared at analysis time, before
   any document is read, so the choice is between declaring
   `py-modindex.html` for every site or for the sites that ask for it. A
   documentation site with no Python in it has no use for a Python module
   index, so writing an empty one everywhere is the wrong default. Two
   alternatives were rejected:

   - **Always write it, and link it only when modules exist.** Every
     non-Python site would ship a pointless file.
   - **Let the action decide, through a directory output.** Bazel lets an
     action fill a tree artifact with whatever it chooses, but the page would
     then live in a subdirectory rather than at the site root where Sphinx
     puts it, breaking links copied from a Sphinx site — too high a price for
     one page.

   The attribute takes a list, as Sphinx's setting does, so a later index of
   another domain is a new value rather than a new attribute.

   Enabling it on a site documenting no module is reported, since the page
   would be empty. Leaving it off on a site that documents modules is
   silent: unlike `diagrams = True`, nothing *breaks* without it — no
   construct fails to render, and the tooltips still work — so there is no
   line to point a diagnostic at, and a Sphinx project switching the index
   off is an ordinary choice.

   The rule passes `--domain-index <name>` to the index action alone, which
   stores it as `ProjectIndex::domain_indices` — a build input like
   `external_inventories`, never merged — and runs the `modindex` action only
   when enabled; a name outside the known set fails while analyzing, and the
   worker refuses it too. The setting is a rule attribute and not a
   `rinx.toml` field because the rule must know it to declare the output,
   and cannot read the config file; one source cannot drift. It lives in the
   index rather than on every action's command line because `:ref:` needs
   it inside the body renderer, which sees only the index and the config —
   and the live preview, which merges into a stale index, then knows it too.

   With the index enabled, every page's sidebar links it ("Module Index",
   beside "Index"), and `objects.inv` lists both `modindex` and
   `py-modindex` as `std:label`s, exactly as Sphinx 9.1's own inventory
   does.

5. **The predefined labels resolve.** Sphinx predefines `genindex` ("Index")
   and `modindex` ("Module Index"), and each domain index adds its own
   (`py-modindex`, "Python Module Index"). `ProjectIndex::special_page` is
   the one lookup `:ref:` and `:any:` both consult, after the labels
   documents define and before any inventory. It answers `genindex` always,
   and the module index labels only where the site writes the page: Sphinx
   predefines them unconditionally and so links a page it may never have
   written, where this build reports the link broken.

6. **Submodules fold without a script.** Sphinx's toggler is JavaScript
   swapping `minus.png`/`plus.png` and hiding `tr.cg-N` rows. Here a package
   and its submodules form one `<tbody class="modindex-group">`, whose head
   row carries a checkbox; the stylesheet hides the group's submodule rows
   while it is unchecked (`:has()`). The sidebar folds with `<details>`, but
   table rows cannot sit inside one. The page starts folded by Sphinx's own
   rule — only when top-level modules outnumber submodules, a documented
   package counting as top-level.

7. **`modindex_common_prefix`** is a `rinx.toml` list — module names, not
   paths, so the config rule allows it. As in Sphinx, the longest matching
   prefix is set aside only while choosing a module's letter and package;
   the row still shows the full name, and a prefix equal to the whole name
   strips nothing.

## Consequences

- #209 is fixed without touching `for_each_inline_list`: there is no inline
  content in a synopsis to walk.
- Reading the options through `scan_option_lines` also fixed a latent
  position bug: the module's body was parsed without rebasing `ParseCtx`
  past the option lines, so every diagnostic inside it pointed that many
  lines too high.
- The module index is a port of `PythonModuleIndex.generate`, so its grouping
  — package heads, a placeholder row for a submodule whose package is not
  documented, `modindex_common_prefix` — and its initial folding match
  Sphinx's.
- A project migrating from Sphinx gets no module index until it sets
  `domain_indices`; Sphinx's default is on. That is the cost of not writing
  the page for every other project, and it is paid once, in the build file.
