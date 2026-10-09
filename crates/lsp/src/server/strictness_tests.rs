//! A Sphinx project's strictness: what the extensions its `conf.py` declares
//! lower, and the one notice naming those the server cannot model.

use super::test_support::*;
use lsp_types::notification::{Notification as _, ShowMessage};
use lsp_types::{DiagnosticSeverity, FileChangeType, ShowMessageParams};

/// Each diagnostic `replies` last publish for `file`, as its code and
/// severity.
fn severities_for(
    workspace: &Workspace,
    replies: &[Message],
    file: &str,
) -> Vec<(String, Option<DiagnosticSeverity>)> {
    let uri = workspace.uri(file);
    replies
        .iter()
        .filter(|reply| {
            matches!(reply, Message::Notification(n) if n.method == "textDocument/publishDiagnostics")
        })
        .map(published)
        .filter(|params| params.uri == uri)
        .last()
        .map(|params| {
            params
                .diagnostics
                .into_iter()
                .map(|diagnostic| {
                    let code = match diagnostic.code {
                        Some(lsp_types::NumberOrString::String(code)) => code,
                        _ => String::new(),
                    };
                    (code, diagnostic.severity)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The `window/showMessage` texts among `messages`, in order.
fn shown_messages(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::Notification(notification) if notification.method == ShowMessage::METHOD => {
                serde_json::from_value::<ShowMessageParams>(notification.params.clone())
                    .ok()
                    .map(|params| params.message)
            }
            _ => None,
        })
        .collect()
}

const AUTODOC_CONF: &str = "extensions = ['sphinx.ext.autodoc']\n";

#[test]
fn test_a_directive_of_a_declared_extension_is_information_and_a_typo_a_warning() {
    // Given
    let text = "Home\n====\n\n.. automodule:: spam\n\n.. automodul:: spam\n";
    let (workspace, mut state) = scanned(
        "strictness_directives",
        &[("conf.py", AUTODOC_CONF), ("index.rst", text)],
    );

    // When
    let replies = handle_notification(&mut state, workspace.open("index.rst", text));

    // Then
    assert_eq!(
        severities_for(&workspace, &replies, "index.rst"),
        [
            (
                "directive.unknown".to_string(),
                Some(DiagnosticSeverity::INFORMATION)
            ),
            (
                "directive.unknown".to_string(),
                Some(DiagnosticSeverity::WARNING)
            ),
        ]
    );
}

#[test]
fn test_a_reference_an_extension_may_define_is_a_hint_and_a_label_a_warning() {
    // Given — autodoc would define `spam.eggs`; no extension defines labels
    let text = "Home\n====\n\nSee :py:func:`spam.eggs` and :ref:`install`.\n";
    let (workspace, mut state) = scanned(
        "strictness_references",
        &[("conf.py", AUTODOC_CONF), ("index.rst", text)],
    );
    handle_notification(&mut state, workspace.open("index.rst", text));

    // When
    let rendered = render_all(&mut state);

    // Then
    assert_eq!(
        severities_for(&workspace, &rendered, "index.rst"),
        [
            (
                "link.broken-object".to_string(),
                Some(DiagnosticSeverity::HINT)
            ),
            (
                "link.broken-ref".to_string(),
                Some(DiagnosticSeverity::WARNING)
            ),
        ]
    );
}

#[test]
fn test_a_folder_with_no_conf_py_reports_everything_as_a_warning() {
    // Given
    let text = "Home\n====\n\n.. automodule:: spam\n";
    let (workspace, mut state) = scanned("strictness_folder", &[("index.rst", text)]);

    // When
    let replies = handle_notification(&mut state, workspace.open("index.rst", text));

    // Then
    assert_eq!(
        severities_for(&workspace, &replies, "index.rst"),
        [(
            "directive.unknown".to_string(),
            Some(DiagnosticSeverity::WARNING)
        )]
    );
}

#[test]
fn test_the_scan_names_the_unmodelled_extensions_once() {
    // Given
    let workspace = Workspace::new(
        "strictness_notice",
        &[
            (
                "conf.py",
                "extensions = ['sphinx.ext.autodoc', 'my_ext', 'other_ext']\n",
            ),
            ("index.rst", "Home\n====\n"),
        ],
    );
    let (mut state, _) = workspace.server(false);

    // When
    let finished = state.on_scan_event(workspace.scanned());

    // Then — autodoc is modelled, so only the other two are named
    let shown = shown_messages(&finished);
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(
        shown[0].contains("`my_ext`, `other_ext`") && shown[0].contains("conf.py"),
        "{}",
        shown[0]
    );
}

#[test]
fn test_a_project_declaring_only_modelled_extensions_shows_no_notice() {
    // Given
    let workspace = Workspace::new(
        "strictness_no_notice",
        &[("conf.py", AUTODOC_CONF), ("index.rst", "Home\n====\n")],
    );
    let (mut state, _) = workspace.server(false);

    // When
    let finished = state.on_scan_event(workspace.scanned());

    // Then
    assert_eq!(shown_messages(&finished), Vec::<String>::new());
}

#[test]
fn test_changing_conf_py_names_only_extensions_not_named_before() {
    // Given a project whose unmodelled extension was named by the scan
    let (workspace, mut state) = scanned(
        "strictness_notice_again",
        &[
            ("conf.py", "extensions = ['my_ext']\n"),
            ("index.rst", "Home\n====\n"),
        ],
    );

    // When — a change elsewhere in `conf.py` finds the project again
    workspace.write("conf.py", "extensions = ['my_ext']\nnumfig = True\n");
    let unchanged = handle_notification(
        &mut state,
        workspace.watched(&[("conf.py", FileChangeType::CHANGED)]),
    );
    // and then another extension is declared
    workspace.write("conf.py", "extensions = ['my_ext', 'new_ext']\n");
    let extended = handle_notification(
        &mut state,
        workspace.watched(&[("conf.py", FileChangeType::CHANGED)]),
    );

    // Then
    assert_eq!(shown_messages(&unchanged), Vec::<String>::new());
    let shown = shown_messages(&extended);
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(shown[0].contains("`my_ext`, `new_ext`"), "{}", shown[0]);
}
