Updating entities after the fact
=================================

``.. entity-update::``, and its sphinx-needs spelling ``.. needextend::``,
mutate the fields of one or many already-declared entities — from any
document, once the whole project is merged. Applying one is never
destructive: it never overwrites what was written in ``requirements.rst``,
below — it builds a separate, traceable history beside it, which the
entity's own rendered box (and a custom template's ``history`` variable)
reads through. See ``docs/decisions/019-entity-update.md``.

Targeting a single id, appending and removing a tag
-----------------------------------------------------

.. entity-update:: REQ_001
   :+tags: safety-critical

   Flagged during the safety review of the boot sequence.

.. entity-update:: REQ_001
   :-tags: kernel

   The kernel team no longer owns this requirement's timing budget.

``REQ_001`` (declared in ``requirements.rst``, with ``:tags: boot, kernel``)
now shows ``boot``, ``safety-critical`` wherever its tags are read — its own
page there, this page's two directives above, and any ``.. entity-table::``
or ``.. entity-pie::`` that lists it.

Targeting a filter, setting a field on every match at once
-------------------------------------------------------------

.. spec:: Structured logging format
   :id: SPEC_002
   :status: draft

   .. rationale::

      Drafted alongside the boot sequence, but not reviewed yet.

.. entity-update:: type == "spec" and status == "draft"
   :status: approved

   Batch-approving every draft specification ahead of the release review —
   ``SPEC_001`` (already approved in ``requirements.rst``) is unaffected;
   only ``SPEC_002`` above matches.

Appending and removing a relation target
-------------------------------------------

.. impl:: Boot-timing self-check
   :id: IMPL_002
   :module: boot.selfcheck

.. entity-update:: SPEC_001
   :+implemented_by: IMPL_002

   Recording that the boot-timing self-check module also implements this
   specification, alongside ``IMPL_001`` already declared in
   ``requirements.rst``.

A target matching nothing
----------------------------

.. entity-update:: type == "req" and status == "archived"
   :strict: false

   No requirement is archived yet, so this quietly does nothing rather than
   warning about it.

.. entity-update:: type == "test" and automated == False

   Every test in this project is automated, so this one *does* warn
   (``entity-update.empty-result``) — ``:strict:`` defaults to ``true``,
   catching a filter that no longer matches anything it once did.

Rejected mutations
---------------------

Identity fields, and a declared section, cannot be changed this way:

.. entity-update:: REQ_001
   :id: REQ_999

   Reported as ``entity-update.protected-field`` and ignored.

.. entity-update:: REQ_001
   :verification-criteria: Replaced wholesale.

   Reported as ``entity-update.section-not-supported``: a section is a
   document, deliberately kept out of the project index this directive's
   effects live in.

.. entity-update:: REQ_001
   :priority: high

   ``req`` declares no such field at all, so this is
   ``entity-update.unknown-field``.

A conflicting pair
----------------------

.. entity-update:: REQ_001
   :status: closed

   Closed after the boot-timing regression passed.

.. entity-update:: REQ_001
   :status: in_progress

   Reopened pending the safety review above — which disagrees with the
   directive immediately before it about ``REQ_001``'s status. Both are
   reported under ``entity-update.conflicting-update``, each against its own
   directive, and the conflicting value is marked wherever it renders.
