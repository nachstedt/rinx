.. _vscode:

VS Code Extension
=================

The VS Code extension runs the ``rinx`` language server, which underlines problems as you type (see :ref:`vscode-diagnostics` below), and shows a live preview of the page the build writes for the open document (see :ref:`vscode-preview`).

1. Configuration
~~~~~~~~~~~~~~~~

The extension provides the following settings:

- ``rinx.binaryPath``: Path to the ``rinx`` binary the language server runs. When unset, the binary is discovered through Bazel, then taken from the extension itself if it was packaged with one (``npm run package:bundled``), falling back to ``rinx`` on the ``PATH``.
- ``rinx.previewMode``: ``onSave`` or ``onType`` (default: ``onType``).
- ``rinx.trace.server``: ``off``, ``messages`` or ``verbose``; traces the language server's protocol traffic into the *Rinx* output channel.
- ``rinx.inventories.download``: ``ask``, ``always`` or ``never``; whether the language server may download the inventories a Sphinx project links into (see :ref:`vscode-intersphinx`).
- ``rinx.conf.execute``: whether the language server may run a Sphinx project's ``conf.py`` (default: off; see :ref:`vscode-running-conf`).
- ``rinx.conf.pythonPath``: the Python interpreter ``conf.py`` runs in; when empty, the one selected with the Python extension.
- ``rinx.site``: the ``rinx_site`` the workspace is about, such as ``//docs:site``. A document several sites list belongs to it in the language server, and is previewed as part of it (see :ref:`vscode-bazel-projects`).
- ``rinx.autoDiscover``: whether the extension runs Bazel at all, to find the binary a Bazel workspace builds and to build the language server's manifests (default: on).

.. _vscode-preview:

2. The Preview
~~~~~~~~~~~~~~

*Rinx: Show Preview*, from the command palette or the preview button in the title bar of a reStructuredText editor, opens a panel beside the editor, which keeps the focus, showing the page of the reStructuredText document last active in an editor. The language server renders it, with a ``rinx/preview`` request, from what it already holds: the document as you type it, and the project index it keeps current as every document changes. So the page is never stale: a title changed in another file, even unsaved, shows in the sidebar and in every link to it at once, without a build. With ``rinx.previewMode`` set to ``onType`` the page is asked for again 150 ms after you stop typing in any reStructuredText document, with ``onSave`` when you save one. The new page replaces the old one in place, so the preview keeps its scroll position; switching to another document starts at its top.

The page is the one the build writes: the same renderer, the same template call, and the project's configuration as the server reads it (see :ref:`vscode-bazel-projects` and :ref:`vscode-sphinx-projects`). A ``rinx_site`` is drawn with its own ``template``, ``css`` and ``entity_templates``, read from the disk whenever the page is rendered, so editing one shows on the next page. Every other project, a Sphinx project's included, and a file outside every workspace folder, is drawn with the build's default template and stylesheet. A webview reads only the files of the workspace folders, so the page loads those from where they lie: a site's stylesheet, and every image a ``.. image::``, a ``.. figure::`` or a substitution shows, from the source tree rather than from the ``_images/`` a build copies it into. A ``:loading: embed`` image is linked the same way. A stylesheet outside every workspace folder, such as the default template's in a module depending on rinx, is embedded in the page instead.

Links in the page are followed as a reader expects, although they point into a built site the editor never writes. A link to a document opens its source in the editor column it was last shown in, and the preview follows it to the link's anchor; it opens at the file's first line, not at the label. A ``#`` link scrolls the page, an ``http(s):`` or ``mailto:`` link opens in the browser, and a ``:download:`` link opens the file in VS Code. The language server, which knows every document's name and file, says where a link leads with a ``rinx/previewLink`` request. A link to a page the build writes on its own, such as the general index, leads to no source, so following it only says so in the status bar.

Diagrams are drawn in the preview, with no Java: the language server puts each diagram's PlantUML text on the page, and the preview draws it with PlantUML's own engine compiled to JavaScript, which the extension brings. A ``.. plantuml::``, an ``.. entity-diagram::`` and a generated ``.. entity-flow::`` or ``.. entity-sequence::`` are drawn alike. The preview lays a diagram out with Graphviz, where the build uses ELK, so a picture may be arranged differently from the published one, though it shows the same. A PlantUML syntax error shows as PlantUML's picture of the error, naming the line, as the build's jar draws it. A diagram whose template cannot be expanded shows the build's warning in its place.

