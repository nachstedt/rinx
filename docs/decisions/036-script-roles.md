# 36. The `:sub:` and `:sup:` roles

## Status

Accepted.

## Context

docutils registers `:subscript:` and `:superscript:` as *generic* roles, with
`:sub:` and `:sup:` as their short names. Until now rinx parsed none of the
four. The catch-all role pattern matched them, and since no schema or
`.. role::` knew the name, the markup was left as text: ``:sub:`2` ``.

What docutils does with them:

- **The content is plain text.** A generic role builds a `subscript` or
  `superscript` node from the text after backslash-escape processing. Nothing
  inside it is parsed as markup, and there is no explicit-title split.
- **The node is an ordinary text element.** So smartquotes still applies to
  its text, as it does to emphasis. Literals, code and math are the
  exceptions.
- **The HTML is `<sub>` and `<sup>`.** A role derived with
  `.. role:: chem(sub)` adds its classes: `<sub class="chem">`. Without
  `:class:`, the role's own name is the class, as for every derived role.
- **Markup inside a word needs escaped spaces**, as in ``H\ :sub:`2`\ O``.
  The escaped spaces disappear, giving `H<sub>2</sub>O`.

## Decision

### One node for both positions

`InlineNode::Script` carries a `ScriptPosition` (`Subscript` or
`Superscript`), the text and the classes. The two roles differ in the element
they are drawn as and in nothing else. That is the reason ADR-034 gave for one
`RegistryReference` over four registries.

`ScriptPosition::from_role_name` is the one list of the four spellings. The
inline scan uses it to read a role, and `.. role::` uses it to accept a base.
So the two cannot disagree about which names exist.

### Plain text, with smart typography

The handler takes every character between the backticks, as `:code:`'s does.
Unlike `:code:`, it applies smart typography, since docutils treats the text
as prose. The escape markers are dropped by `unescape_node`'s display-form
arm. That arm is also what makes the escaped-space idiom work: the escaped
spaces on either side of the role vanish from the neighbouring text nodes.

### Derived roles

`.. role:: name(sub)`, and the same for `subscript`, `sup` and
`superscript`, is now accepted alongside `code`. The base is matched ignoring
case, as for `code`. The table `CustomRoles` now holds a `CustomRole` enum
with one variant per base. A script role takes only `:class:`. A `:language:`
on one is reported as an unknown option, and the role is still defined.

The derived role is built by `inline/roles/custom.rs`. It builds the node its
base builds, so a derived script gets the same typography and unescaping as
`:sub:` itself.

### Deliberate deviations

- **Role names are case-sensitive**, as for every built-in role here (see
  ADR-034): `:Sub:` is not recognized. A `.. role::`'s base is the exception,
  since that is matched ignoring case already.
- **A section title is flattened to plain text in the navigation**, so
  ``CO\ :sub:`2` `` shows as `CO2` in the sidebar and the page `<title>`, as
  emphasis already does. Sphinx keeps the markup in its toctree. The heading
  itself keeps the `<sub>`.
- **The four names are reserved.** A `.. role:: sub(code)` is refused as
  `role.builtin-name`, as for every role rinx implements; docutils lets a
  document replace it.
