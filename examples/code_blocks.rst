.. _code-blocks:

###########
Code Blocks
###########

Code blocks are highlighted **during the build**, into HTML carrying one CSS
class per token. Nothing is loaded at view time: no script, no network, no
webfont. A language the highlighter does not know becomes a build warning
rather than a silently unstyled block in a reader's browser.

The Language Argument
---------------------

The directive's argument names the language:

.. code-block:: python

   def greet(name: str) -> str:
       return f"Hello, {name}!"

   print(greet("World"))

Any of the bundled languages works, and a language's file extension is
accepted as an alias for its name — ``py`` for ``python``, ``rs`` for ``rust``:

.. code-block:: rust

   fn main() {
       let greeting = String::from("Hello, World!");
       println!("{greeting}");
   }

Two argument values are reserved rather than being languages. ``none`` turns
highlighting off for one block:

.. code-block:: none

   This text is shown exactly as written,
   with no highlighting applied to it at all.

``default`` means "Python, but do not complain if it does not fit" — the value
a block falls back to when nothing else has set a language:

.. code-block:: default

   x = [1, 2, 3]

``text`` and ``plain`` are accepted as spellings of ``none``, since Pygments
uses them for its do-nothing lexer and Sphinx documents are full of both.

Omitting the argument entirely does *not* mean "no language": the block
inherits whatever ``.. highlight::`` last set, described below.

``:caption:`` and ``:name:``
----------------------------

A caption is rendered above the block. A ``:name:`` makes the block a
cross-reference target, so :ref:`greet-example` links to it:

.. code-block:: python
   :caption: A greeting, with a caption and a name
   :name: greet-example

   def greet(name):
       return f"Hello, {name}!"

``:linenos:`` and ``:lineno-start:``
------------------------------------

``:linenos:`` numbers the lines from one:

.. code-block:: python
   :linenos:

   import sys

   def main():
       sys.exit(0)

``:lineno-start:`` numbers them from somewhere else, which is what an excerpt
lifted out of a longer file wants. It implies ``:linenos:``, so writing both is
unnecessary:

.. code-block:: python
   :lineno-start: 42

   def main():
       sys.exit(0)

``:emphasize-lines:``
---------------------

Draws a band across the lines named, given as a comma-separated list in which
each entry is either a single line or an inclusive ``first-last`` range. The
numbers are relative to the block, not to the file it came from, and are
validated against the block's own length while parsing:

.. code-block:: python
   :linenos:
   :emphasize-lines: 1,4-5

   import sys

   def main():
       print("this line and the next are emphasized")
       sys.exit(0)

``:dedent:``
------------

Removes leading columns from every line, for source pasted in at an
indentation that made sense where it came from but not here. With a value it
removes exactly that many columns:

.. code-block:: python
   :dedent: 4

       # four columns were removed from each of these lines
       if True:
           pass

Without a value it removes whatever indentation the lines share, which is the
common case:

.. code-block:: python
   :dedent:

       def indented():
           return "the shared indent is gone"

``:class:`` and ``:force:``
---------------------------

``:class:`` adds CSS classes to the block's wrapper, after the language class
the renderer always emits. ``:force:`` says a highlighting failure is
acceptable, which suppresses the warning an unknown language would otherwise
produce:

.. code-block:: not-a-real-language
   :class: custom-block
   :force:

   Whatever this is, the build does not complain about it.

``.. highlight::``
------------------

Sets the language every following block inherits, so a document written mostly
in one language need not repeat it:

.. highlight:: rust

With that in force, a block with no argument is Rust:

.. code-block::

   let inherited = "this block never named a language";

So is a plain ``::`` literal block, exactly as in Sphinx::

    let also_inherited = 1;

A block that names its own language still wins:

.. code-block:: python

   inherited = False

``:linenothreshold:`` numbers any following block at least that many lines
long, without each one having to ask:

.. highlight:: python
   :linenothreshold: 3

This block is three lines long, so it is numbered:

.. code-block::

   first = 1
   second = 2
   third = 3

This one is shorter than the threshold, so it is not:

.. code-block::

   only = 1

Restoring the default for the rest of the page:

.. highlight:: default

``.. code::``
-------------

docutils spells the same directive ``.. code::``. It takes the language as its
argument in the same way, but writes line numbering as a single
``:number-lines:`` option, whose optional value is the starting number:

.. code:: python
   :number-lines:

   def counted():
       return "numbered from one"

.. code:: python
   :number-lines: 10

   def counted_from_ten():
       return "numbered from ten"

Because the two directives are genuinely different spellings rather than
aliases, each accepts only its own options: ``:linenos:`` on a ``.. code::``
is reported as an unknown option, and so is ``:number-lines:`` on a
``.. code-block::``.