Rendering a page after an edit takes about 100 ms for CPython's ``library/os.rst`` and 155 ms for ``library/stdtypes.rst``, parse, index and render included. ``rinx lsp --check <folder> --preview <document>`` measures it.

What the preview does not show yet:

- An image outside every workspace folder, which the webview cannot reach, and an image changed on the disk until the page is loaded again.
- A diagram including PlantUML's standard library (``!include <C4/C4_Context>``), which the extension does not bring.
- The general index and the domain indices, which have no source to open.

.. _vscode-diagnostics:

3. Diagnostics
~~~~~~~~~~~~~~

Opening a ``.rst`` file starts the language server, ``rinx lsp``. It is the binary the build runs, so the editor reports the build's own diagnostics: the same codes and the same messages, underlined in the editor and listed in the *Problems* view as you type. For example, ``.. foo::`` is reported as ``directive.unknown``, and the code links to its entry in :ref:`diagnostics`. A ``.. noqa:`` comment silences a diagnostic in the editor exactly as it does in the build.

A mistake inside a file that an open document pulls in with ``.. include::`` is underlined in that file, at its own line, even when the file itself is not open. The ``.. include::`` line itself carries a note saying how many problems the file brought in, with a link to each, so the problem is visible from the document you are editing; it is information rather than a warning, so a problem is not counted twice. Editing an included file, saved or not, re-checks the open documents that include it. While an open document includes a file, that file shows what was found through its includers rather than what it reports on its own, because a fragment on its own lacks what its includer defines before the ``.. include::``.

That holds for documents that are not open, too. The server reads every ``.rst`` file under each workspace folder when it starts, so a fragment opened on its own is checked as the documents including it read it, whether or not they are open.

That scan also builds the project index the server keeps current as you edit. The status bar shows its progress, then for example ``rinx: 512 docs indexed (0.8 s)``; clicking it opens the server's log. The scan skips hidden directories and does not follow a symlinked directory, so a Bazel workspace's ``bazel-*`` output links are not read twice; ``.gitignore`` is not consulted, since generated sources are often part of the documentation; a project says which files are documents in its ``conf.py`` (see :ref:`vscode-sphinx-projects`). A workspace folder with no ``conf.py`` is one project, with ``index`` as its root document. ``docs/dev/lsp-roadmap.md`` in the repository lists the steps, and ``docs/decisions/038-language-server.md`` describes the design.

