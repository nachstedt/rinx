.. _button-link-diagnostics:

===========
Diagnostics
===========

Every case below is reported while building this site, and none of them costs
the button: an unreadable option is dropped and the button still renders. The
``.. noqa:`` comment above each one keeps the example site's build quiet while
still showing the construct — remove one and the warning appears.

A missing argument
==================

With no URL there is no button, so the directive is drawn as an error block
quoting its source — what an argument-less ``.. image::`` does too:

.. noqa: button-link.missing-target

.. button-link::

   There is nowhere for this to point.

An unreadable colour
====================

The eleven colour names are a closed set, so a twelfth is reported and the
button renders unpainted rather than carrying a class no stylesheet defines:

.. noqa: button-link.invalid-color

.. button-link:: https://example.com/bad-color
   :color: puce

   A colour nobody defined

An unreadable alignment
=======================

``:align:`` takes four names. docutils' *image* ``:align:`` also has ``top``,
``middle`` and ``bottom``, which this option does not:

.. noqa: button-link.invalid-align

.. button-link:: https://example.com/bad-align
   :align: middle

   An alignment this option does not have

An outline with no colour
=========================

sphinx-design adds a colour class only when ``:color:`` was written, so
``:outline:`` alone draws nothing at all. It is reported rather than left
silently invisible:

.. noqa: button-link.unusable-outline

.. button-link:: https://example.com/bare-outline
   :outline:

   An outline with no colour to draw it in

An option that belongs on the other button
==========================================

sphinx-design accepts ``:ref-type:`` here only because both of its button
directives share one option spec: it selects how a ``.. button-ref::``
resolves its argument, and this directive's argument is a URL. It is refused
by name rather than accepted and ignored:

.. noqa: button-link.unsupported-option

.. button-link:: https://example.com/ref-type
   :ref-type: myst

   An option for the directive this build does not have

A reference inside the label
============================

A button is itself a link, so a reference role in its label would nest one
link inside another. The reference is kept and shown as its text alone:

.. noqa: button-link.nested-reference

.. button-link:: https://example.com/nested

   See :ref:`the basics page <button-link-basics>` for the plain forms

An option with no value
=======================

``:tooltip:`` is the one option here that requires a value, so writing it bare
is reported and no tooltip is set:

.. noqa: button-link.empty-option-value

.. button-link:: https://example.com/empty-tooltip
   :tooltip:

   A tooltip with nothing to show

An unknown option
=================

A misspelled option is reported and dropped, leaving the button intact:

.. noqa: directive.button-link-unknown-option

.. button-link:: https://example.com/unknown-option
   :colour: primary

   A misspelled option name
