import * as assert from 'assert';
import * as path from 'path';
import * as vscode from 'vscode';
import type { RinxApi } from '../../extension';
import { statusText } from '../../status';
import { waitFor } from './wait';

/** The fixture workspace's Sphinx project, `sphinx/`. */
function project(file: string): vscode.Uri {
    const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
    return vscode.Uri.file(path.join(root, 'sphinx', file));
}

/** The extension's API, once it is active. */
async function rinx(): Promise<RinxApi> {
    const extension = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx');
    assert.ok(extension, 'the extension is installed');
    return extension.isActive ? extension.exports : await extension.activate();
}

/** The codes of what the editor shows for `uri`. */
function codesFor(uri: vscode.Uri): string[] {
    return vscode.languages.getDiagnostics(uri).map((diagnostic) => {
        const code = diagnostic.code;
        return typeof code === 'object' ? String(code.value) : String(code);
    });
}

suite('Sphinx project (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('the status bar names the Sphinx project by its conf.py', async () => {
        // Given
        await vscode.window.showTextDocument(project('index.rst'));
        const api = await rinx();

        // When
        const status = await waitFor('the index to be ready', () => {
            const reported = api.indexStatus();
            return reported?.state === 'ready' && reported;
        });

        // Then
        assert.ok(statusText(status).startsWith('rinx: Sphinx project (sphinx/conf.py) · '), statusText(status));
    });

    test('a label only an excluded file defines is a broken reference', async () => {
        // When
        await vscode.window.showTextDocument(project('index.rst'));

        // Then — `drafts/` is excluded, so its label is in no index
        await waitFor('the reference to render broken', () =>
            codesFor(project('index.rst')).includes('link.broken-ref'),
        );
    });

    test('what the server could not read in conf.py is underlined there', async () => {
        // When — the server starts with the first reStructuredText document
        await vscode.window.showTextDocument(project('index.rst'));

        // Then
        const diagnostics = await waitFor('conf.py to be diagnosed', () => {
            const shown = vscode.languages.getDiagnostics(project('conf.py'));
            return shown.length > 0 && shown;
        });
        assert.deepStrictEqual(codesFor(project('conf.py')), ['conf.unread-setting']);
        assert.strictEqual(diagnostics[0].range.start.line, 7);
    });
});
