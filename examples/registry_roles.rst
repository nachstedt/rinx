.. _registry-roles-example:

Registry Roles: ``:pep:``, ``:rfc:``, ``:cve:`` and ``:cwe:``
=============================================================

Four roles link a numbered document of a registry outside the site, and list
each mention in the site-wide general index (``genindex.html``, linked from
the sidebar), linking back to the sentence it was written in.

The ``:pep:`` role
------------------

``:pep:`` links a Python Enhancement Proposal: :pep:`8` is the style guide,
written as ``:pep:`8```. The number is padded to the PEP's page name, so the
link goes to ``https://peps.python.org/pep-0008/``. Its index entries are
filed under *Python Enhancement Proposals*.

The ``:rfc:`` role
------------------

``:rfc:`` links an IETF Request for Comments: :rfc:`2324`, written as
``:rfc:`2324```, links ``https://datatracker.ietf.org/doc/html/rfc2324.html``.
A ``section-``, ``appendix-`` or ``page-`` anchor is spelled out in the link
text, as Sphinx does: :rfc:`2324#section-2.3.2` is written as
``:rfc:`2324#section-2.3.2```. Its index entries are filed under *RFC*.

The ``:cve:`` role
------------------

``:cve:`` links a Common Vulnerabilities and Exposures record by its year
and sequence number: :cve:`2024-3094`, written as ``:cve:`2024-3094```, links
``https://www.cve.org/CVERecord?id=CVE-2024-3094``. Its index entries are
filed under *Common Vulnerabilities and Exposures*.

The ``:cwe:`` role
------------------

``:cwe:`` links a Common Weakness Enumeration entry: :cwe:`787`, written as
``:cwe:`787```, links ``https://cwe.mitre.org/data/definitions/787.html``.
Its index entries are filed under *Common Weakness Enumeration*.

An explicit title
-----------------

An explicit title replaces the ``PEP 20`` text on any of the four:
:pep:`The Zen of Python <20>`, written as ``:pep:`The Zen of Python <20>```,
or :cwe:`Out-of-bounds Write <787>`.

A section of a document
-----------------------

A ``#`` and an anchor link one section, and — as in Sphinx — the anchor stays
in the text: :pep:`484#type-aliases`.

A target that is not one
------------------------

A target its registry cannot link is shown as written and reported, as
``pep.invalid-number``, ``rfc.invalid-number``, ``cwe.invalid-number`` or
``cve.invalid-id``; the comment below silences those here.

.. noqa: pep.invalid-number, rfc.invalid-number, cwe.invalid-number, cve.invalid-id

:pep:`eight`, :rfc:`HTTP`, :cwe:`CWE-787` and :cve:`CVE-2024-3094`.

A ``:cve:`` target is checked where Sphinx checks nothing: it must be a year
and a sequence number, so the full identifier ``CVE-2024-3094`` — which would
link ``CVE-CVE-2024-3094`` — is refused with a hint to drop the prefix.

The registry indexes
--------------------

PEP links go to ``https://peps.python.org/`` and RFC links to
``https://datatracker.ietf.org/doc/html/`` unless the site's ``rinx.toml``
sets ``pep_base_url`` or ``rfc_base_url``, for example to a mirror. CVE and
CWE links go to fixed addresses, as in Sphinx.

docutils' ``:pep-reference:`` and ``:rfc-reference:``
-----------------------------------------------------

docutils' own roles, which Sphinx leaves in place beside ``:pep:`` and
``:rfc:``, link the document and nothing more: :pep-reference:`8`, written as
``:pep-reference:`8```, and :rfc-reference:`2822#section-3`, written as
``:rfc-reference:`2822#section-3```. They make no general index entry and
show no bold text. A ``:pep-reference:`` link, ``https://peps.python.org/pep-0008``,
has no trailing slash; an ``:rfc-reference:`` keeps its ``#`` section in the
link but not in the text.

A ``:pep-reference:`` target is the number alone, from 0 to 9999, and an
``:rfc-reference:`` target a number of at least 1 with an optional ``#``
section; an explicit title is not a number. Anything else is shown as
written and reported as ``pep-reference.invalid-number`` or
``rfc-reference.invalid-number``; the comment below silences those here.

.. noqa: pep-reference.invalid-number, rfc-reference.invalid-number

:pep-reference:`8#naming` and :rfc-reference:`0`.
