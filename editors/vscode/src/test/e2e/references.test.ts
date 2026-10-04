import * as assert from 'assert';
import * as path from 'path';
import * as vscode from 'vscode';
import { waitFor } from './wait';

/** The file `name` at the root of the fixture workspace. */
function workspaceFile(name: string): vscode.Uri {
    const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
    return vscode.Uri.file(path.join(root, name));
}

/** A diagnostic's code as the text the server sent. */
function codeOf(diagnostic: vscode.Diagnostic): string | undefined {
    const code = diagnostic.code;
    return typeof code === 'object' ? String(code.value) : code === undefined ? undefined : String(code);
}

/** The codes of the diagnostics on `uri`. */
function codesOn(uri: vscode.Uri): (string | undefined)[] {
    return vscode.languages.getDiagnostics(uri).map(codeOf);
}

suite('Broken references (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('a reference to no label is underlined, and defining the label elsewhere clears it', async () => {
        // Given — `broken_reference.rst` refers to `no-such-label`, which no
        // document defines.
        const broken = workspaceFile('broken_reference.rst');
        await vscode.window.showTextDocument(broken);
        const [found] = await waitFor('the broken reference underlined', () => {
            const diagnostics = vscode.languages.getDiagnostics(broken);
            return diagnostics.length > 0 && diagnostics;
        });
        assert.strictEqual(codeOf(found), 'link.broken-ref');
        assert.strictEqual(found.message, "broken ref 'no-such-label'");

        // When — another document gains the label in its buffer, unsaved.
        const labelled = await vscode.workspace.openTextDocument(workspaceFile('labelled.rst'));
        await vscode.window.showTextDocument(labelled);
        const edit = new vscode.WorkspaceEdit();
        edit.insert(labelled.uri, new vscode.Position(0, 0), '.. _no-such-label:\n\n');
        await vscode.workspace.applyEdit(edit);

        // Then
        await waitFor('the broken reference cleared', () => codesOn(broken).length === 0);
    });
});
