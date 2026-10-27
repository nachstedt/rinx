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
 * Every place going to the definition at `position` in `uri` leads, as
 * `file:line:start-end` — the file's name and the 0-based range opened.
 */
async function definitionsAt(uri: vscode.Uri, position: vscode.Position): Promise<string[]> {
    const definitions = await vscode.commands.executeCommand<(vscode.Location | vscode.LocationLink)[]>(
        'vscode.executeDefinitionProvider',
        uri,
        position,
    );
    return definitions.map((definition) => {
        const [file, range] =
            'targetUri' in definition
                ? [definition.targetUri, definition.targetSelectionRange ?? definition.targetRange]
                : [definition.uri, definition.range];
        return `${path.basename(file.fsPath)}:${range.start.line}:${range.start.character}-${range.end.character}`;
    });
}

suite('Go to definition (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('a :ref: leads to its label, and a :doc: to the document it names', async () => {
        // Given — `labelled.rst`, never opened, labels its section
        // `install-guide`; `navigating.rst` references it both ways.
        const navigating = workspaceFile('navigating.rst');
        await vscode.window.showTextDocument(navigating);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When — the cursor inside each role
        const byRef = await definitionsAt(navigating, new vscode.Position(0, 8));
        const byDoc = await definitionsAt(navigating, new vscode.Position(0, 32));

        // Then — the label's line, `.. _install-guide:`, and the file's start
        assert.deepStrictEqual(byRef, ['labelled.rst:0:0-18']);
        assert.deepStrictEqual(byDoc, ['labelled.rst:0:0-0']);
    });

    test('plain text leads nowhere', async () => {
        // Given
        const navigating = workspaceFile('navigating.rst');
        await vscode.window.showTextDocument(navigating);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When — the cursor on "before"
        const files = await definitionsAt(navigating, new vscode.Position(0, 48));

        // Then
        assert.deepStrictEqual(files, []);
    });
});
