Domains
=======

Rusty-Sphinx supports Sphinx-style domains for namespacing directives and
cross-reference roles by language, mirroring real Sphinx. This page uses
``py`` and ``c`` as the first two domains: ``c`` has a ``function`` object
type, and ``py`` additionally has ``module``, ``data``, ``method``,
``class``, ``attribute``, and ``exception`` object types.

Python Domain
-------------

.. py:function:: greet(name)

   Greets the given name.

.. function:: farewell(name)

   Says goodbye to the given name. Written without an explicit domain
   prefix — this library's default domain is ``py`` (the default), so this
   resolves to ``py:function``.

.. py:data:: DEFAULT_TIMEOUT
   :type: int
   :value: 30

   The default timeout, in seconds, used when greeting someone takes too
   long to respond.

Multiple Signatures Per Directive
"""""""""""""""""""""""""""""""""

One definition directive may declare several argument lines, each an
independently referenceable alias for the same documented object, all
sharing a single docstring. This is the shape CPython's ``library/socket.rst``
uses for its address families and ``library/re.rst`` for its flag aliases.

.. py:data:: AF_UNIX
             AF_INET
             AF_INET6

   The supported address families. All three names refer to this one
   description, and each can be cross-referenced on its own.

Continuation lines are not limited to bare names — for a function-like
object each is a full signature:

.. py:function:: spawnl(mode, file, *args)
                 spawnle(mode, file, *args, env)

   Spawn a new process. The ``e`` variant additionally takes a mapping of
   environment variables.

The ``Greeter`` Class
""""""""""""""""""""""

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

