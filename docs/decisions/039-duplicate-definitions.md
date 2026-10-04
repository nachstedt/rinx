# 39. A definition claimed twice defines nothing

## Status

Accepted.

## Context

Several documents can define the same name: a label, a glossary term, an
equation label, a domain object, an entity id. Sphinx warns ("duplicate
label %s, other instance in %s", "duplicate object description of %s, other
instance in %s, use :no-index: for one of them") and keeps whichever
definition it read **last**. Until now rinx did the same without the
warning for most of these: `ProjectIndex::merge` extended its maps, so the
last document merged won, and only entity ids and glossary terms were
reported — as "the latter definition wins".

"Last" is the order of the index action's `--inputs`, or of the documents an
editor happens to have analysed. The language server (ADR-038, roadmap #4)
folds a per-document map in whatever order edits arrive, so the same project
would resolve a reference to one definition in the build and to the other in
the editor. A result must not depend on the order its inputs arrive in.

Measured before the change, CPython's docs held 33 domain objects described in
two documents and one external hyperlink name with different URLs per page.
All of them were artefacts of other bugs — `:noindex:` ignored (#259), wrapped
signatures split into junk names, external targets indexed project-wide
(#261) — and are gone; neither benchmark corpus defines anything twice now.

## Decision

1. **A name claimed by two or more documents defines nothing.** The merge
   removes it from its family's map and records every claimant in
   `ProjectIndex::ambiguous_definitions` (`rinx_index::AmbiguousDefinitions`),
   one map per family, since each family is its own namespace. The merge rule
   is a union of claimants per key — commutative and associative — so any
   order of documents gives a byte-identical index. A definition from the
   document already holding the key *replaces* it: that is a fresh analysis
   of the same document (the live preview), not a second claimant. A domain
   object is keyed by name and object type, so a function and a class of one
   name do not contest each other.
2. **Every claimant is warned, each naming the others**, under a code naming
   the construct: `target.duplicate-name`, `glossary.duplicate-term`,
   `object.duplicate-description` (advising `:no-index:`, as Sphinx does),
   `math.duplicate-label` and `entity.duplicate-id`. A contested entity id
   also contests its `:ref:` target and is reported once, as the entity.
   The warnings have no position: what is wrong is the existence of another
   definition, in another file.
3. **A reference to a contested name is `link.ambiguous-target`**, naming the
   claimants, from every role that resolves through these families (`:ref:`,
   `:term:`, `:eq:`, `:numref:`, domain roles, `:option:`, entity roles,
   `:any:`). It is never looked up in another site's inventory instead: this
   site does define the name, twice, and linking elsewhere would hide that.
4. **A named reference (`name_`) still reaches its own document's label**
   when another document defines the label too. It resolves within its
   document only, as in docutils (#261), so the clash is not its
   concern.
5. **The accumulated lists are put in document order** after the fold:
   general-index entries by document (each document's own in written order),
   redirects sorted. They were the remaining input-order dependence in the
   index.

## Consequences

- The build and the language server agree on every resolution, whatever
  order documents arrive in; the server's per-document fold can be checked
  against `build_project_index` byte for byte.
- A project that relied on "last wins" now gets warnings and broken links
  where Sphinx linked silently to one definition. That is deliberate: the
  author is told which documents to settle between.
- The serialized index gains a field only when something is contested.
- Within one document, a name defined twice is still last-wins in the
  analyzer; reporting it is #260, which also needs targets to carry spans.

## Alternatives considered

- **Keep Sphinx's last-wins.** Rejected: "last" is an accident of input
  order, and the editor and the build would disagree.
- **First-wins in path order.** Deterministic, but arbitrary: a rename of an
  unrelated file could move a link, and the author is not told.
- **Resolve to the definition nearest the referencing document.** No rule
  for "nearest" survives a large project, and it still hides the clash.
