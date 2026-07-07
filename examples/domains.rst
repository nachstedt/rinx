Domains
=======

Rusty-Sphinx supports Sphinx-style domains for namespacing directives and
cross-reference roles by language, mirroring real Sphinx. This page uses
``py`` and ``c`` as the first two domains, each with a ``function`` object
type.

Python Domain
-------------

.. py:function:: greet(name)

   Greets the given name.

.. function:: farewell(name)

   Says goodbye to the given name. Written without an explicit domain
   prefix — this library's default domain is ``py`` (the default), so this
   resolves to ``py:function``.

.. py:module:: greetings
   :platform: Unix, Windows
   :synopsis: Greeting utilities.
   :deprecated:

   A module holding the ``greet`` and ``farewell`` functions.

.. py:function:: greetings.shout(name)

   Greets the given name loudly.

The :py:mod:`greetings` Module
""""""""""""""""""""""""""""""

This heading's title itself contains a cross-reference role, proving that
inline markup and domain-object roles resolve inside headings, not just in
paragraph text — the heading text above renders as a working link to the
module definition, not literal ``:py:mod:`greetings``` text.

C Domain
--------

.. c:function:: int add(int a, int b)

   Adds two numbers and returns the sum.

Cross-References
-----------------

Call :py:func:`greet` to greet someone, or :c:func:`add` to add two numbers.
The bare role :func:`farewell` also resolves to the ``py`` domain, since
that is this library's default. The :py:mod:`greetings` module holds both
of them.

See also Team A's :c:func:`subtract`, defined in a library whose
``default_domain`` is set to ``c``.

A domain-object role's target may be prefixed with ``!`` to suppress the
link entirely — e.g. :py:func:`!not_a_real_function` renders as plain text
with no warning, even though no such function is defined. A ``~`` prefix
instead keeps the link but shortens the displayed text to the last dotted
component — e.g. :py:func:`~greetings.shout` links to ``greetings.shout``
but displays only ``shout``.
