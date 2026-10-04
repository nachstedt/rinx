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

/** The text of every hover the editor shows at `position` in `uri`. */
async function hoverTextAt(uri: vscode.Uri, position: vscode.Position): Promise<string[]> {
    const hovers = await vscode.commands.executeCommand<vscode.Hover[]>('vscode.executeHoverProvider', uri, position);
    return hovers.flatMap((hover) =>
        hover.contents.map((content) => (typeof content === 'string' ? content : content.value)),
    );
}

suite('Reference hover (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('hovering a :ref: shows the section title and the file it is in', async () => {
        // Given — `labelled.rst`, never opened, labels its section
        // `install-guide`, which `hovering.rst` references.
        const hovering = workspaceFile('hovering.rst');
        await vscode.window.showTextDocument(hovering);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When — the cursor inside the role
        const texts = await hoverTextAt(hovering, new vscode.Position(0, 8));

        // Then
        const text = texts.join('\n');
        assert.ok(text.includes('**Installing**'), text);
        assert.ok(text.includes('labelled\\.rst'), text);
    });

    test('hovering plain text shows nothing', async () => {
        // Given
        const hovering = workspaceFile('hovering.rst');
        await vscode.window.showTextDocument(hovering);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When — the cursor on "before"
        const texts = await hoverTextAt(hovering, new vscode.Position(0, 28));

        // Then
        assert.deepStrictEqual(texts, []);
    });
});
