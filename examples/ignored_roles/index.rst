.. _ignored-roles-index:

##############
Ignored Roles
##############

A Sphinx project may use roles its own extensions provide, which rinx does not
implement. Its library lists them as ``ignored_roles`` in the
``rinx_parser.toml`` its ``parse_config`` names, and each is shown as its text,
as docutils shows a role it has no meaning for:

* Before the text: fixed in :issue:`4321`.
* After the text: reported in `1234`:issue:.
* With a domain: as :cite:p:`knuth1984` shows.

A role neither implemented nor ignored keeps the source it was written as,
and is reported as ``interpreted.unknown-role`` — silenced here:

.. noqa: interpreted.unknown-role

:gh:`99`.

A role Sphinx defines that rinx does not implement yet is reported as
``interpreted.unsupported-role`` instead, which the language server shows as
information in a Sphinx project:

.. noqa: interpreted.unsupported-role

Press :kbd:`Ctrl+C`.
