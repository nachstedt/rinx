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

A role neither implemented nor ignored keeps the source it was written as:
:gh:`99`.
