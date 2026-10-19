;;; rinx-eglot.el --- rinx's language server for reStructuredText, through Eglot  -*- lexical-binding: t; -*-

;; Eglot is built into Emacs 29 and later.
(require 'eglot)
(add-to-list 'eglot-server-programs '(rst-mode . ("rinx" "lsp")))
(add-hook 'rst-mode-hook #'eglot-ensure)

;; Emacs 31.1 shows Flymake's diagnostics only for files listed in
;; `trusted-content', Eglot's included; later releases exempt Eglot, whose
;; diagnostics come from the server rather than from running the file.
(put 'eglot-flymake-backend 'flymake-always-safe t)
