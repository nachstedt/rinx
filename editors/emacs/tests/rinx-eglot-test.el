;;; rinx-eglot-test.el --- `rinx lsp' through Eglot  -*- lexical-binding: t; -*-

;; Drives `rinx lsp' through Eglot, configured by the rinx-eglot.el
;; docs/editors.rst shows, over a copy of editors/test-fixtures/workspace.
;;
;; Run by editors/run-tests.sh, which loads rinx-eglot.el first and names the
;; copy in RINX_TEST_WORKSPACE.

;;; Code:

(require 'ert)
(require 'eglot)
(require 'flymake)

(defconst rinx-test-timeout 20
  "Seconds any one answer may take before a test fails rather than hangs.")

(defun rinx-test--wait (what predicate)
  "Return PREDICATE's first non-nil value, failing after `rinx-test-timeout'.
WHAT names what is waited for in the failure."
  (let ((deadline (+ (float-time) rinx-test-timeout))
        value)
    (while (and (not (setq value (funcall predicate)))
                (< (float-time) deadline))
      (accept-process-output nil 0.1))
    (or value (ert-fail (format "timed out waiting for %s" what)))))

(defun rinx-test--index ()
  "The fixture's index.rst, visited, with Eglot connected to rinx."
  (let ((buffer (find-file-noselect
                 (expand-file-name "index.rst" (getenv "RINX_TEST_WORKSPACE")))))
    (with-current-buffer buffer
      ;; `eglot-ensure' connects after the next command, and a batch session
      ;; runs none: run the hook it waits on.
      (run-hooks 'post-command-hook)
      (rinx-test--wait "Eglot to connect" #'eglot-current-server))
    buffer))

(defun rinx-test--at (line character)
  "The position of CHARACTER on LINE of the current buffer, as the protocol has it."
  `(:textDocument (:uri ,(concat "file://" (expand-file-name buffer-file-name)))
    :position (:line ,line :character ,character)))

(defun rinx-test--request (method params)
  "The server's answer to METHOD with PARAMS, asked again until it is not empty.
The workspace scan may still be running when it is first asked."
  (rinx-test--wait
   (format "an answer to %s" method)
   (lambda ()
     (let ((result (jsonrpc-request (eglot-current-server) method params)))
       (and result (not (seq-empty-p result)) result)))))

(defmacro rinx-test--with-index (&rest body)
  "Run BODY in the fixture's index.rst, printing Eglot's events if it fails."
  (declare (indent 0))
  `(with-current-buffer (rinx-test--index)
     (condition-case err
         (progn ,@body)
       ((debug error ert-test-failed)
        (message "--- Eglot events\n%s"
                 (with-current-buffer (jsonrpc-events-buffer (eglot-current-server))
                   (buffer-string)))
        (signal (car err) (cdr err))))))

(ert-deftest rinx-eglot-underlines-the-unknown-directive ()
  (rinx-test--with-index
    ;; Flymake starts its backends on an idle timer, which a batch session
    ;; never fires: start them while waiting.
    (let* ((diagnostics (rinx-test--wait "diagnostics"
                                         (lambda () (flymake-start) (flymake-diagnostics))))
           (diagnostic (car diagnostics))
           (lsp (alist-get 'eglot-lsp-diag (flymake-diagnostic-data diagnostic))))
      (should (= (length diagnostics) 1))
      (should (equal (plist-get lsp :code) "directive.unknown"))
      (should (= (line-number-at-pos (flymake-diagnostic-beg diagnostic)) 10)))))

(ert-deftest rinx-eglot-hovers-the-title-a-reference-leads-to ()
  (rinx-test--with-index
    (let ((hover (rinx-test--request :textDocument/hover (rinx-test--at 7 4))))
      (should (string-search "**Installing**"
                             (plist-get (plist-get hover :contents) :value))))))

(ert-deftest rinx-eglot-opens-the-label-a-reference-leads-to ()
  (rinx-test--with-index
    (let* ((definition (rinx-test--request :textDocument/definition (rinx-test--at 7 4)))
           (location (if (vectorp definition) (aref definition 0) definition))
           (range (or (plist-get location :targetSelectionRange)
                      (plist-get location :range))))
      (should (string-suffix-p "/target.rst"
                               (or (plist-get location :targetUri)
                                   (plist-get location :uri))))
      ;; The label, `.. _install:`, not the start of the file.
      (should (equal (plist-get (plist-get range :end) :character) 12)))))

(ert-deftest rinx-eglot-completes-the-label ()
  (rinx-test--with-index
    (let* ((completion (rinx-test--request :textDocument/completion (rinx-test--at 7 10)))
           (items (if (vectorp completion) completion (plist-get completion :items))))
      (should (seq-some (lambda (item) (equal (plist-get item :label) "install"))
                        items)))))

(ert-deftest rinx-eglot-completes-the-directive-name ()
  (rinx-test--with-index
    (let* ((completion (rinx-test--request :textDocument/completion (rinx-test--at 9 4)))
           (items (if (vectorp completion) completion (plist-get completion :items))))
      (should (seq-some (lambda (item) (equal (plist-get item :label) "figure"))
                        items)))))

(ert-deftest rinx-eglot-completes-the-role-name ()
  (rinx-test--with-index
    (let* ((completion (rinx-test--request :textDocument/completion (rinx-test--at 7 5)))
           (items (if (vectorp completion) completion (plist-get completion :items))))
      (should (seq-some (lambda (item) (equal (plist-get item :label) "ref"))
                        items)))))

(ert-deftest rinx-eglot-completes-the-domain-role-target ()
  (rinx-test--with-index
    (let* ((completion (rinx-test--request :textDocument/completion (rinx-test--at 13 14)))
           (items (if (vectorp completion) completion (plist-get completion :items))))
      (should (seq-some (lambda (item) (equal (plist-get item :label) "run"))
                        items)))))

;; The outline lists the title, and under it the function the file defines:
;; nested when the client takes it so, else naming the title as its holder.
(ert-deftest rinx-eglot-outlines-the-document ()
  (rinx-test--with-index
    (let* ((outline (rinx-test--request
                     :textDocument/documentSymbol
                     `(:textDocument (:uri ,(concat "file://" (expand-file-name buffer-file-name))))))
           (title (aref outline 0)))
      (should (equal (plist-get title :name) "Editor fixture"))
      (should (or (seq-some (lambda (symbol) (equal (plist-get symbol :name) "run"))
                            (plist-get title :children))
                  (seq-some (lambda (symbol)
                              (and (equal (plist-get symbol :name) "run")
                                   (equal (plist-get symbol :containerName) "Editor fixture")))
                            outline))))))

;;; rinx-eglot-test.el ends here
