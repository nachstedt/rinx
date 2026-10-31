# 47. The parser records the syntax an editor folds and colours

## Status

Accepted.

## Context

Roadmap #33 lets an editor fold sections, directives and the other blocks
an author collapses, and colour roles, references, directives, options and
substitutions by kind. To answer both, the server has to know where each
block ends and exactly which characters each name takes. The parsed document
held little of that:

- A block recorded at most its first line: a heading its title line, a
  directive its marker line (ADR-045). An admonition, an unknown directive, a
  comment, a literal block and a table recorded nothing, and no block
  recorded where it ends. The outline (#28) worked out a directive's extent
  again from the text, by docutils' indentation rule.
- An inline node's span covers the whole construct, `` :ref:`Title <x>` ``.
  Nothing records a role's name, a directive's name or an option's name.
- A `|substitution|` is replaced by its definition before `parse()` returns,
  so no position of the reference survives at all.

The step's wording proposed "spans plus light lexing": the server would lex
what the spans do not cover. That is a second reader of the grammar, one that
can disagree with the parser about what is a role (a `:name:` inside a
literal), where a directive's block ends (a blank line, a dedent, an
`.. include::` splice) and where a line really is once a Jinja pass has moved
it. It would also have to be written again for every syntax rinx reads: a
MyST front end is planned, and its directives and roles are spelled
differently.

## Decision

### The parser records a syntax table beside the nodes

`rinx_ast::SyntaxTable` holds two lists, filled while the document is parsed:

- **Blocks** (`SyntaxBlock`): a heading with its adornment lines and level,
  a directive's marker and its whole block, a comment, a literal or doctest
  block from the line introducing it, and a table. Each runs from its first
  written line to its last non-blank one.
- **Tokens** (`SyntaxToken`): a directive's name with its `::`, an option's
  `:name:`, a role's `:name:` before or after its text, what a reference leads
  to (a role's target, a hyperlink's name or address, a `.. _name:` label),
  and a `|substitution|`. A label and a substitution's definition carry the
  `Declaration` modifier.

Each entry is recorded where the parser already reads the construct, by a
`SyntaxRecorder` the parse context borrows, as it borrows the document's
roles. A directive is recorded where its marker is recognized, before it is
routed, so an unknown or malformed one folds like any other. An option is
recorded by the reader every directive is split by. A role is recorded by the
inline scan, which knows which node its match made, so only a match that
leads somewhere gets a reference token, and an inline literal none. A
substitution reference is recorded when it is scanned, before it is resolved
away. Every position goes through `ParseCtx::position`, so a nested block, an
included fragment (its `FileId`) and a Jinja-rendered line are placed exactly
as a diagnostic is.

### It is never serialized

`Document::syntax` is `#[serde(skip)]`, as `include_sites` is: only the
process that parsed the document reads it, and a `.ast` must not change when a
line moves (ADR-045). A test checks that a serialized document holds none of
it.

### The server only reads it

`textDocument/foldingRange` folds each section from its title line to the
last written line before the next heading at its level or above, and each
directive, comment, literal block and table over its own lines. Comments
fold with the `comment` kind. Lists and paragraphs do not fold, since folding
them is noise. A client's `rangeLimit` keeps the longest ranges.

`textDocument/semanticTokens/full` and `…/range` colour the tokens. A token
running over several lines is sent once per line, since not every client
takes `multilineTokenSupport`. Where two tokens claim the same characters,
the first is kept: a Jinja pass can map two rendered lines onto one written
one (issue #321). Neither request answers for a block or token in another
file: a fragment is coloured when it is opened itself.

### The legend names what the text is

The token types are rinx's own: `directive`, `option`, `role`, `reference`
and `substitution`, plus the standard `declaration` modifier. Standard names
(`macro`, `function`, `property`, …) would colour in every theme with no
setup, but they would misdescribe the text, and a theme could not tell a role
from a Python function. So the names stay honest, and each editor gets a
fallback:

- The VS Code extension contributes each type with a standard `superType`
  (`macro`, `property`, `function`, `string`, `variable`) and a TextMate scope.
  It also turns semantic highlighting on for reStructuredText, which most
  themes leave off.
- The Neovim snippet links each `@lsp.type.<kind>` group to a standard one,
  as a default a colour scheme overrides.

### No delta

Only `full` and `range` are announced. Measured on CPython's 556 documents
with a release build, colouring the largest documents from their latest parse
takes under a millisecond: `builtins/stdtypes.rst`, 2595 tokens, 0.4 ms;
`library/os.rst`, 3730 tokens, 0.8 ms. The time after an edit is the parse's,
which a delta would not save. `rinx lsp --check <folder> --semantic-tokens
<document>` times both.

### What recording costs

Recording costs nothing measurable. On the same corpus, `rinx lsp --check`
over every document takes 11.8 to 12.0 s with or without it. With all 556
documents open, the server holds 7 MB more (403 MB against 396 MB, 1.7%).

## Consequences

- A future MyST reader colours and folds by filling the same table from its
  own syntax, with no lexer in the server.
- Any parser change to where a construct is read moves its colouring with it,
  for free. A construct the parser reads but records nothing for, such as
  an unknown directive's options, is not coloured.
- The outline reads a directive's extent from the table too, rather than
  working it out again from the text by indentation.
