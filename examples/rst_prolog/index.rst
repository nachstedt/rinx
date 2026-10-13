.. _rst-prolog-index:

Prolog and Epilog
=================

This document defines no substitution, role or link target, yet uses all
three. Its library parses ``prolog.rst`` at the start of every document and
``epilog.rst`` at the end, as a Sphinx project's ``rst_prolog`` and
``rst_epilog`` do:

* A substitution from the prolog: |project| |release|.
* A role from the prolog: :python:`print("hello")`.
* A link target from the epilog: the `rinx repository`_.

A mistake in either file is reported at its own line in that file, not in the
document it was parsed with.
