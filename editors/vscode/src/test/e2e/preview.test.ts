import * as path from 'path';
import * as vscode from 'vscode';
import { waitFor } from './wait';

/** The open tab titled `label`, if any. */
function tabLabelled(label: string): vscode.Tab | undefined {
    return vscode.window.tabGroups.all.flatMap((group) => group.tabs).find((tab) => tab.label === label);
}

suite('Preview (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('the preview command opens a preview panel', async () => {
        // Given
        const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
        await vscode.window.showTextDocument(vscode.Uri.file(path.join(root, 'with_include.rst')));

        // When
        await vscode.commands.executeCommand('rinx.showPreview');

        // Then
        await waitFor('the preview tab', () => tabLabelled('Rinx Preview'));
    });
});
