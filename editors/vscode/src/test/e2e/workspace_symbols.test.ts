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

/** What Ctrl+T lists for `query`, as `name kind file:line`. */
async function symbolsFor(query: string): Promise<string[]> {
    const symbols = await vscode.commands.executeCommand<vscode.SymbolInformation[]>(
        'vscode.executeWorkspaceSymbolProvider',
        query,
    );
    return symbols.map(
        ({ name, kind, location }) =>
            `${name} ${vscode.SymbolKind[kind]} ${path.basename(location.uri.fsPath)}:${location.range.start.line}`,
    );
}

suite('Workspace symbols (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('Ctrl+T finds a label and its section in a document never opened', async () => {
        // Given — `labelled.rst` labels its section `install-guide`
        await vscode.window.showTextDocument(workspaceFile('navigating.rst'));
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When
        const found = await symbolsFor('install-guide');

        // Then — the label, on its own line
        assert.ok(found.includes('install-guide Key labelled.rst:0'), found.join('\n'));
    });

    test('Ctrl+T finds a method by its qualified name', async () => {
        // Given — `objects.rst` documents `pkg.Box.open`
        await vscode.window.showTextDocument(workspaceFile('navigating.rst'));
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When
        const found = await symbolsFor('Box.open');

        // Then
        assert.ok(found.includes('pkg.Box.open Method objects.rst:4'), found.join('\n'));
    });
});
