.. _any-role-example:

The ``:any:`` Role
==================

``:any:`` names a target without saying what kind of thing it is. Every kind
a dedicated role could reach is searched — labels, documents, glossary terms,
command-line options, equations and ``py``/``c`` objects — and the one hit is
drawn exactly as its own role would draw it.

One role for every kind of target
---------------------------------

* A label, showing the title of the section it sits above:
  :any:`code-blocks`.
* A document, showing its title: :any:`glossary`.
* A glossary term, found case-insensitively as ``:term:`` finds it:
  :any:`Environment`.
* A command-line option, with its program written in the target as
  ``:option:`` accepts it: :any:`greet --config`.
* A Python function, by its qualified name — :any:`greetings.shout` — or by
  a dotted suffix, with the call parentheses an author may add:
  :any:`shout()`.
* A C macro: :any:`PY_SSIZE_T_MAX`.
* A labeled equation, showing its number: :any:`euler-identity`.
* An entity, which is a label like any other: :any:`REQ_001`.

The forms every cross-reference role has work here too: an explicit title,
:any:`the code-block page <code-blocks>`, and a leading ``!`` that shows the
target without looking it up, :any:`!greetings.shout`.

.. _python:

A name that means two things
----------------------------

This section is labeled ``python``, and the glossary defines a term *Python*,
so ``:any:`python``` names two targets at once. Rather than link whichever it
finds first — as Sphinx does — rinx links neither and reports
``link.ambiguous-any``, listing the role that would name each one alone. It
is suppressed here, since this site is built with ``strict_links = True``:

.. noqa: link.ambiguous-any

:any:`python` is ambiguous; :term:`Python` is not.
