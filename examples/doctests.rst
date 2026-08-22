.. _doctests:

Doctest Directives
==================

This page demonstrates the ``sphinx.ext.doctest`` directive family. Every block
here is *rendered* by the normal site build; *executing* them is the job of a
separate, opt-in test target, so building this page never needs a Python
interpreter.

Interactive examples
--------------------

A ``.. doctest::`` block holds an interactive session — ``>>>`` prompts with
their expected output inline:

.. doctest::

   >>> 1 + 1
   2
   >>> "rusty".upper()
   'RUSTY'

Separate code and output
------------------------

``.. testcode::`` holds plain Python, and the ``.. testoutput::`` that follows
it in the same group states what it should print:

.. testcode::

   def double(value):
       return value * 2

   print(double(21))

.. testoutput::

   42

Named groups
------------

A group is an independent namespace. Blocks join one by naming it as the
directive argument; the argument is a comma-separated list, so a block can join
several at once.

.. testcode:: greetings

   greeting = "hello"
   print(greeting)

.. testoutput:: greetings

   hello

Setup and cleanup
-----------------

``.. testsetup::`` runs before a group's tests and ``.. testcleanup::`` after
it. Neither is ever rendered — that is a property of the directive, not an
option — so the two blocks below produce no output on this page. ``*`` selects
every group in the document.

.. testsetup:: *

   import math

.. testcleanup:: *

   del math

Hidden blocks
-------------

``:hide:`` suppresses a block that still runs. The next block is executed but
not shown:

.. testcode:: greetings
   :hide:

   assert greeting == "hello"

Comparison options
------------------

``:options:`` passes flags to the comparison, each carrying a ``+`` or ``-``
sign. ``+ELLIPSIS`` lets ``...`` stand in for any text:

.. doctest::
   :options: +ELLIPSIS

   >>> list(range(20))
   [0, 1, 2, ...]

Doctest flag comments
---------------------

Flags can also be written as a trailing ``# doctest:`` comment. These direct the
runner and are noise to a reader, so they are stripped from the rendered page by
default — the block below displays without its comment:

.. doctest::

   >>> print("some long value")  # doctest: +NORMALIZE_WHITESPACE
   some long value

``:no-trim-doctest-flags:`` keeps them visible, which is useful when the
documentation is *about* doctest itself:

.. doctest::
   :no-trim-doctest-flags:

   >>> print("some long value")  # doctest: +NORMALIZE_WHITESPACE
   some long value

Conditional execution
---------------------

``:pyversion:`` restricts a block to matching interpreters, and ``:skipif:``
skips it when a Python expression is true. Both are evaluated when the tests
run, never while parsing:

.. testcode::
   :pyversion: >= 3.5

   print("modern enough")

.. testoutput::
   :pyversion: >= 3.5

   modern enough

.. testcode::
   :skipif: True

   raise AssertionError("never reached — this block is always skipped")
