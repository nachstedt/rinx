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

suite('Folding (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('a section and a directive fold', async () => {
        // Given — `colours.rst` holds a section and a note in it
        const colours = workspaceFile('colours.rst');
        await vscode.window.showTextDocument(colours);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When
        const ranges = await vscode.commands.executeCommand<vscode.FoldingRange[] | undefined>(
            'vscode.executeFoldingRangeProvider',
            colours,
        );

        // Then — the section from its title, the note from its marker
        const folded = (ranges ?? []).map((range) => [range.start, range.end]);
        assert.deepStrictEqual(folded, [
            [0, 5],
            [3, 5],
        ]);
    });
});
