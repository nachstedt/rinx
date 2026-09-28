.. _doc-role-example:

The ``:doc:`` Role
==================

``:doc:`` links a whole page by its document name — the path of its source
file without the ``.rst`` — and shows the page's title.

Naming a document
-----------------

* Relative to this page, as a ``.. toctree::`` entry is:
  :doc:`glossary`, and into a subdirectory, :doc:`toctree/entry_forms`.
* From the source root, with a leading ``/`` — which for a Bazel build is the
  workspace root, where every document's path starts, so this site's pages
  are under ``/examples``: :doc:`/examples/any_role`.
* Spelled with its domain, as Sphinx also accepts it: :std:doc:`index`.
* From a page in a subdirectory, climbing out with ``..`` — the
  :doc:`toctree/index` page links back here as ``:doc:`../doc_role```.

The link text
-------------

* A bare ``:doc:`` shows the target page's title: :doc:`math`.
* An explicit title wins: :doc:`the glossary page <glossary>`.
* A page with no title of its own shows ``<no title>``, as in Sphinx:
  :doc:`doc_role_untitled`.
* A leading ``!`` shows the text without looking it up, and — as in Sphinx —
  shows all of it: :doc:`!Glossary <glossary>`.

An unknown document is reported as ``link.broken-doc``. It is suppressed here,
since this site is built with ``strict_links = True``:

.. noqa: link.broken-doc

:doc:`no-such-page`