Legacy ``.. classmethod::`` / ``.. staticmethod::`` Directives
'''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''

Real Sphinx also accepts the legacy directive spellings ``.. classmethod::``
and ``.. staticmethod::`` as aliases for ``.. py:method::`` with the
corresponding option pre-set (this is the shape CPython's own ``zoneinfo``
docs use, e.g. ``.. classmethod:: ZoneInfo.clear_cache``). They are
``py``-domain only and index and cross-reference exactly like a
``.. py:method::`` carrying ``:classmethod:``/``:staticmethod:``, so the
``from_nickname`` and ``anonymous`` methods below — written with the legacy
spelling and qualified by hand (``Greeter.``) since they're documented as
siblings of the class rather than nested in it — resolve as
``Greeter.from_nickname`` and ``Greeter.anonymous``, and render with the same
``classmethod``/``staticmethod`` prefix labels the nested methods above do.

.. classmethod:: Greeter.from_nickname(nickname)

   Creates a ``Greeter`` from a nickname. Written with the legacy
   ``.. classmethod::`` spelling instead of ``.. py:method:: :classmethod:``.

.. staticmethod:: Greeter.anonymous()

   Returns a ``Greeter`` for an anonymous caller. Written with the legacy
   ``.. staticmethod::`` spelling.

.. py:class:: ImmutableGreeter
   :final:

   A ``:final:`` class, rendered with a ``final`` prefix label before
   ``class`` (not enforced by rusty-sphinx, just documented via the label,
   like real Sphinx).

   .. py:class:: Options

      A class nested inside another class — qualification composes, so this
      is indexed and cross-referenced as ``ImmutableGreeter.Options``, and
      anything nested inside *it* would be qualified two levels deep.

Custom Exceptions
""""""""""""""""""

``py:exception`` shares ``py:class``'s signature grammar and ``:final:``
option, and — since exceptions are classes in Python — the same
nesting-based cross-reference qualification for any domain object
documented inside its body.

.. py:exception:: GreeterError

   The base error raised when greeting fails.

.. py:exception:: InvalidNameError(GreeterError)
   :final:

   A ``:final:`` exception class, rendered with a ``final`` prefix label
   before ``exception``, exactly like ``py:class``'s ``:final:`` label.

   .. py:method:: reason(self)

      Returns why the name was rejected. Nesting it inside
      ``InvalidNameError`` qualifies its cross-reference name to
      ``InvalidNameError.reason``, exactly like a method nested in a class.

Role-Target Aliasing (``class`` / ``exception``)
""""""""""""""""""""""""""""""""""""""""""""""""

Real Sphinx treats ``class`` and ``exception`` as mutually resolvable role
targets: a ``.. py:class::`` definition can be referenced via ``:exc:`` as
well as ``:class:``, and vice versa. This mirrors CPython's own
``xmlrpc.client`` docs, which define ``Fault`` via ``.. class::`` but
reference it via ``:exc:`Fault``` throughout.

.. py:class:: Fault

   Encapsulates the content of an XML-RPC fault tag.

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

The :py:mod:`greetings` Module
""""""""""""""""""""""""""""""

This heading's title itself contains a cross-reference role, proving that
inline markup and domain-object roles resolve inside headings, not just in
paragraph text — the heading text above renders as a working link to the
module definition, not literal ``:py:mod:`greetings``` text.

.. py:module:: greetings
   :platform: Unix, Windows
   :synopsis: Greeting utilities.
   :deprecated:

   A module holding the ``greet`` and ``farewell`` functions.

.. py:function:: shout(name)

   Greets the given name loudly. Written unqualified — a domain object
   documented as a sibling *after* a ``py:module`` directive (real Sphinx
   docs never nest them; ``py:module`` has no indented content of its own)
   is automatically qualified with that module's name for cross-referencing,
   so this resolves as ``greetings.shout`` without writing the prefix by
   hand, exactly like real Sphinx. This is also why the ``greetings``
   module is documented last among this page's ``py`` domain objects —
   real Sphinx's module context is sequential and stays in effect for the
   rest of the document until changed, so anything documented after a
   ``py:module`` directive is implicitly considered part of it.

C Domain
--------

.. c:function:: int add(int a, int b)

   Adds two numbers and returns the sum.

.. c:function:: char *duplicate_greeting(const char *name)

   Returns a newly-allocated greeting string for *name*. Demonstrates a
   pointer-return-type signature (the return type's ``*`` glued to the
   function name, as real-world C API docs like CPython's
   ``PyUnicode_FromString`` commonly write it). ``c:function`` is
   unaffected by ``py:module``'s current-module tracking above — module
   context is a ``py``-domain-only concept in real Sphinx, so it never
   qualifies objects in other domains, even when documented afterward in
   the same file.

.. c:macro:: PY_SSIZE_T_MAX

   The maximum value of a ``Py_ssize_t``. An object-like macro: no
   parentheses, so no signature to parse beyond the bare name.

.. c:macro:: MAX(a, b)

   Expands to whichever of *a* or *b* is greater. A function-like macro:
   unlike ``c:function``, there is no return type or parameter types to
   strip from the signature.

.. c:struct:: Data

   A data record. Nesting a ``c:member`` inside a ``c:struct``/``c:union``
   body auto-qualifies its bare name against the enclosing container, the
   same way a ``py:method`` written inside ``py:class`` picks up its class
   prefix.

   .. c:member:: int count

      A count, indexed as ``Data.count`` even though the signature above
      only says ``count`` — the enclosing ``c:struct:: Data`` supplies the
      rest.

.. c:union:: Number

   A numeric union, demonstrating that ``c:member`` nests under
   ``c:union`` exactly like it does under ``c:struct``.

   .. c:member:: int as_int

      The integer view of the union's value, indexed as ``Number.as_int``.

.. c:member:: PyObject *PyTypeObject.tp_bases

   The type's base classes. The real CPython-docs shape: no enclosing
   ``.. c:struct::`` at all — the struct name is simply written as part of
   the dotted signature instead of relying on nesting.

.. c:var:: int errno

   The last error number set by a failed system call. ``.. c:var::`` is a
   pure directive-name alias for ``.. c:member::`` in real Sphinx — both
   produce the exact same kind of definition, just spelled differently.

.. c:member:: int hidden_field
   :no-index:

   A member with ``:no-index:`` set: it is still typeset here, but gets no
   cross-reference target at all (and, since ``no-index`` implies
   ``no-index-entry``, no general-index entry either) — a role trying to
   reference ``hidden_field`` would not resolve.

.. c:member:: int quiet_field
   :no-index-entry:

   A member with only ``:no-index-entry:`` set: it still gets a
   cross-reference target (:c:member:`quiet_field` resolves normally), but
   is left out of the general index page — which would list neither this
   nor the ``:no-index:`` member above.

.. c:type:: PyMemAllocatorDomain

   Enumeration of possible memory allocator domains, the real-world CPython
   ``c-api/memory.rst`` shape that motivated ``c:type`` support (previously
   every definition nested inside a ``c:type`` body was silently dropped,
   since the directive wasn't recognized at all).

   .. c:macro:: PYMEM_DOMAIN_RAW

      The raw domain. Nesting a ``c:macro`` inside ``c:type`` does *not*
      qualify its name with the enclosing type — unlike ``c:member``,
      ``c:macro`` (like ``c:function``) never consults the enclosing C
      scope regardless of what it's nested under, so this is indexed as the
      bare ``PYMEM_DOMAIN_RAW``, not ``PyMemAllocatorDomain.PYMEM_DOMAIN_RAW``.
      This happens to match real Sphinx's actual rendered output too, though
      for a different reason: real CPython's docs precede these macros with
      ``.. c:namespace:: NULL`` to reset what would otherwise be automatic
      qualification — ``c:namespace`` itself isn't implemented here yet.

.. c:type:: unsigned long ulong

   A ``type name`` typedef-alias signature, real Sphinx's other ``c:type``
   form (as opposed to ``PyMemAllocatorDomain``'s bare-name form above).

.. c:type:: int (*Py_tracefunc)(PyObject *obj, PyFrameObject *frame, int what, PyObject *arg)

   A function-pointer typedef, real Sphinx's third ``c:type`` signature
   form and the shape CPython's ``c-api/init.rst`` declares ``Py_tracefunc``
   with. The declared name sits inside the ``(*NAME)`` group rather than
   before the first parenthesis, so recovering it takes a real C declarator
   parse — a heuristic reading the text before the first ``(`` would index
   this under the return type ``int`` instead.

.. c:type:: PyObject *(*unaryfunc)(PyObject *)

   The same shape with a pointer return type, as CPython's
   ``c-api/typeobj.rst`` declares its slot typedefs.

.. c:type:: int (*callbacks[8])(void *state)

   An array declarator: ``callbacks`` is an array of function pointers.
   Array and grouped declarators compose, and the name is still found at
   the innermost position.

.. c:type:: Hidden
   :no-index:

   A type with ``:no-index:`` set: it is still typeset here, but gets no
   cross-reference target at all, exactly like the ``c:member`` no-index
   example above.

.. c:type:: Quiet
   :no-index-entry:

   A type with only ``:no-index-entry:`` set: it still gets a
   cross-reference target (:c:type:`Quiet` resolves normally), but is left
   out of the general index page.

Cross-References
-----------------

Call :py:func:`greet` to greet someone, or :c:func:`add` to add two numbers.
See :c:func:`duplicate_greeting` for the pointer-return-type function above.
The bare role :func:`farewell` also resolves to the ``py`` domain, since
that is this library's default. The :py:mod:`greetings` module holds both
of them.

See also Team A's :c:func:`subtract`, defined in a library whose
``default_domain`` is set to ``c``.

The :c:macro:`PY_SSIZE_T_MAX` macro and the :c:macro:`MAX` function-like
macro are both defined above. Like ``:c:func:``, ``:c:macro:`` is C-only —
there is no ``py:macro`` equivalent.

:c:data:`PY_SSIZE_T_MAX` and :c:var:`PY_SSIZE_T_MAX` both resolve against
the ``.. c:macro::`` definition above rather than a ``.. c:member::``/
``.. c:var::`` definition of their own — real Sphinx's C domain resolves a
role by name without checking it against the matched declaration's actual
object type, and real-world docs like CPython's ``c-api/module.rst`` rely on
exactly this to reference macro-defined slot constants (e.g.
``Py_mod_exec``) via ``:c:data:``.

:c:struct:`Data` and :c:union:`Number` reference the two containers above.
:c:member:`Data.count` and :c:member:`Number.as_int` reference their nested
members by full dotted name; the same target also resolves via
:c:data:`Data.count` or :c:var:`Data.count`, since real Sphinx documents
``member``/``data``/``var`` as equivalent role spellings for one object
type — unlike the ``PY_SSIZE_T_MAX`` case above, this one *does* have a real
``.. c:member::`` definition backing it. A dot-prefixed target,
:c:member:`.count`, finds the same nested member via suffix search without
repeating ``Data.`` at all — the same mechanism that lets :py:func:`.shout`
(demonstrated further below) reach into a module it never names.

:c:member:`PyTypeObject.tp_bases` references the flat, non-nested member
defined above, and :c:var:`errno` references the ``.. c:var::`` global,
confirming it produced the same kind of definition ``.. c:member::`` does.

:c:type:`PyMemAllocatorDomain` and :c:type:`unsigned long ulong <ulong>`
reference the two ``c:type`` definitions above — the second using explicit-title
syntax, since ``ulong``'s own two-token signature isn't a valid target by
itself. :c:macro:`PYMEM_DOMAIN_RAW` reaches the macro nested inside
``PyMemAllocatorDomain``'s body by its bare name, confirming that nesting a
macro under ``c:type`` leaves it unqualified. Unlike ``c:struct``/``c:union``
and their ``c:member`` aliases, ``:c:type:`` has no real-Sphinx cross-role
looseness with any other object type — it only resolves against
``.. c:type::`` definitions.

:c:type:`Py_tracefunc`, :c:type:`unaryfunc` and :c:type:`callbacks` reference
the three declarator-shaped definitions above by the name their declarators
bind, none of which is the leading token of the signature. Each is parsed by
``rusty_sphinx_cdecl``; a signature whose grammar it cannot handle still
yields a target via a name heuristic, and reports a parser diagnostic rather
than being dropped.

The :py:data:`DEFAULT_TIMEOUT` data item has no equivalent ``py:const``
directive in real Sphinx — instead, :py:const:`DEFAULT_TIMEOUT` is simply
an alternate role spelling for the same object, used when the author wants
to emphasize that it's a constant. Both roles resolve to the same
``.. py:data::`` definition above.

Every name a multi-signature directive declares resolves independently:
:py:data:`AF_UNIX`, :py:data:`AF_INET`, and :py:data:`AF_INET6` all link to
the same description, as do :py:func:`spawnl` and :py:func:`spawnle`.

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

A domain-object role's target may also use Sphinx's explicit-title syntax,
``text <target>``, to display custom text while resolving against a
different target — e.g. :py:func:`the greet function <greet>` displays "the
greet function" but links to the ``greet`` function defined above, exactly
like :ref:`the site home <home-index>` already does for ``:ref:``.

Calling :py:func:`shout` — written bare, with no ``greetings.`` prefix — is
the same as calling :py:func:`~greetings.shout`. This is the shape real-world
Sphinx docs actually use (e.g. every CPython module page cross-references its
own members unqualified): resolution first tries the reference against the
current module/class scope (here, ``greetings``, still in effect from the
``py:module`` directive above) before falling back to a global, unqualified
lookup, mirroring how ``shout``'s own definition was qualified when indexed.

Call :py:meth:`Greeter.greet` to greet someone synchronously, or await
:py:meth:`Greeter.greet_async` for the asynchronous variant. The bare role
:meth:`Greeter.create` also resolves to the ``py`` domain, like the other
bare roles above. As with :py:func:`~greetings.shout`, the ``~`` prefix
shortens the display text — :py:meth:`~Greeter.validate` links to
``Greeter.validate`` but displays only ``validate``. Note that none of these
targets are written qualified anywhere in the ``Greeter`` class's own
directives above — the qualification comes entirely from nesting.

The :py:meth:`Greeter.from_nickname` classmethod and :py:meth:`Greeter.anonymous`
staticmethod — both defined above with the legacy ``.. classmethod::`` /
``.. staticmethod::`` directive spellings — resolve through the ordinary
``:py:meth:`` role just like the ones written as ``.. py:method::``, proving
the legacy aliases are indexed identically.

:py:class:`Greeter` and the bare role :class:`ImmutableGreeter` both
resolve to their respective ``py:class`` definitions above. The nested class
:py:class:`ImmutableGreeter.Options` resolves too, proving qualification
composes across two levels of nesting.

The :py:attr:`Greeter.name` attribute (or, using the bare role,
:attr:`Greeter.name`) links to the nested ``.. py:attribute::`` definition
above — qualification applies to nested attributes exactly as it does to
nested methods. Using the ``~`` prefix, :py:attr:`~Greeter.name` links to
the same target but displays only ``name``.

Raising a :py:exc:`GreeterError` (or, using the bare role,
:exc:`GreeterError`) signals a greeting failure; :py:exc:`InvalidNameError`
is its ``:final:`` subclass. The nested method
:py:meth:`InvalidNameError.reason` resolves too, proving ``py:exception``
bodies qualify nested domain objects exactly like ``py:class`` bodies do. As
with the other roles above, ``!``/``~`` prefixes work the same way —
:py:exc:`!NotARealError` renders as plain text with no warning, and
:py:meth:`~InvalidNameError.reason` links to ``InvalidNameError.reason``
but displays only ``reason``.

Both :py:exc:`Fault` and :py:class:`Fault` (or, using the bare roles,
:exc:`Fault` and :class:`Fault`) resolve to the same ``.. py:class::``
definition above, even though it was documented as a ``class``, not an
``exception`` — proving the two object types alias each other for
cross-reference purposes, exactly as real Sphinx does.

The Flat ``clock.now`` Classmethod
-----------------------------------

Real-world Sphinx docs (CPython's own ``datetime`` module is the canonical
example) sometimes document a class's members as flat, column-0 siblings of
the class itself — and give the class the *same name* as its own module
(CPython's ``datetime`` module defines a class also named ``datetime``).
When that happens, a flat signature repeating the module's name is the
*class* name, not a repeat of the module, and must not be collapsed away:
``.. py:classmethod:: clock.now`` below is written as a sibling of both
``.. py:module:: clock`` and ``.. py:class:: clock`` (never nested inside the
class), and is indexed and cross-referenced as ``clock.clock.now`` — the
module, then the class, then the method — not ``clock.now``. This section is
placed last, after every other cross-reference on this page, since
``py:module`` context is sequential and stays in effect for the rest of the
document — placing it earlier would shift what the ``greetings``-relative
bare references above resolve against.

.. py:module:: clock

   A module for telling the time.

.. py:class:: clock

   Represents a point in time.

.. py:classmethod:: clock.now()

   Returns the current time. Written as a flat, unnested sibling signature —
   compare with :py:meth:`Greeter.greet` earlier on this page, which is
   nested instead.

:py:meth:`clock.clock.now` resolves to the classmethod above, proving the
module and class prefixes are both preserved rather than collapsed into each
other just because they're spelled the same.

Target Resolution Order
------------------------

Sphinx searches a role's target in one of two orders, chosen by whether the
target is written with a leading dot. Without a dot, the target is tried
unqualified first and only then against progressively more of the enclosing
scope, so a global name wins; with a dot, that order is reversed, so the
nearby name wins. The ``clock`` module above is the ideal place to show the
difference, because a module and a class there share the same name:

:py:mod:`clock` — no dot — matches the unqualified name first, which is the
``py:module`` itself. :py:class:`.clock` — same spelling, leading dot — tries
the current module's scope first instead and so reaches the ``py:class``
``clock.clock``. The dot is markup: it steers the lookup and is never shown
to the reader, so both of those render as ``clock``.

A dotted target may also be qualified further: :py:meth:`.clock.now` resolves
to ``clock.clock.now``, the classmethod defined above, by prepending the
current module to the target as written.

When a dot-prefixed target matches nothing exactly, Sphinx falls back to
treating it as a *suffix* and searching every documented object for it — this
is how a reference can reach into a module it never names. :py:func:`.shout`
resolves to ``greetings.shout`` that way: the current module here is
``clock``, so neither ``clock.shout`` nor a bare ``shout`` exists, and only
the suffix search finds it.

Only a suffix search that matches exactly one object resolves. If several
objects share a suffix, the reference is reported as ambiguous, listing the
candidates, and is left unlinked rather than silently pointing at whichever
one happened to sort first — so no such case appears on this page, which is
built with ``strict_links``.
