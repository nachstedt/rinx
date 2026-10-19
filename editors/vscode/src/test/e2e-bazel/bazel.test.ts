import * as assert from 'assert';
import * as path from 'path';
import * as vscode from 'vscode';
import type { RinxApi } from '../../extension';
import { statusText } from '../../status';
import { waitFor } from '../e2e/wait';

/** `file` of the fixture's Bazel workspace. */
function workspaceFile(file: string): vscode.Uri {
    const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
    return vscode.Uri.file(path.join(root, file));
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

suite('Bazel project (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('the status bar names the site the manifest describes', async () => {
        // Given
        await vscode.window.showTextDocument(workspaceFile('reqs/index.rst'));
        const api = await rinx();

        // When — the server asks for the manifests, and the extension builds them
        const status = await waitFor('the site to be read from its manifest', () => {
            const reported = api.indexStatus();
            return (
                reported?.state === 'ready' &&
                reported.projects?.some((project) => project.kind === 'bazel') &&
                reported
            );
        });

        // Then
        assert.ok(statusText(status).startsWith('rinx: rinx project //:site · '), statusText(status));
    });

    test("each document is parsed with its own library's schema", async () => {
        // When
        await vscode.window.showTextDocument(workspaceFile('reqs/index.rst'));
        await vscode.window.showTextDocument(workspaceFile('other/index.rst'));

        // Then — `//reqs:docs` declares `req`, `//other:docs` does not
        await waitFor('the other library to report its entity as unknown', () =>
            codesFor(workspaceFile('other/index.rst')).includes('directive.unknown'),
        );
        await waitFor(
            'the entity library to report nothing',
            () => codesFor(workspaceFile('reqs/index.rst')).length === 0,
        );
    });
});
