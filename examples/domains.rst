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

C Domain
--------

.. c:function:: int add(int a, int b)

   Adds two numbers and returns the sum.

Cross-References
-----------------

Call :py:func:`greet` to greet someone, or :c:func:`add` to add two numbers.
The bare role :func:`farewell` also resolves to the ``py`` domain, since
that is this library's default.

See also Team A's :c:func:`subtract`, defined in a library whose
``default_domain`` is set to ``c``.
