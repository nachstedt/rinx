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

Decorator Directives
'''''''''''''''''''''

Real Sphinx also has ``.. decorator::``/``.. decoratormethod::`` directives
for documenting decorators — CPython's own ``Doc/reference/datamodel``
documents the ``classmethod``/``staticmethod`` builtins this way. Both are
directive-name aliases too, but onto ``py:function``/``py:method`` rather
than forcing an option flag the way ``.. classmethod::``/``.. staticmethod::``
do: real Sphinx's ``PyDecoratorFunction``/``PyDecoratorMethod`` register
exactly as ``py:function``/``py:method`` (so plain ``:func:``/``:meth:``
references resolve them, with no dedicated ``:deco:`` role — real Sphinx
defines none), and additionally prefix the rendered signature with a literal
``@``.

.. decorator:: classmethod

   Transform a method into a class method, the built-in decorator itself.
   Written with ``.. decorator::`` rather than ``.. function::``.

.. py:class:: Registry

   A plugin registry.

   .. decoratormethod:: register(cls)

      Registers *cls* as a plugin, for use as ``@Registry.register``. Nested
      inside ``Registry`` like an ordinary method, so it's indexed and
      cross-referenced as ``Registry.register``.

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

The ``:module:`` Option Override
""""""""""""""""""""""""""""""""

Every ``py:*`` object-description directive except ``py:module`` itself
(which has no such option — it *is* the module declaration) accepts a
``:module:`` option that overrides the ambient module context for that one
definition and its nested body only, then restores it afterward. Real
CPython docs use this to document a class on a *different* module's page
than the one it lives under — ``Doc/library/multiprocessing.shared_memory.rst``
documents ``SharedMemoryManager`` this way, which is the shape reproduced
below.

.. py:module:: multiprocessing.shared_memory

   Provides shared memory for direct access across processes.

.. py:class:: SharedMemoryManager([address[, authkey]])
   :module: multiprocessing.managers

   Written under the ``multiprocessing.shared_memory`` module above, but the
   ``:module:`` option qualifies this as
   ``multiprocessing.managers.SharedMemoryManager`` instead — a subclass of
   ``BaseManager`` which can be used for the management of shared memory
   blocks across processes.

   .. py:method:: get_server()

      Nested inside the overridden class, so it inherits the override too:
      this qualifies as
      ``multiprocessing.managers.SharedMemoryManager.get_server``, not
      ``multiprocessing.shared_memory.SharedMemoryManager.get_server``.

.. py:function:: track(size)

   Documented as a sibling *after* the overridden class, with no
   ``:module:`` of its own — the override is scoped to
   ``SharedMemoryManager`` alone (and its nested body), so this reverts to
   the enclosing ``multiprocessing.shared_memory`` module, qualifying as
   ``multiprocessing.shared_memory.track`` rather than
   ``multiprocessing.managers.track``.

.. currentmodule:: greetings

Restores the current module to ``greetings`` for the rest of this page —
real Sphinx's module context is sequential and document-order, so without
this the bare ``shout`` references further below would resolve against
``multiprocessing.shared_memory`` instead.

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

   Referencing :c:member:`count` right here, unqualified and with no leading
   dot, resolves against the enclosing ``c:struct`` scope — it was written
   this way in real CPython docs (see ``c-api/long.rst``'s ``digits``) and
   used to be a broken link, since resolution only ever consulted the
   ``py`` domain's scope, never the ``c`` domain's own container nesting.

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

   .. c:namespace:: NULL

   .. c:macro:: PYMEM_DOMAIN_RAW

      The raw domain. Nesting a ``c:macro`` inside ``c:type`` would normally
      qualify its name with the enclosing type, exactly like ``c:member``
      does — every ``c``-domain object qualifies against the same enclosing
      scope, matching real Sphinx's C domain, which nests *any* declaration
      generically off whatever declaration it's indented under.

      The ``.. c:namespace:: NULL`` above resets that scope back to global,
      so this constant is indexed bare as ``PYMEM_DOMAIN_RAW`` rather than
      ``PyMemAllocatorDomain.PYMEM_DOMAIN_RAW``. This is verbatim the shape
      real CPython's ``c-api/memory.rst`` uses, and the reason its enum-style
      constants document under their bare names despite being written inside
      the type's body.

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

C Namespaces
------------

The ``c:namespace`` family moves the current ``c``-domain scope without
documenting anything itself — the ``c``-domain counterpart to
``py:currentmodule``. It shares one scope with the automatic nesting the
``c:struct``/``c:union``/``c:type`` bodies above establish, which is what
lets the ``.. c:namespace:: NULL`` inside ``PyMemAllocatorDomain`` reset
that body's qualification.

``.. c:namespace::`` sets the scope *absolutely* and resets the push/pop
stack:

.. c:namespace:: Outer.Inner

.. c:macro:: NAMESPACED_CONSTANT

   Declared under ``.. c:namespace:: Outer.Inner``, so it is indexed as
   ``Outer.Inner.NAMESPACED_CONSTANT``.

``.. c:namespace-push::`` extends the current scope *relatively*, and
``.. c:namespace-pop::`` undoes that push in its entirety — not merely one
dotted segment of it:

.. c:namespace-push:: Deeper.Still

.. c:macro:: PUSHED_CONSTANT

   Declared after pushing ``Deeper.Still`` onto ``Outer.Inner``, so it is
   indexed as ``Outer.Inner.Deeper.Still.PUSHED_CONSTANT``.

.. c:namespace-pop::

.. c:macro:: POPPED_CONSTANT

   Declared after the pop. The whole two-segment ``Deeper.Still`` push is
   undone at once, so this is back to ``Outer.Inner.POPPED_CONSTANT`` — not
   ``Outer.Inner.Deeper.POPPED_CONSTANT``.

A reference written while a namespace is current is searched for starting in
that scope, so :c:macro:`NAMESPACED_CONSTANT` resolves here by its bare name
even though it is indexed fully qualified.

.. c:namespace:: NULL

.. c:macro:: GLOBAL_AGAIN

   ``.. c:namespace:: NULL`` (``0`` works too) resets to global scope, so
   this is indexed bare — and the namespace no longer leaks into the
   sections below.

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

The same looseness runs between ``c:function`` and ``c:macro``, in both
directions. :c:func:`MAX` resolves against the ``.. c:macro:: MAX(a, b)``
definition above, the shape CPython's ``c-api/gcsupport.rst`` uses when it
documents the function-like macro ``Py_VISIT`` with ``.. c:macro::`` and then
references it as ``:c:func:`Py_VISIT```. Conversely :c:macro:`add` resolves
against the ``.. c:function:: int add(int a, int b)`` definition, the shape
``c-api/structures.rst`` uses when it references ``Py_REFCNT`` — declared
``.. c:function::`` over in ``c-api/refcounting.rst`` — as
``:c:macro:`Py_REFCNT```. Both files build clean under real Sphinx's
nit-picky mode, so both really do resolve there.

Aliasing is mutual but *not* chained: ``c:function`` and ``c:macro`` accept
each other, and ``c:macro`` and ``c:member`` accept each other, yet
``c:function`` and ``c:member`` do not — each pair is modelled because
real-world docs were found to collide on it, not because the C domain is
treated as type-free in general. A role still has to name a compatible object
type, and resolving through an alias is reported as a soft object-type
mismatch rather than passing silently.

:c:struct:`Data` and :c:union:`Number` reference the two containers above.
:c:member:`Data.count` and :c:member:`Number.as_int` reference their nested
members by full dotted name; the same target also resolves via
:c:data:`Data.count` or :c:var:`Data.count`, since real Sphinx documents
``member``/``data``/``var`` as equivalent role spellings for one object
type — unlike the ``PY_SSIZE_T_MAX`` case above, this one *does* have a real
``.. c:member::`` definition backing it. A dot-prefixed target,
:c:member:`.count`, finds the same nested member via suffix search without
repeating ``Data.`` at all — the same mechanism that lets :py:func:`.shout`
(demonstrated further below) reach into a module it never names. An
unqualified target written *inside* ``Data``'s own body (see the
``c:struct`` above) resolves too, via the enclosing ``c:struct`` container
scope rather than the suffix search.

:c:member:`PyTypeObject.tp_bases` references the flat, non-nested member
defined above, and :c:var:`errno` references the ``.. c:var::`` global,
confirming it produced the same kind of definition ``.. c:member::`` does.

:c:type:`PyMemAllocatorDomain` and :c:type:`unsigned long ulong <ulong>`
reference the two ``c:type`` definitions above — the second using explicit-title
syntax, since ``ulong``'s own two-token signature isn't a valid target by
itself. :c:macro:`PYMEM_DOMAIN_RAW` reaches the macro nested inside
``PyMemAllocatorDomain``'s body by its *bare* name, confirming that the
``.. c:namespace:: NULL`` written in that body reset the qualification the
enclosing ``c:type`` would otherwise have applied. Unlike the
``c:macro``/``c:member`` and ``c:function``/``c:macro`` pairs above,
``:c:type:`` has no confirmed cross-role collision with any other object
type — it only resolves against ``.. c:type::`` definitions.

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

A target may also carry a trailing ``()``, so the reference reads as a call at
the point of use — the shape CPython's C API docs use throughout, e.g.
``:c:func:`Py_TYPE()```. The parens are markup, not part of any name:
:c:func:`add()` and :c:macro:`MAX()` resolve against the plain
``.. c:function:: int add(int a, int b)`` and ``.. c:macro:: MAX(a, b)``
definitions above, because a declaration's name is always cut at its first
``(``. The reader still sees ``add()`` and ``MAX()``. This is not a C-domain
quirk: :py:func:`greet()` resolves the same way, since Sphinx's Python domain
strips a trailing ``()`` from the target too.

The two domains differ in exactly one respect, and that difference is
reproduced rather than smoothed over. The C domain skips whitespace before the
parens, so :c:func:`add ()` still resolves; the Python domain does not, so a
Python target written ``:py:func:`greet ()``` stays unresolved and is reported,
instead of being quietly repaired into something the author never wrote.

The trailing ``()`` composes with the prefixes above. :py:func:`~greetings.shout()`
shortens the display to ``shout()`` while still resolving ``greetings.shout``.
:py:func:`the greet function <greet()>` puts the parens on the *target* inside
the angle brackets, where they are stripped and never seen, since an explicit
title decides the display on its own. And :py:func:`!not_a_real_function()`
suppresses the link entirely, leaving ``not_a_real_function()`` as literal text
that is never looked up — so, as with every ``!`` target, no warning either.

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

The ``.. decorator::`` example above defines :func:`classmethod` — indexed
as a plain ``py:function``, so the ordinary ``:func:`` role resolves it, and
its rendered signature is prefixed ``@classmethod``. Likewise
:meth:`Registry.register`, defined via ``.. decoratormethod::`` nested inside
``Registry``, resolves through ``:meth:`` exactly like a nested
``.. py:method::`` would, and renders as ``@register(cls)``.

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

Standard Domain
------------------

The ``std`` domain has no language affiliation. ``.. option::`` (and its
legacy directive-name alias ``.. cmdoption::``) documents a command-line
flag; ``.. program::`` sets the "current program" flags are scoped under, so
the same flag name (e.g. ``-h``) can be documented once per tool without
colliding. Neither is gated by this library's ``default_domain`` the way
``py``/``c`` directives are — both are recognized unconditionally, and
neither affects the ``py``/``c`` scope state used above on this page.

.. program:: greet

.. option:: -c <name>, --config <name>

   Load configuration from *name*. Comma-separated specs on one line share a
   single ``<dt>`` — both ``-c`` and ``--config`` resolve independently
   (as ``greet.-c``/``greet.--config``), but are typeset together.

.. cmdoption:: -q
                --quiet

   Suppress the banner. Written one flag per line instead of
   comma-separated: real Sphinx (and this renderer) treats each *line* as
   its own signature, so ``-q`` and ``--quiet`` each get their own ``<dt>``,
   sharing this description — unlike the comma-joined pair above.

.. option:: -h

   Show a short help message and exit. Still under the ``greet`` program
   set above: indexed as ``greet.-h``.

:option:`-h` and :option:`--config` both resolve here, against the ambient
``.. program:: greet`` in effect at this point in the document — the same
document-order persistence ``.. py:currentmodule::``/``.. c:namespace::``
already have. The explicit-title form also works:
:option:`the config flag <-c>` links to the same target as :option:`-c` but
displays different text.

.. program:: None

.. option:: -x

   A bare, no-program option — the shape CPython's own top-level
   ``using/cmdline.rst`` uses for e.g. ``-X``: no ``.. program::`` at all.
   Indexed as a bare ``-x``, the ``.. program:: None`` reset form above
   having cleared the ambient program.

:option:`-x` resolves here with no ambient program in effect. A reference
can also *name* a different program directly inside its target, regardless
of what is ambient where the role is written: :option:`--config <greet --config>`
still finds ``greet.--config`` even though no ``.. program::`` is currently
in effect — the reference text's own leading word is peeled off and tried as
an explicit program name once the ambient-scoped and bare lookups both miss.
This is the same idiom real CPython docs use to cross-reference one tool's
options from another tool's page — e.g. ``dis.rst`` referencing
``ast.rst``'s ``--feature-version`` as
``:option:`--feature-version <ast --feature-version>```.
