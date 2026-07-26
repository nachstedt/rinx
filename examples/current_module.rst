Current Module
==============

Real Sphinx's ``.. currentmodule::`` directive (equivalently,
``.. py:currentmodule::`` with an explicit domain prefix) sets the current
``py``-domain module for the rest of the document, exactly like
``.. py:module::`` does for scoping unqualified cross-references — but,
unlike ``.. py:module::``, it documents nothing itself: it creates no index
entry and renders no HTML of its own. This is the shape real-world narrative
documentation (tutorials, howto guides) commonly uses: a chapter discussing a
module that is defined and documented on a *different* page, switching
context with ``currentmodule`` before referencing that module's members
unqualified.

This page defines no domain objects of its own — every object referenced
below is defined on the Domains page (``greetings.shout`` and
``clock.clock.now``), proving ``currentmodule`` scopes references across
files exactly as ``py:module`` does within one.

.. currentmodule:: greetings

With ``greetings`` set as the current module, :py:func:`shout` — written
bare, with no ``greetings.`` prefix, and with no ``.. py:module::`` anywhere
on this page — still resolves to ``greetings.shout``, exactly as if this
page had opened with ``.. py:module:: greetings`` itself.

.. currentmodule:: clock

Switching the current module again reaches a different page's objects the
same way: :py:meth:`clock.now` resolves to ``clock.clock.now``. The module
switch to ``clock`` supplies the module tier of the search, and the ``clock``
class prefix written in the target itself supplies the rest — together
reaching the same classmethod the Domains page cross-references as
``clock.clock.now`` after nesting under both ``py:module:: clock`` and
``py:class:: clock`` there.

.. currentmodule:: None

``.. currentmodule:: None`` is real Sphinx's reset form: it clears the
current module entirely, as if no ``currentmodule``/``module`` directive had
ever appeared on this page. A bare reference written after the reset would no
longer resolve — this page is built with ``strict_links``, so that failure
case isn't demonstrated directly, but :py:func:`greetings.shout` still
resolves here because it is written fully qualified, independent of module
scope.
