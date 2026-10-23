# 44. The roles are a table, and each pattern is built from its spec

## Status

Accepted.

## Context

Roadmap #26 is role and domain-object completion in the editor. Like #25's
directives (ADR-043), it needs a list the editor can offer, and there was
none: a role's names lived only inside its regular expression, one hand-written
`LazyLock<Regex>` per role in `inline/regexes.rs`, paired with a `kind` tag
in `SIMPLE_ROLE_REGEXES`. `find_role_kind` could *check* a name by spelling
it as ``:name:`x` `` and matching, but nothing could list one, and which
domains a role takes, or whether it reads an `:external:` prefix, was
spelled into each pattern by hand.

## Decision

### The spec is data, beside its handler

Every role with a fixed name has a `RoleSpec` constant beside its handler
(`py/func.rs`'s `SPEC`, `math.rs`'s `MATH_SPEC` and `EQ_SPEC`), built from
the leaf `inline/roles/spec.rs`: its `names`, the `domains` it may be
qualified with, what an `:external:` prefix does to it (`ExternalPrefix`:
accepted, read only to be refused — `:numref:`'s — or not read), and a
sentence of `doc`. Like a `DirectiveSpec` it says nothing about the
``:name:`text` `` syntax, so another front end could name the same roles.
`:ref:`, `:term:` and `:program:`, whose handlers lived in the dispatcher,
moved into `std_/` beside `:option:` to get a home.

### The pattern is built from the spec

`regexes.rs`'s `role_regex` builds every role's pattern from its spec — the
`external` capture, the `domain` capture over the spec's domains, the `role`
capture over its names — so a role is matched by exactly the names it is
listed under. `ROLES` is the one list pairing spec, pattern and `kind`;
`scan_patterns` chains it with the links and the catch-all `named_role`, in
the order the scan always used. `role_specs()` is public, for completion.

### A bare name depends on the default domain

A pattern matches a role's bare name whatever the default domain, because the
handler decides: a bare `:macro:` in a `py` document is text.
`RoleSpec::accepts_bare_name` states that rule — always for a role of no
domain or of `std`, otherwise only in a domain the role takes — so a list of
spellings for a document holds only names that are roles there, and a test
checks both directions against the handlers.

## Consequences

- `:data:`'s one pattern accepted `py:var`, `py:member` and `c:const`, which
  no domain defines and the handler turned into text. It is now two specs
  sharing a handler — `py`'s `data`/`const` and `c`'s `member`/`data`/`var`,
  kinds `data` and `member` — so those spellings are no roles: a default role
  of `py:var` is refused as unknown instead of rendering as text.
- The registry roles' name capture is `role` like every other's.

## Alternatives considered

- **A list of names beside the patterns, checked by a test.** A test can
  check that each listed name matches, but not that every matched name is
  listed: a regular expression's language cannot be enumerated.
- **One spec for both `:data:` families.** It would list `py:var` as a role.

## Not done

- Roles a schema declares or a document defines with `.. role::` have no
  spec: a schema or a document says what they are, and completion reads them
  from there (roadmap #26).
- `:std:ref:`, `:std:term:` and `:std:option:`, which Sphinx accepts, are not
  spellings here; adding `Domain::Std` to their specs would make them so.
