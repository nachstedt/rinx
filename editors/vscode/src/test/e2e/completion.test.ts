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

/** An item's label as the editor shows it, whichever form the server sent. */
function labelOf(item: vscode.CompletionItem): string {
    return typeof item.label === 'string' ? item.label : item.label.label;
}

/** The completion items the editor offers at `position` in `uri`. */
async function completionsAt(uri: vscode.Uri, position: vscode.Position): Promise<vscode.CompletionItem[]> {
    const list = await vscode.commands.executeCommand<vscode.CompletionList>(
        'vscode.executeCompletionItemProvider',
        uri,
        position,
    );
    return list.items;
}

suite('Reference completion (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('a :ref: lists a label of another document with its title', async () => {
        // Given — `labelled.rst`, never opened, labels its section
        // `install-guide`; the cursor stands after ":ref:`".
        const referencing = workspaceFile('referencing.rst');
        await vscode.window.showTextDocument(referencing);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When
        const items = await completionsAt(referencing, new vscode.Position(0, 10));

        // Then
        const label = items.find((item) => labelOf(item) === 'install-guide');
        assert.ok(label, `install-guide among ${JSON.stringify(items.map(labelOf))}`);
        assert.strictEqual(label.detail, 'Installing');
    });

    test('a :doc: lists the documents with their titles', async () => {
        // Given — the cursor stands after ":doc:`".
        const referencing = workspaceFile('referencing.rst');
        await vscode.window.showTextDocument(referencing);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When
        const items = await completionsAt(referencing, new vscode.Position(2, 11));

        // Then
        const document = items.find((item) => labelOf(item) === 'labelled');
        assert.ok(document, `labelled among ${JSON.stringify(items.map(labelOf))}`);
        assert.strictEqual(document.detail, 'Installing');
    });
});

suite('Directive completion (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('a marker lists the directives with what each is for', async () => {
        // Given — the cursor stands after ".. no".
        const directives = workspaceFile('directives.rst');
        await vscode.window.showTextDocument(directives);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When
        const items = await completionsAt(directives, new vscode.Position(3, 5));

        // Then
        const note = items.find((item) => labelOf(item) === 'note');
        assert.ok(note, `note among ${JSON.stringify(items.map(labelOf))}`);
        assert.ok(note.insertText instanceof vscode.SnippetString, 'the name is written as a snippet');
    });

    test('an option line lists the options the directive takes', async () => {
        // Given — the cursor stands after ":max" below ".. toctree::".
        const directives = workspaceFile('directives.rst');
        await vscode.window.showTextDocument(directives);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When
        const items = await completionsAt(directives, new vscode.Position(1, 7));

        // Then
        const maxdepth = items.find((item) => labelOf(item) === 'maxdepth');
        assert.ok(maxdepth, `maxdepth among ${JSON.stringify(items.map(labelOf))}`);
        assert.strictEqual(maxdepth.detail, 'How many heading levels the tree shows; zero or less shows all.');
    });
});
