The ``:download:`` Role
=======================

``:download:`` links a file the reader can save — a script, a data file, a
picture. The build copies the file into the site's ``_downloads/`` directory
and the link points there. In a Bazel build the file must be declared in the
library's ``downloads`` attribute; an undeclared one fails the build, reported
as ``download.undeclared``.

Naming a file
-------------

* Relative to this page: :download:`data/fruits.csv`.
* From the source root, with a leading ``/`` — which for a Bazel build is the
  workspace root, so this site's files are under ``/examples``:
  :download:`/examples/shared/greeter.py`.
* Climbing out with ``..`` works too: :download:`../examples/data/logo.svg`.
* Spelled with its domain, as Sphinx also accepts it:
  :std:download:`data/fruits.csv`.

The same file may be both shown and offered: ``logo.svg`` is declared in
``images`` for the :doc:`images` page and in ``downloads`` for this one, and
``greeter.py`` in ``parse_data`` for the :doc:`includes` page's
``.. literalinclude::``. The three attributes answer different questions, so
declaring a file in one says nothing about the others.

The link text
-------------

* A bare ``:download:`` shows the target as written, as in Sphinx:
  :download:`data/fruits.csv`.
* An explicit title wins: :download:`the fruit table <data/fruits.csv>`.
* A leading ``!`` shows the text as a literal, links nothing and copies
  nothing: :download:`!data/not-shipped.csv`.

External files
--------------

A URL is linked as written and never copied, so it needs no declaration:
:download:`the Sphinx sources <https://github.com/sphinx-doc/sphinx/archive/refs/heads/master.zip>`.
