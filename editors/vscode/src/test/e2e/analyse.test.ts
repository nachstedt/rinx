import * as assert from 'assert';
import * as path from 'path';
import * as vscode from 'vscode';
import type { RinxApi } from '../../extension';
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

suite('Project analysis (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('the command lists what the Sphinx project declares and writes', async () => {
        // Given
        await vscode.window.showTextDocument(project('index.rst'));
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When
        await vscode.commands.executeCommand('rinx.analyseProject');
        const page = await waitFor('the report', () => {
            const html = api.analysisHtml();
            return html?.includes('sphinx/conf.py') && html;
        });

        // Then
        assert.match(page, /sphinx\.ext\.autodoc<\/code>: not fully supported/);
        assert.match(page, /compatibility\.html#compatibility-autodoc/);
        assert.match(page, />sphinx\/api\.rst:4</);
    });
});
