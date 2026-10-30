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
 * Every edit renaming at `position` in `uri` to `newName` would make, as
 * `file:line:start-end=text`, in the order the edit lists them — never
 * applied, so the fixture stays as it is.
 */
async function renameEdits(uri: vscode.Uri, position: vscode.Position, newName: string): Promise<string[]> {
    const edit = await vscode.commands.executeCommand<vscode.WorkspaceEdit>(
        'vscode.executeDocumentRenameProvider',
        uri,
        position,
        newName,
    );
    return edit
        .entries()
        .flatMap(([file, edits]) =>
            edits.map(
                ({ range, newText }) =>
                    `${path.basename(file.fsPath)}:${range.start.line}:${range.start.character}-${range.end.character}=${newText}`,
            ),
        );
}

suite('Rename (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('F2 on a :ref: renames its label and every use, in documents never opened too', async () => {
        // Given — `labelled.rst` labels its section `install-guide`, and
        // `hovering.rst` and `navigating.rst` each refer to it.
        const navigating = workspaceFile('navigating.rst');
        await vscode.window.showTextDocument(navigating);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When — the cursor inside the role
        const edits = await renameEdits(navigating, new vscode.Position(0, 8), 'setup');

        // Then — the label's name and each role's target, each once and
        // nothing else, in whichever order the files are listed
        assert.strictEqual(edits.length, 3);
        assert.deepStrictEqual(
            new Set(edits),
            new Set(['hovering.rst:0:10-23=setup', 'labelled.rst:0:4-17=setup', 'navigating.rst:0:10-23=setup']),
        );
    });

    test('a refused rename says why', async () => {
        // Given
        const navigating = workspaceFile('navigating.rst');
        await vscode.window.showTextDocument(navigating);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When — on the `:doc:` role, which names a document
        const refused = await vscode.commands
            .executeCommand('vscode.prepareRename', navigating, new vscode.Position(0, 33))
            .then(
                () => '',
                (error: unknown) => String(error),
            );

        // Then
        assert.match(refused, /a document is renamed by renaming its file/);
    });
});
