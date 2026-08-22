Deliberately Failing Doctests
=============================

This document exists only for ``tests/test_doctest_failure.sh``. Its doctest
target is tagged ``manual`` so a plain ``bazel test //...`` stays green, and it
is deliberately not part of ``//examples:site``.

The expected output below is wrong on purpose:

.. testcode::

   print("actual value")

.. testoutput::

   a completely different value
