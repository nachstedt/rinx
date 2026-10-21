import * as assert from 'assert';
import * as path from 'path';
import * as vscode from 'vscode';
import type { RinxApi } from '../../extension';
import { waitFor } from './wait';

/** The file `name` at the root of the fixture workspace. */
function workspaceFile(name: string): vscode.Uri {
    const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
    return vscode.Uri.file(path.join(root, name));
}

/** The extension's API, once it is active. */
async function rinx(): Promise<RinxApi> {
    const extension = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx');
    assert.ok(extension, 'the extension is installed');
    return extension.isActive ? extension.exports : await extension.activate();
}

/** The open tab titled `label`, if any. */
function tabLabelled(label: string): vscode.Tab | undefined {
    return vscode.window.tabGroups.all.flatMap((group) => group.tabs).find((tab) => tab.label === label);
}

suite('Preview (e2e)', () => {
    teardown(async () => {
        const labelled = vscode.workspace.textDocuments.find((document) =>
            document.uri.fsPath.endsWith('labelled.rst'),
        );
        if (labelled?.isDirty) {
            await vscode.window.showTextDocument(labelled);
            await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
        }
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('the preview command opens a preview panel', async () => {
        // Given
        await vscode.window.showTextDocument(workspaceFile('with_include.rst'));

        // When
        await vscode.commands.executeCommand('rinx.showPreview');

        // Then
        await waitFor('the preview tab', () => tabLabelled('Rinx Preview'));
    });

    test('the preview shows an image of a Sphinx project where it lies', async () => {
        // Given
        const api = await rinx();
        await vscode.window.showTextDocument(workspaceFile('sphinx/pictured.rst'));

        // When
        await vscode.commands.executeCommand('rinx.showPreview');

        // Then — linked under the address the webview serves files at, not
        // into the `_images/` a build would copy it into
        const page = await waitFor('the page with its image', () => {
            const html = api.previewHtml();
            return html?.includes('Pictured') && html;
        });
        const src = /<img src="([^"]*)"/.exec(page)?.[1] ?? '';
        assert.ok(src.startsWith('https://'), page);
        assert.ok(src.endsWith(workspaceFile('sphinx/logo.svg').path), page);
    });

    test('the preview shows a title changed a moment ago in another document', async () => {
        // Given — `navigating.rst` links to `labelled.rst`'s section by
        // label, so the link shows that section's title
        const api = await rinx();
        await vscode.window.showTextDocument(workspaceFile('navigating.rst'));
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');
        await vscode.commands.executeCommand('rinx.showPreview');
        await waitFor('the page with the old title', () => api.previewHtml()?.includes('Installing'));
        // The focus into the preview and back to the editor, as when glancing
        // at the page and typing on, the panel visible throughout
        await vscode.commands.executeCommand('workbench.action.focusSecondEditorGroup');
        await vscode.commands.executeCommand('workbench.action.focusFirstEditorGroup');

        // When — the title changes in `labelled.rst`'s buffer, unsaved
        const labelled = await vscode.workspace.openTextDocument(workspaceFile('labelled.rst'));
        const edit = new vscode.WorkspaceEdit();
        edit.replace(labelled.uri, new vscode.Range(2, 0, 2, 10), 'Setting up');
        await vscode.workspace.applyEdit(edit);

        // Then
        const page = await waitFor('the page with the new title', () => {
            const html = api.previewHtml();
            return html?.includes('Setting up') && html;
        });
        assert.ok(page.includes('before you start.'), 'still the page of navigating.rst');
    });
});
