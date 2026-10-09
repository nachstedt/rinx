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

/** A diagnostic's code as the text the server sent. */
function codeOf(diagnostic: vscode.Diagnostic): string | undefined {
    const code = diagnostic.code;
    return typeof code === 'object' ? String(code.value) : code === undefined ? undefined : String(code);
}

/** The extension's API, once it is active. */
async function rinx(): Promise<RinxApi> {
    const extension = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx');
    assert.ok(extension, 'the extension is installed');
    return extension.isActive ? extension.exports : await extension.activate();
}

/** The codes of the diagnostics on `uri`. */
function codesOn(uri: vscode.Uri): (string | undefined)[] {
    return vscode.languages.getDiagnostics(uri).map(codeOf);
}

suite('File-system watching (e2e)', () => {
    const target = workspaceFile('watched_target.rst');
    const renamed = workspaceFile('watched_renamed.rst');

    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
        // Leave the fixture as the other suites expect it.
        try {
            await vscode.workspace.fs.rename(renamed, target);
        } catch {
            // Already restored by the test itself.
        }
    });

    test('renaming a file breaks the references to it, and renaming it back mends them', async () => {
        // Given — `watched_referrer.rst` refers to `watched_target`, which
        // the finished scan has indexed.
        const referrer = workspaceFile('watched_referrer.rst');
        await vscode.window.showTextDocument(referrer);
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');
        // A render from before the scan may still show the target missing.
        await waitFor('the referrer rendered clean', () => codesOn(referrer).length === 0);

        // When — renamed as the Explorer renames it
        await vscode.workspace.fs.rename(target, renamed);

        // Then
        await waitFor('the reference to the renamed file underlined', () => {
            const codes = codesOn(referrer);
            return codes.length === 1 && codes[0] === 'link.broken-doc';
        });

        // When — renamed back
        await vscode.workspace.fs.rename(renamed, target);

        // Then
        await waitFor('the reference mended', () => codesOn(referrer).length === 0);
    });
});
