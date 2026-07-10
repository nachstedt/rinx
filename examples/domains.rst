Domains
=======

Rusty-Sphinx supports Sphinx-style domains for namespacing directives and
cross-reference roles by language, mirroring real Sphinx. This page uses
``py`` and ``c`` as the first two domains: ``c`` has a ``function`` object
type, and ``py`` additionally has ``module``, ``data``, ``method``,
``class``, and ``attribute`` object types.

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

.. py:data:: DEFAULT_TIMEOUT
   :type: int
   :value: 30

   The default timeout, in seconds, used when greeting someone takes too
   long to respond.

The ``Greeter`` Class
"""""""""""""""""""""

Nesting a ``py:method`` (or any domain object) directive inside a
``py:class`` body automatically qualifies its cross-reference name with the
enclosing class's name — the signatures below are written unqualified
(``greet``, not ``Greeter.greet``), and still resolve as ``Greeter.greet``
when cross-referenced, exactly like real Sphinx.

.. py:class:: Greeter

   A greeter.

   .. py:attribute:: name
      :type: str
      :value: "anonymous"
      :canonical: greetings.Greeter.name

      The name this greeter addresses. Nesting it inside the ``Greeter``
      class body qualifies its cross-reference name to ``Greeter.name``,
      exactly like the nested methods below. The ``:canonical:`` option
      records where the attribute is really defined when documented via a
      re-export; rusty-sphinx renders it as metadata only, with no
      alias/redirect behavior in cross-reference resolution.

   .. py:method:: greet(self, name)

      Greets the given name.

   .. py:method:: create(cls)
      :classmethod:

      Creates a new ``Greeter``.

   .. py:method:: default_name()
      :staticmethod:

      Returns the default name used when none is given.

   .. py:method:: validate(cls, name)
      :classmethod:
      :abstractmethod:

      Validates a name before greeting it. Combines two modifier options at
      once — each renders as its own prefix label, in a fixed
      ``abstractmethod``/``async``/``classmethod``/``staticmethod`` order
      regardless of the order the options were written in.

   .. py:method:: greet_async(self, name)
      :async:

      An asynchronous variant of ``greet``.

.. py:class:: ImmutableGreeter
   :final:

   A ``:final:`` class, rendered with a ``final`` prefix label before
   ``class`` (not enforced by rusty-sphinx, just documented via the label,
   like real Sphinx).

   .. py:class:: Options

      A class nested inside another class — qualification composes, so this
      is indexed and cross-referenced as ``ImmutableGreeter.Options``, and
      anything nested inside *it* would be qualified two levels deep.

The :py:mod:`greetings` Module
""""""""""""""""""""""""""""""

This heading's title itself contains a cross-reference role, proving that
inline markup and domain-object roles resolve inside headings, not just in
paragraph text — the heading text above renders as a working link to the
module definition, not literal ``:py:mod:`greetings``` text.

``py:data`` Definitions Inside a Table
""""""""""""""""""""""""""""""""""""""

Real Sphinx projects (e.g. CPython's ``curses`` module docs) commonly define
a whole family of constants as ``.. data::`` directives nested inside a grid
table cell, one per row, rather than as standalone top-level directives.
Definitions nested this way are still indexed for cross-referencing, not
just rendered:

+-------------------------------------------------+
| .. py:data:: GREETING_ATTR_NORMAL               |
|                                                 |
|    Normal greeting attribute.                   |
+-------------------------------------------------+
| .. py:data:: GREETING_ATTR_BOLD                 |
|                                                 |
|    Bold greeting attribute.                     |
+-------------------------------------------------+

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

The :py:data:`DEFAULT_TIMEOUT` data item has no equivalent ``py:const``
directive in real Sphinx — instead, :py:const:`DEFAULT_TIMEOUT` is simply
an alternate role spelling for the same object, used when the author wants
to emphasize that it's a constant. Both roles resolve to the same
``.. py:data::`` definition above.

:py:const:`GREETING_ATTR_NORMAL` and :py:const:`GREETING_ATTR_BOLD` resolve
correctly even though both are defined inside the table above rather than as
top-level directives — nested definitions like these are indexed for
cross-referencing, not just rendered on the page.

A domain-object role's target may be prefixed with ``!`` to suppress the
link entirely — e.g. :py:func:`!not_a_real_function` renders as plain text
with no warning, even though no such function is defined. A ``~`` prefix
instead keeps the link but shortens the displayed text to the last dotted
component — e.g. :py:func:`~greetings.shout` links to ``greetings.shout``
but displays only ``shout``.

Call :py:meth:`Greeter.greet` to greet someone synchronously, or await
:py:meth:`Greeter.greet_async` for the asynchronous variant. The bare role
:meth:`Greeter.create` also resolves to the ``py`` domain, like the other
bare roles above. As with :py:func:`~greetings.shout`, the ``~`` prefix
shortens the display text — :py:meth:`~Greeter.validate` links to
``Greeter.validate`` but displays only ``validate``. Note that none of these
targets are written qualified anywhere in the ``Greeter`` class's own
directives above — the qualification comes entirely from nesting.

:py:class:`Greeter` and the bare role :class:`ImmutableGreeter` both
resolve to their respective ``py:class`` definitions above. The nested class
:py:class:`ImmutableGreeter.Options` resolves too, proving qualification
composes across two levels of nesting.

The :py:attr:`Greeter.name` attribute (or, using the bare role,
:attr:`Greeter.name`) links to the nested ``.. py:attribute::`` definition
above — qualification applies to nested attributes exactly as it does to
nested methods. Using the ``~`` prefix, :py:attr:`~Greeter.name` links to
the same target but displays only ``name``.
