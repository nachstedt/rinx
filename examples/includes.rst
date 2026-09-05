Includes
========

Two directives splice another file into this one: ``.. include::`` brings in
reStructuredText, which is parsed as if it had been typed here, and
``.. literalinclude::`` brings in a source file, which is shown verbatim.

Both read their file **while parsing**, so under Bazel the file has to be
declared in the library's ``parse_data`` attribute. A file that is included
must *not* also appear in ``srcs``, or it would be published as a page of its
own as well as being spliced in here.

Including reStructuredText
--------------------------

The whole of a fragment, parsed in place:

.. include:: shared/parameters.rst

Selecting part of a fragment
----------------------------

The same fragment carries ``BEGIN``/``END`` markers, so a document can take
just the part it wants. Note that the markers themselves stay out:

.. include:: shared/parameters.rst
   :start-after: BEGIN PARAMETERS
   :end-before: END PARAMETERS

docutils' ``:start-line:``/``:end-line:`` select by index instead. They are
0-based and the end is exclusive — the one place in reStructuredText that
counts from zero:

.. include:: shared/parameters.rst
   :start-line: 5
   :end-line: 7

Including a file verbatim
-------------------------

``:literal:`` shows the fragment's own markup rather than interpreting it,
and ``:code:`` does the same with a language for highlighting:

.. include:: shared/parameters.rst
   :code: rst
   :start-after: BEGIN PARAMETERS
   :end-before: END PARAMETERS

Including source files
----------------------

``.. literalinclude::`` takes a path as its argument and the language as an
*option*, unlike ``.. code-block::``:

.. literalinclude:: shared/greeter.py
   :language: python
   :caption:

With no ``:language:`` it inherits from the enclosing ``.. highlight::``,
exactly as a ``.. code-block::`` with no argument does.

Selecting lines
~~~~~~~~~~~~~~~

``:lines:`` takes numbers and ranges, and ``:lineno-match:`` numbers the
result from the line it really starts at in the file:

.. literalinclude:: shared/greeter.py
   :language: python
   :lines: 7-11
   :lineno-match:
   :emphasize-lines: 3

The marker comments in the file are the more maintainable way to say the same
thing, since they survive an edit above them:

.. literalinclude:: shared/greeter.py
   :language: python
   :start-after: # [greet]
   :end-before: # [/greet]
   :dedent: 0

Framing the excerpt
~~~~~~~~~~~~~~~~~~~

``:prepend:`` and ``:append:`` add lines around the selection, which is how an
excerpt from the middle of a file is made to stand on its own:

.. literalinclude:: shared/greeter.py
   :language: python
   :start-after: # [greet]
   :end-before: # [/greet]
   :prepend: import sys
   :append: # ... and so on

Showing a change
~~~~~~~~~~~~~~~~

``:diff:`` shows the patch between another file and this one, rather than
either file's contents:

.. literalinclude:: shared/greeter.py
   :diff: shared/greeter_before.py

Line numbers and captions
~~~~~~~~~~~~~~~~~~~~~~~~~

``:linenos:``, ``:lineno-start:``, ``:name:`` and ``:class:`` mean exactly
what they do on a ``.. code-block::``, and a ``:caption:`` with no value takes
the filename:

.. literalinclude:: shared/greeter.py
   :language: python
   :lines: 14-16
   :linenos:
   :lineno-start: 100
   :name: greeter-main
   :caption: The entry point

A reference to :ref:`greeter-main` resolves to the block above.
