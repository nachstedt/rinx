.. _pep-role-example:

The ``:pep:`` Role
==================

``:pep:`` links a Python Enhancement Proposal: :pep:`8` is the style guide,
written as ``:pep:`8```. The number is padded to the PEP's page name, so the
link goes to ``https://peps.python.org/pep-0008/``.

Each mention is also an entry in the site-wide general index
(``genindex.html``, linked from the sidebar), under *Python Enhancement
Proposals*, linking back to the sentence it was written in.

An explicit title
-----------------

An explicit title replaces the ``PEP 20`` text: :pep:`The Zen of Python <20>`,
written as ``:pep:`The Zen of Python <20>```.

A section of a PEP
------------------

A ``#`` and an anchor link one section of the PEP, and — as in Sphinx — the
anchor stays in the text: :pep:`484#type-aliases`.

A number that is not one
------------------------

A target that is not a number is shown as written and reported as
``pep.invalid-number``; the comment below silences that here.

.. noqa: pep.invalid-number

:pep:`eight`

The PEP index
-------------

Links go to ``https://peps.python.org/`` unless the site's ``rinx.toml`` sets
``pep_base_url``, for example to a mirror.

docutils' ``:pep-reference:``
-----------------------------

docutils' own role, which Sphinx leaves in place beside ``:pep:``, links the
PEP and nothing more: :pep-reference:`8`, written as ``:pep-reference:`8```.
It makes no general index entry, shows no bold text, and its link,
``https://peps.python.org/pep-0008``, has no trailing slash.

Its target is the number alone, from 0 to 9999: an explicit title or a ``#``
anchor is not a number, so it is shown as written and reported as
``pep-reference.invalid-number``; the comment below silences that here.

.. noqa: pep-reference.invalid-number

:pep-reference:`8#naming`