The index answers completion. Typing a role and the backtick opening its target lists every target that role can name, with the title a reference to it shows and the file it is in: labels after ``:ref:``, documents after ``:doc:``, glossary terms after ``:term:``, numbered labels after ``:numref:``, equations after ``:eq:``, entity ids after an entity role, a program's options after ``:option:``, the objects of the role's type after a domain role (``:py:func:``, ``:c:macro:``, and a bare ``:func:`` in the project's default domain), and all of them after ``:any:``. A bare backtick completes as the default role, once ``default_role`` or a ``.. default-role::`` above the cursor names one. Each target is listed only if the built page would link it from where you are typing, and written as briefly as it still leads there: inside ``.. py:class:: Box`` of ``.. py:module:: pkg``, a method of ``Box`` is offered as ``open`` rather than ``pkg.Box.open``, and listed first, being the nearest; an option of the ambient ``.. program::`` is offered without its program. Continue a longer spelling, and that is the one written. The targets of another site's inventory complete too, once it is loaded (see :ref:`vscode-intersphinx`), after the site's own; after ``:external+name:`` only that inventory's. Documents are named relative to the current document, as ``:doc:`` reads them; begin the name with ``/`` to list the names from the source root instead. Inside ``Title <target>`` only the target completes, once the ``<`` is written. A long list is cut to its nearest 200 and refreshed as you type. A target just written in an unsaved buffer completes at once, and while the scan is still running the list is refreshed as you type. The role must still be on one line.

Directives complete too, from the same table the build reads them by, so what is offered is what the build takes. After a line's ``..``, the list holds every directive with what it is for: the built-in ones, the domain objects by their bare name in the project's default domain (``primary_domain``) and by every qualified one (``py:function``, ``c:function``), and the entity types the project declares. Choosing one writes its ``::`` and, as a snippet, a tab stop for its argument and the first line of its content. A ``:`` on the lines directly below a directive's marker, before the first blank line, lists the options it takes, leaving out the ones already written, under any spelling; after ``:name:`` and a space, an option with a fixed set of values lists them, and a list leaves out the values it already holds. The list opens as the space is typed, and at once when the option itself was chosen from the list. An option the build does not implement is listed last and struck through, saying what to write instead. An entity type's options are its ``:id:``, its attributes and its relations. Below an ``.. entity-update::``, the options are the fields of the entities its argument names or matches, as the build resolves them; after ``:+`` or ``:-`` only the list fields. A relation's value completes the ids of the project's entities that it accepts, leaving out the ones already listed and the entity's own, each with its title, type and fields; a relation an ``.. entity-update::`` changes offers only the entities every matched type's relation accepts. A long list is cut to 200 and refreshed as you type. Entity ids after an entity role carry the same fields.

Role names complete the same way, from the build's own role table. A ``:`` where a role may start — after a space or an opening bracket, outside every literal and reference — lists every role with what it is for: the built-in ones, the domain roles by their bare name where the project's default domain has them (``:class:`` in a Python project, ``:macro:`` in a C one) and by every qualified one (``:py:class:``, ``:c:macro:``), the entity roles the project declares, and the roles the document has defined above the cursor with ``.. role::``. After ``:external:`` or ``:external+name:`` only the roles whose targets an inventory may hold are listed. Choosing one writes its ``:`` and the opening backtick and, as a snippet, the closing one too, with the cursor between them.

Hovering a reference shows where it leads: the title of what it names — a section's title, a document's title, an object's qualified name, the number a ``:numref:`` or ``:eq:`` shows, an entity's title — and the file it is in, as a link that opens it at the line the target is written on. A reference into another site's inventory shows that page's address instead. This covers ``:ref:``, ``:doc:``, ``:term:``, ``:numref:``, ``:eq:``, ``:any:``, ``:option:``, the domain roles and the entity roles, resolved exactly as the built page links them, in the scope the reference is written in. A reference to an entity, by an entity role or by ``:ref:``, shows the entity's fields below that: its attributes, its relations and the entities tracing back to it, under their labels and with every ``.. entity-update::`` applied, as its box on the page lists them; a value conflicting updates dispute is marked so. The same shows when hovering an id written in a relation option, and on an entity's own directive line or its ``:id:`` line, which is where to see what traces back to it. A reference the page would draw broken shows no hover, since its warning already says why. The hover reads the document as you type it, so it is current without waiting for a pause, and resolves into the inventories the server has loaded (see :ref:`vscode-intersphinx`).

Go to Definition (F12, or Ctrl+click) on the same references jumps to where their target is written: a label's ``.. _name:`` line, a glossary term's line, an object's or entity's directive, an equation, or the start of the document a ``:doc:`` names. A target an ``.. include::`` brings in opens in the included file. An entity id written in a relation option, an ``.. entity-update::``'s included, leads to that entity's directive too. It covers the same roles and resolves them the same way, so a reference the page would draw broken leads nowhere, and it reads every document as last typed, saved or not. An entry of a ``.. toctree::`` opens the document it names, as the page's navigation links it, and ``self`` the document itself; a ``:glob:`` pattern offers every document it matches, and hovering an entry shows their titles and files. An entry naming a document the project does not have, or an external link, leads nowhere. A reference into another site's inventory has no definition, since there is no source to open; its hover shows the address.

Find All References (Shift+F12) on a reference, a relation's id or a ``.. toctree::`` entry lists every place in the project that leads to the same target, in documents open or not and in the fragments an ``.. include::`` brings in: every ``:ref:``, ``:numref:`` and ``:any:`` naming a label, every role naming an object, term, equation or entity, and for a document every ``:doc:`` and toctree entry naming it. On a definition itself — a label's ``.. _name:`` line, a directive's ``:name:`` line (a figure's, a table's, a code block's or a toctree's), a ``.. math::``'s ``:label:``, a glossary term, an object's or entity's directive or its ``:id:`` — it lists the uses of that definition, such as every ``:numref:`` and ``:ref:`` naming a table. A label's uses include its page's named references — ``install_``, ```text <install_>`_``, and an alias target ``.. _docs: install_`` — and an entity's each id written in a relation option, an ``.. entity-update::``'s too, and the ``.. entity-update::`` naming it, and a ``:ref:`` to its label. Each reference is resolved as the page would link it, so one in another scope naming an object of the same name is not listed, and a reference the page would draw broken is no use of anything.

Rename Symbol (F2) on a label, an entity id or an equation's label — on a reference to it or where it is defined — renames it everywhere the project writes it: the name in its ``.. _name:`` line or its ``:name:``, ``:label:`` or ``:id:`` value, the target of every role naming it (an explicit title is kept), each id in a relation option, each named reference, and documents not open included. A named reference's text is its name, so ```Install`_`` becomes ```Setup`_``, and a bare ``install_`` gains backquotes when the new name needs them. A reference reaching the label through an alias keeps the alias. The rename is refused, saying why, rather than done in part: for a document (rename its file), a section, a glossary term or a domain object; for a name two documents define; for an entity id that was composed or generated rather than written as ``:id:``; for a name already defined, or one a label cannot hold (a backquote, an angle bracket, a colon); for a use in ``conf.py``'s ``rst_prolog`` or ``rst_epilog``; and when a file no longer reads as the server last analysed it, such as a file changed on disk unseen. Ids written inside a ``:filter:`` or a diagram template are not renamed.

The Outline view and the breadcrumbs above the editor show the document's sections, nested as their titles are, and under each one the objects and entities written in it: a domain object by the qualified name a reference uses (``pkg.Box.open``), with a class's methods under the class, and an entity by its id, with its type beside it. A section or object that an ``.. include::`` brings in is placed on the ``.. include::`` line. The outline reads the document as you type it.

Go to Symbol in Workspace (Ctrl+T) searches what every document of the workspace defines, open or not: sections by their titles, labels, glossary terms, equations, domain objects by their qualified names, and entities by their ids. Matching is fuzzy and ignores case, so ``bxopen`` finds ``pkg.Box.open``. Each symbol names the document defining it, which tells apart two sections both titled *Notes*, and opens where it was written, in the included file for what an ``.. include::`` brings in. A label two documents define is listed under both. The best 200 matches are listed, and the list narrows as you type.

Sections, directives, comments, literal blocks and tables fold. A section folds from its title to its last line before the next title at its level or above, and a directive over its whole indented block. The names the markup writes are coloured by kind: a directive's name, an option's name, a role's name, what a reference leads to (a role's target, a link's name or address, a ``.. _name:`` label), and a ``|substitution|``. Both read the document as you type it, exactly as the build reads it, so ``:ref:`` inside an inline literal is not coloured. Neither covers what an ``.. include::`` brings in; open the included file to see it folded and coloured. The extension turns semantic highlighting on for reStructuredText, and every theme colours the five kinds as it colours macros, properties, functions, strings and variables; a theme may also colour them itself by their names: ``directive``, ``option``, ``role``, ``reference`` and ``substitution``, with the ``declaration`` modifier on a label or substitution being defined.

The index also decides which references are broken. What the parser finds is reported as you type; what only rendering the page finds comes a moment later, once you pause for 300 ms: a ``:ref:``, ``:doc:`` or domain reference that resolves nowhere (``link.broken-ref``, ``link.broken-doc``, …), a label two documents define, an equation that does not convert, a listing whose filter matches nothing, a diagram asking for an entity no document declares or whose template is no valid Jinja (``uml.*``, ``entity-flow.*``, ``entity-sequence.*``). A template error is underlined on its own line of the template. No diagram is compiled for this, so PlantUML's own syntax errors are not underlined; the preview shows them. So do the problems only the whole project reveals, which the build reports while indexing: a toctree entry naming no document, or a document in no toctree; a label, term, object or entity id two documents define; an entity relation naming an id no document declares. A toctree entry and an entity relation are shown on the line that writes them. The others are about the project rather than a line, so they are shown on the document's first line, and a ``.. noqa:`` cannot silence one without a line, as in the build. These are the build's own warnings, with the same codes and messages, and a ``.. noqa:`` silences them as it does in the build. Only the documents you have open are rendered, along with any closed document that includes an open file. Defining a missing label in another document, even in an unsaved buffer, clears the warning without your touching the reference. The page is rendered with what the server reads of the project's configuration, and with the inventories it has loaded, so a reference the build resolves through intersphinx resolves here too (see :ref:`vscode-intersphinx`). Diagrams are not compiled.

.. _vscode-bazel-projects:

Bazel projects
^^^^^^^^^^^^^^

In a workspace folder holding ``MODULE.bazel`` or a ``WORKSPACE`` file, the extension finds every ``rinx_site``, builds each one's ``lsp_manifest`` output group (see :ref:`rules`) and hands the manifests to the language server. Each site is a project, and its documents are exactly the ``srcs`` of its libraries, each parsed with its own library's settings: its ``default_domain``, ``default_role``, ``entity_schema``, ``rst_prolog`` and ``rst_epilog``. A library setting ``jinja = True`` has its documents rendered with its ``jinja_context`` before they are parsed, as the build renders them, and each finding is shown on the line it was written on: in the document, or in the template an ``{% include %}`` read, which is shown there. A document's ``.. include::``, ``.. literalinclude::`` and ``.. csv-table:: :file:`` and ``{% include %}`` may read only the files its library declares in ``parse_data``, so an undeclared one is ``include.file-unreadable`` (``jinja.template-not-found`` for a template) in the editor as it is in the build, even while the file is open. The site's ``rinx.toml``, ``domain_indices`` and ``inventories`` decide how its pages render; an inventory is read from the file the build reads, and nothing is downloaded. A mistake in a library's ``rst_prolog`` is shown in that file.

What fails the build is an error in the editor, at the line to change: a toctree entry naming a document outside the library's ``deps`` (``toctree.undeclared-dependency``), a diagram in a library without ``diagrams = True``, and an image or a ``:download:`` no library of the site declares in ``images`` or ``downloads`` (``image.undeclared``, ``download.undeclared``). These are the build's own checks, and like the build no ``.. noqa:`` silences them: the fix is in ``BUILD.bazel``. For a toctree entry, the quick fix (Ctrl+.) makes it: it adds the library listing the document to the ``deps`` of the library writing the entry, in that library's ``BUILD.bazel``. The library is found among the site's, or else in the build file of the document's own package, whose ``rinx_library`` lists it in ``srcs`` — as a list, or a ``glob``. No fix is offered when ``deps`` or ``srcs`` is computed, or when the library is declared by a macro. The edit is made in the editor, so it can be undone; saving the file builds the manifests again, which clears the error. In a site setting ``strict_links``, a broken reference is an error too. Everything else is reported exactly as the build reports it, as a warning.

The status bar then names the site, for example ``rinx: rinx project //docs:site · 105 docs (0.3 s)``, or counts them when there are several. A document two sites list belongs to the one ``rinx.site`` names, else to the first by label; the other sites are read without it. What the manifests do not list — a ``README.rst``, a test fixture — stays the folder's, or a ``conf.py``'s. Editing a ``BUILD`` file, a ``.bzl`` file or ``MODULE.bazel`` builds the manifests again a moment later; until they arrive, the folder is read as it would be without Bazel. A manifest that cannot be read, and a setting in it the build would refuse — an invalid schema or ``rinx.toml``, a ``default_role`` no role answers to — is shown once in a notification, and the setting keeps its default. ``rinx lsp --check <folder> --manifest <manifest>`` (once per site) does the same from the command line.

.. _vscode-sphinx-projects:

Sphinx projects
^^^^^^^^^^^^^^^

A directory holding a ``conf.py`` is a Sphinx project, and every file belongs to the nearest project above it. What lies under no ``conf.py`` stays the workspace folder's own project. The status bar then names the project, for example ``rinx: Sphinx project (docs/conf.py) · 512 docs (0.8 s)``, and its tooltip lists every project.

By default the server reads ``conf.py`` without running it, since running it needs the project's Python and its extensions, and runs code you have not chosen to trust (see :ref:`vscode-running-conf` to let it). It takes each setting below where it is written as a literal: a string, a number, ``True``, ``False``, ``None``, or a list, tuple or dict of those. Every setting it cannot take as written is reported in ``conf.py`` itself:

- ``conf.unread-setting``: the value is computed or imported, so the setting keeps its default.
- ``conf.modified-setting``: the literal is changed afterwards, as by ``exclude_patterns.append(…)`` inside an ``if``. The literal applies; the change does not.
- ``conf.invalid-value``: the value is not one the setting takes, or not one rinx supports.
- ``conf.wildcard-import``: a ``from … import *``, which may set anything.
- ``conf.syntax-error``: where reading stopped; every setting after it keeps its default.

These are the settings read:

- ``root_doc`` (or ``master_doc``): the root document. As in Sphinx, ``contents`` stands in for a missing ``index``.
- ``source_suffix``: the suffixes of documents. Only reStructuredText's are read, so a Markdown suffix is reported, and a file under two suffixes is one document, the first by name.
- ``exclude_patterns`` and ``include_patterns``: which files are documents, matched as Sphinx matches them. ``templates_path``, ``html_static_path`` and ``html_extra_path`` are excluded too, as Sphinx excludes them. An excluded file you open is still checked, but it is in no index and is not rendered.
- ``default_role``: the role a bare `` `text` `` is read as.
- ``primary_domain``: ``py`` or ``c``. Any other domain, or ``None``, is reported, and ``py`` is used.
- ``numfig``, ``numfig_secnum_depth`` and ``highlight_language``: how pages are rendered.
- ``extensions``: how strictly the project's findings are reported (see below).
- ``intersphinx_mapping`` and ``intersphinx_cache_limit``: the other sites the project links into, when ``extensions`` declares ``sphinx.ext.intersphinx`` (see :ref:`vscode-intersphinx`).
- ``rst_prolog`` and ``rst_epilog``: text parsed at the start and the end of every document, typically substitutions. Read without running ``conf.py``, only a literal string is taken: an f-string such as CPython's ``rst_epilog`` is reported, and its substitutions stay undefined. A mistake in the text itself is shown at the start of each document it is parsed with.

- The sphinx-needs settings, when ``extensions`` declares ``sphinx_needs`` (see below).

Every other setting is ignored, and not reported. Nor is ``setup()`` run, so a project rendering its documents as Jinja templates from a ``source-read`` handler has them read as written. Saving ``conf.py``, or creating or deleting one, reads the folder's projects again.

A project declaring ``sphinx_needs`` gets its need types as directives: ``.. req::`` is recognised and checked, its id is a target, and ``:need:`` and each type's own role resolve, hover and go to the need, and complete the ids of the needs the project defines. The server converts the project's sphinx-needs configuration into an entity schema (see :ref:`entities`), the conversion ``rinx needs_schema`` writes an ``entities.toml`` with:

- The literal ``needs_*`` settings of ``conf.py``: ``needs_types``, ``needs_fields`` or ``needs_extra_options``, ``needs_links`` or ``needs_extra_links``, ``needs_statuses``, ``needs_id_required``, ``needs_import_keys``, and those sphinx-needs offers that the entity model cannot express, so they can be reported.
- The ``[needs]`` table of the ``ubproject.toml`` that ``needs_from_toml`` names, relative to ``conf.py``, under the table ``needs_from_toml_table`` names, if any. As in sphinx-needs, a setting written there overrides the same setting in ``conf.py``, and a path written there is relative to the TOML file.
- The ``schemas.json`` that ``needs_schema_definitions_from_json`` names: the rules selecting one need type, which make an option required or constrain a value.

What could not be read is ``conf.needs-invalid``, and each construct the entity model cannot express, such as a link whose targets a function computes or a conditional ``schemas.json`` rule, is ``conf.needs-unconverted``. Both are reported on the ``needs_from_toml`` line, or on ``extensions`` when there is none. Saving the ``ubproject.toml`` or the ``schemas.json`` reads the project again, as saving ``conf.py`` does.

A Sphinx project leans on extensions rinx does not run, such as autodoc. Reported as warnings, everything they explain would drown the findings you can fix, so the server lowers what a declared extension accounts for:

- An unknown directive that a declared extension provides, such as ``.. automodule::`` under ``sphinx.ext.autodoc``, is *Information*: its contents are not checked. Any other unknown directive stays a warning, since it may be a typo.
- A domain reference that resolves to nothing, such as ``:py:func:`spam.eggs```, is a *Hint* when a declared extension may define it: autodoc defines ``py`` objects. ``:ref:``, ``:doc:`` and every other reference stay warnings.

Which extensions rinx knows, and what each one adds, is a table in the repository, ``crates/lsp/src/project/extension_profiles.toml``; teaching it another extension needs no Rust. An extension missing from the table is assumed to define objects of every domain, so it lowers every broken domain reference to a hint, but it explains no directive: nothing says which are its. The server names those extensions once, in a notification, when it first finds the project. The code of a lowered diagnostic is unchanged, so a ``.. noqa:`` still names it, and the build still reports it as a warning.

.. _vscode-running-conf:

Running ``conf.py``
^^^^^^^^^^^^^^^^^^^

A project that computes its settings — a version imported from the package it documents, an epilog built with an f-string, exclusions added inside an ``if`` — gets their exact values once the server may run its ``conf.py``. Turn on ``rinx.conf.execute``: the server then runs each ``conf.py`` whose reading fell short, and only those, and only in a workspace you trust: in an untrusted one the extension names no interpreter, whatever the settings say. It runs in ``rinx.conf.pythonPath`` when set — a command on the ``PATH``, or a path relative to the workspace folder — or else in the interpreter the Python extension has selected for the file.

``conf.py`` runs as Sphinx runs it, in its own directory with ``__file__`` and an empty ``tags`` bound, and the server then takes every setting above from what it computed instead of from its text, checked as a literal would be. So none of the findings above is made, while a value of the wrong kind still is, on the line that assigns it. A value with no literal spelling, such as an object, is ``conf.unrepresentable-value``, and the setting keeps its default. If running fails — an exception, an import the interpreter cannot satisfy, more than 20 seconds — that is ``conf.execution-failed``, on the line it failed at, and ``conf.py`` is read as before. What ``conf.py`` prints is ignored.

The status bar then says ``Sphinx project (docs/conf.py, run)``, and its tooltip names the interpreter. Saving ``conf.py``, or a module of the project it imported, runs it again, and so does granting trust, changing either setting, or selecting another interpreter. ``rinx lsp --check <folder> --python <interpreter>`` does the same from the command line.

.. _vscode-intersphinx:

Inventories
^^^^^^^^^^^

A Sphinx project declaring ``sphinx.ext.intersphinx`` links into the sites its ``intersphinx_mapping`` names, so ``:py:class:`dict``` leads to Python's documentation. The editor resolves those references too: hovering one shows the address it leads to. It needs each site's ``objects.inv``, read from where the mapping says, as Sphinx reads it:

- An entry written ``'python': ('https://docs.python.org/3', None)`` reads ``objects.inv`` under the site's address. A second element naming an address or a file reads that instead, and a tuple of them is tried in order until one loads.
- A file is read relative to the directory holding ``conf.py``, and read again when it changes on the disk.
- An address is downloaded by the extension, never by the language server, which stays off the network. The first time a workspace needs one, the extension asks, naming the sites: *Allow* is remembered for the workspace, *Never* too, and *Not Now* asks again in the next session. The ``rinx.inventories.download`` setting decides without asking: ``always`` or ``never``.
- A download is kept in the extension's global storage, and used without the network for ``intersphinx_cache_limit`` days (5 unless the project sets it; a negative number keeps it for ever). An older copy is still used when downloading it again fails, as when you are offline.

The address-keyed form older Sphinx versions took, ``{'https://docs.python.org/3': None}``, is reported as ``conf.invalid-value``: Sphinx 8 removed it. An entry none of whose inventories could be read — a missing file, or one that is no inventory — is reported on its line in ``conf.py`` as ``conf.inventory-unreadable``.

Until an entry's inventory is loaded — not downloaded yet, declined, or unreadable — a domain reference that resolves nowhere may well be in it, so it is a *Hint* naming the inventory and why it is missing, as is an ``:external+python:`` reference to that inventory. Once every inventory is loaded, a reference still resolving nowhere is the warning the build reports. ``:ref:``, ``:doc:`` and ``:term:`` stay warnings either way. Editors other than VS Code download nothing, so there only file inventories load (see :ref:`editors`).

Development and Local Testing
-----------------------------

1. Repository Structure
~~~~~~~~~~~~~~~~~~~~~~~
The extension should be developed in a dedicated directory at the project root:
- ``editors/vscode/``: Contains the TypeScript source code, ``package.json``, and VS Code configuration.

2. Prerequisites
~~~~~~~~~~~~~~~~
- **Node.js & npm**: Required for building the extension.
- **VS Code**: The target editor.
- ``vsce``: The VS Code Extension Manager, installed as a development dependency by ``npm install``.

3. Local Development Workflow
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
To develop and debug the extension without publishing:
1.  **Open the directory**: Open ``editors/vscode`` in a new VS Code window.
2.  **Install dependencies**: Run ``npm install``.
3.  **Launch Extension Development Host**: Press ``F5`` (or go to *Run and Debug* -> *Launch Extension*). This opens a new VS Code window with the extension loaded.
4.  **Live Debugging**: You can set breakpoints in the TypeScript source and use the *Debug Console* to inspect the extension's behavior.

4. Checks and Tests
~~~~~~~~~~~~~~~~~~~
CI runs all of these on every pull request, from ``editors/vscode``:

- ``npm run typecheck``, ``npm run lint`` and ``npm run format:check``. The linter is oxlint, with type-aware rules: the extension is on TypeScript 7, which ESLint's TypeScript support cannot load.
- ``npm test`` runs the tests inside a real VS Code through ``@vscode/test-cli``, configured in ``.vscode-test.mjs``. ``VSCODE_VERSION=min`` picks the oldest VS Code that ``engines.vscode`` allows, and ``VSCODE_VERSION=insiders`` picks the next one. ``npm test -- --coverage`` also measures coverage, which ``node scripts/check-coverage.mjs <floor>`` checks.
- The tests come in four labels, and ``npm test -- --label <name>`` runs one. ``unit`` needs nothing else. ``e2e`` opens a copy of ``test-fixtures/workspace`` and talks to the real ``rinx lsp``: the binary named by ``RINX_BINARY``, or else ``target/debug/rinx``, so run ``cargo build`` first. ``e2e-no-server`` checks that the extension survives a binary that does not exist. ``e2e-vsix`` exists only when ``RINX_VSIX`` names a packaged extension: it runs the ``e2e`` tests against that VSIX installed into VS Code, rather than against the checkout. With ``RINX_VSIX_BUNDLED=1`` the VSIX is one with a binary inside, and no ``rinx.binaryPath`` is set, so the tests run against the binary it carries.

5. Packaging and Local Installation
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
To test the extension as a "production" build:
1.  **Package the extension**: Run ``npm run package`` inside ``editors/vscode``. This bundles the extension with esbuild into ``dist/`` and writes ``rinx.vsix``. ``npm run package:check`` compares the packaged files against ``vsix-files.txt``. Every CI run also uploads the VSIX as the ``rinx-vsix`` artifact.
2.  **Or package it with a binary**: Run ``npm run package:bundled``. It builds ``rinx`` in release mode (or takes ``-- --binary <path>``), packages it into the VSIX under ``bin/`` and writes ``rinx-<platform>.vsix``, which works on the packaging machine's platform only. Such a VSIX needs no further setup: the extension uses its own binary unless ``rinx.binaryPath`` is set or a Bazel workspace provides one.
3.  **Install locally**: Run ``code --install-extension rinx.vsix`` (or the ``rinx-<platform>.vsix``).
4.  **Configure**: For a VSIX without a binary, set ``rinx.binaryPath`` in your global VS Code settings to point to your locally built binary, such as ``target/debug/rinx`` after ``cargo build``. Reload the window after rebuilding it, so the language server restarts.

6. Bazel Integration (Optional)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
While standard VS Code development tools are recommended, the final packaging step can be integrated into Bazel using a ``genrule`` that invokes ``vsce package``, ensuring that the extension version matches the project version.
