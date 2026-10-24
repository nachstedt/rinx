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

/** Each symbol's name and kind, nested as the outline nests them. */
interface Shown {
    name: string;
    kind: vscode.SymbolKind;
    children: Shown[];
}

function shown(symbols: vscode.DocumentSymbol[]): Shown[] {
    return symbols.map((symbol) => ({ name: symbol.name, kind: symbol.kind, children: shown(symbol.children) }));
}

suite('Outline (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('the outline nests a class method under its class', async () => {
        // Given — `objects.rst` documents module `pkg`, its class `Box` and
        // the method `open`
        const objects = workspaceFile('objects.rst');
        await vscode.window.showTextDocument(objects);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When
        const symbols = await vscode.commands.executeCommand<vscode.DocumentSymbol[] | undefined>(
            'vscode.executeDocumentSymbolProvider',
            objects,
        );

        // Then — named as the index names them
        assert.deepStrictEqual(shown(symbols ?? []), [
            { name: 'pkg', kind: vscode.SymbolKind.Module, children: [] },
            {
                name: 'pkg.Box',
                kind: vscode.SymbolKind.Class,
                children: [{ name: 'pkg.Box.open', kind: vscode.SymbolKind.Method, children: [] }],
            },
        ]);
    });
});
