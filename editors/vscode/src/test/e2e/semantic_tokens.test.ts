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

/** Each token's line, start, length and type name, decoded from `tokens`. */
function decoded(
    tokens: vscode.SemanticTokens,
    legend: vscode.SemanticTokensLegend,
): [number, number, number, string][] {
    const shown: [number, number, number, string][] = [];
    let line = 0;
    let start = 0;
    for (let at = 0; at < tokens.data.length; at += 5) {
        const [deltaLine, deltaStart, length, type] = tokens.data.slice(at, at + 4);
        line += deltaLine;
        start = deltaLine === 0 ? start + deltaStart : deltaStart;
        shown.push([line, start, length, legend.tokenTypes[type]]);
    }
    return shown;
}

suite('Semantic tokens (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('a directive, a role and its target are coloured by kind', async () => {
        // Given — `colours.rst` holds a note citing a document with `:doc:`
        const colours = workspaceFile('colours.rst');
        await vscode.window.showTextDocument(colours);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When
        const legend = await vscode.commands.executeCommand<vscode.SemanticTokensLegend | undefined>(
            'vscode.provideDocumentSemanticTokensLegend',
            colours,
        );
        const tokens = await vscode.commands.executeCommand<vscode.SemanticTokens | undefined>(
            'vscode.provideDocumentSemanticTokens',
            colours,
        );

        // Then
        assert.ok(legend && tokens, 'the server colours the document');
        assert.deepStrictEqual(decoded(tokens, legend), [
            [3, 3, 6, 'directive'],
            [5, 7, 5, 'role'],
            [5, 13, 7, 'reference'],
        ]);
    });
});
