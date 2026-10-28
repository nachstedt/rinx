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

/**
 * Every place Shift+F12 at `position` in `uri` lists, the definition
 * included, as `file:line:start-end` — the file's name and the 0-based range
 * — in the server's order, by file and then by place.
 */
async function referencesAt(uri: vscode.Uri, position: vscode.Position): Promise<string[]> {
    const references = await vscode.commands.executeCommand<vscode.Location[]>(
        'vscode.executeReferenceProvider',
        uri,
        position,
    );
    return references.map(
        ({ uri: file, range }) =>
            `${path.basename(file.fsPath)}:${range.start.line}:${range.start.character}-${range.end.character}`,
    );
}

suite('Find all references (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('a :ref: lists every use of its label, in documents never opened too', async () => {
        // Given — `labelled.rst` labels its section `install-guide`, and
        // `hovering.rst` and `navigating.rst` each refer to it.
        const navigating = workspaceFile('navigating.rst');
        await vscode.window.showTextDocument(navigating);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When — the cursor inside the role
        const found = await referencesAt(navigating, new vscode.Position(0, 8));

        // Then — both uses, and the label's line
        assert.deepStrictEqual(found, ['hovering.rst:0:4-24', 'labelled.rst:0:0-18', 'navigating.rst:0:4-24']);
    });

    test('the label itself lists the same uses', async () => {
        // Given
        const labelled = workspaceFile('labelled.rst');
        await vscode.window.showTextDocument(labelled);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When — the cursor on `.. _install-guide:`
        const found = await referencesAt(labelled, new vscode.Position(0, 6));

        // Then
        assert.deepStrictEqual(found, ['hovering.rst:0:4-24', 'labelled.rst:0:0-18', 'navigating.rst:0:4-24']);
    });
});
