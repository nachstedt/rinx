.. _code-role-example:

The ``:code:`` Role
===================

``:code:`` marks inline text as code. On its own it has no language, so it is
shown like an inline literal: :code:`x = compute(1, 2)`. Unlike an inline
literal its content is interpreted text, so a backslash escapes the next
character: :code:`a\*b` shows ``a*b``.

Highlighted inline code
-----------------------

A language comes from a role derived from ``code`` with ``.. role::``. It
applies from its definition to the end of the page.

.. role:: python(code)
   :language: python

.. role:: rust(code)
   :language: rust
   :class: signature

Python: :python:`print(f"{name!r} has {len(items)} items")`.

Rust: :rust:`fn area(width: u32, height: u32) -> u32`.

The ``:class:`` option adds classes to the rendered ``<code>``; without it,
the role's own name is its class.

A role without a language
-------------------------

.. role:: shell(code)

A derived role that names no language stays unhighlighted but still carries
its class: :shell:`bazel build //examples:site`.

.. role:: plain(code)
   :language: none

``:language: none`` says so explicitly: :plain:`not highlighted`.
