.. _editors:

Other editors
=============

The language server is the ``rinx`` binary itself, run as ``rinx lsp``. It speaks the Language Server Protocol over standard input and output, so any editor with a language client can use it. The :ref:`VS Code extension <vscode>` is one such client. This page configures three others: Neovim, Helix and Emacs.

Each configuration below is a file in the repository, under ``editors/``, and continuous integration runs every one in its editor against the server. Neovim and Emacs are checked for diagnostics, hover, go to definition and completion. Helix, which cannot be scripted, is checked for finding the server and showing its diagnostics.

Installing the server
---------------------

Each configuration starts ``rinx`` from the ``PATH``. Install it with Cargo:

.. code-block:: console

   $ cargo install --git https://github.com/nachstedt/rinx rinx

In a Bazel workspace depending on rinx, ``bazel build @rinx//:rinx`` builds the version the workspace's documentation build uses. Put that binary on the ``PATH``, or name its path in place of ``rinx`` below.

Neovim
------

Neovim 0.11 or later, with no plugin. Put this in ``init.lua``, or in a file of its own under ``~/.config/nvim/plugin/``:

.. literalinclude:: ../editors/neovim/rinx.lua
   :language: lua

The server's root is the nearest directory above the file holding ``conf.py`` or ``MODULE.bazel``, or else the Git repository. A file outside every one is checked on its own, with no project around it.

On Linux, Neovim does not watch files for a language server unless asked, so renaming or deleting a file in another program goes unnoticed until the document is opened again. To have it watch, add ``capabilities = { workspace = { didChangeWatchedFiles = { dynamicRegistration = true } } }`` to the table above.

Helix
-----

Add this to ``~/.config/helix/languages.toml``, or to ``.helix/languages.toml`` in the project:

.. literalinclude:: ../editors/helix/languages.toml
   :language: toml

``hx --health rst`` then lists ``rinx`` among the configured language servers. The server's root is the nearest directory above the file holding ``conf.py`` or ``MODULE.bazel``, or else the workspace Helix opened.

Emacs
-----

Emacs 29 or later, through Eglot, which is built in. Put this in ``init.el``:

.. literalinclude:: ../editors/emacs/rinx-eglot.el
   :language: lisp
   :start-at: ;; Eglot

The last line matters only on Emacs 31.1. That release shows Flymake's diagnostics only in files listed in ``trusted-content``, Eglot's among them, so without it a file shows hover and completion but no diagnostics. Later releases exempt Eglot themselves.

The server's root is the project Emacs finds for the file, usually its version-control repository. Within it, each document still belongs to the nearest ``conf.py`` (see :ref:`vscode-sphinx-projects`). lsp-mode works as well, as a client registered for ``rst-mode`` running ``rinx lsp``, but this configuration is not tested.

What every editor gets
----------------------

The server behaves the same in every editor. :ref:`vscode-diagnostics` describes what it does, and none of it depends on VS Code:

- Diagnostics as you type, with the build's codes and messages, linking to :ref:`diagnostics`. This includes the problems an ``.. include::`` brings in, and broken references once you pause.
- Completion of ``:ref:`` and ``:doc:`` targets, and of entity ids.
- Hover and go to definition on references.
- Reading a Sphinx project's ``conf.py``, and the extensions it declares (see :ref:`vscode-sphinx-projects`).
- Inventories that ``intersphinx_mapping`` names as files (see :ref:`vscode-intersphinx`).
- Watching the workspace for files changed by other programs, when the editor announces that it watches files for a server.

What only VS Code has
---------------------

Some features need the editor to do something for the server, and only the VS Code extension does so far:

- The live preview.
- Downloading inventories. The server never touches the network, so in other editors an inventory named by an address is not loaded, and references into it are hints rather than warnings.
- Running ``conf.py``. The server needs to be told which interpreter to run it in, and only in a trusted workspace (see :ref:`vscode-running-conf`). Elsewhere, ``conf.py`` is only read.
- Reading a Bazel workspace's sites as the build does. The server never runs Bazel, so it needs to be handed each site's manifest (see :ref:`vscode-bazel-projects`). Elsewhere, a Bazel workspace is read as a folder, so neither what fails the build nor the quick fix adding a missing ``deps`` entry is shown.
- The status bar, which shows the scan's progress and the project. Other editors show the protocol's own progress reports, where they support them.

Protocol extensions
^^^^^^^^^^^^^^^^^^^

A client can offer what VS Code offers through these messages. Each one is opt-in, so a client that knows none of them gets a working server:

- ``rinx/status``, a notification from the server, with the index's state (``indexing`` or ``ready``), the number of documents indexed, and the projects found. A client may ignore it.
- The ``fetchInventories`` initialization option. With it, the server sends ``rinx/fetchInventory`` requests naming an address, and the client answers with the downloaded inventory.
- The ``evaluateConf`` initialization option. With it, the server sends a ``rinx/confInterpreter`` request for each ``conf.py`` it could not read, and the client answers with an interpreter or none. The client sends a ``rinx/confInterpreterChanged`` notification when its answer would change.
- The ``bazelManifests`` initialization option. With it, the server sends a ``rinx/bazelManifests`` request for each workspace folder holding ``MODULE.bazel`` or a ``WORKSPACE`` file, and the client answers with the paths of the ``<site>.lsp_manifest.json`` files that ``bazel build --output_groups=lsp_manifest`` wrote, the workspace and execution roots, and optionally the preferred site — or none. The client sends a ``rinx/bazelManifestsChanged`` notification when its answer would change.

The exact shapes are in ``crates/lsp/src/server/`` in the repository.
