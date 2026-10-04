import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import type { RinxApi } from '../../extension';
import { waitFor } from './wait';

/** The root of the fixture workspace. */
function workspaceRoot(): string {
    return vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
}

/** The extension's API, once it is active. */
async function rinx(): Promise<RinxApi> {
    const extension = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx');
    assert.ok(extension, 'the extension is installed');
    return extension.isActive ? extension.exports : await extension.activate();
}

/** A diagnostic's code as the text the server sent. */
function codeOf(diagnostic: vscode.Diagnostic): string | undefined {
    const code = diagnostic.code;
    return typeof code === 'object' ? String(code.value) : code === undefined ? undefined : String(code);
}

suite('Workspace index (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('the status bar reports every document of the workspace indexed', async () => {
        // Given — the server starts with the first reStructuredText document.
        await vscode.window.showTextDocument(vscode.Uri.file(path.join(workspaceRoot(), 'fragment.rst')));
        const api = await rinx();

        // When
        const status = await waitFor('the index to be ready', () => {
            const reported = api.indexStatus();
            return reported?.state === 'ready' && reported;
        });

        // Then
        const documents = fs.readdirSync(workspaceRoot()).filter((name) => name.endsWith('.rst'));
        assert.strictEqual(status.documents, documents.length);
    });

    test('a fragment opened alone is diagnosed as the document including it reads it', async () => {
        // Given — `context_includer.rst`, never opened, defines the
        // substitution the fragment uses before including it.
        const fragment = vscode.Uri.file(path.join(workspaceRoot(), 'context_fragment.rst'));

        // When
        await vscode.window.showTextDocument(fragment);

        // Then — only the unknown directive: on its own the fragment would
        // also report its substitution as undefined.
        await waitFor('the fragment diagnosed in its includer', () => {
            const codes = vscode.languages.getDiagnostics(fragment).map(codeOf);
            return codes.length === 1 && codes[0] === 'directive.unknown';
        });
    });
});
