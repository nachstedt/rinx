import * as assert from 'assert';
import * as fs from 'fs';
import * as http from 'http';
import * as path from 'path';
import * as vscode from 'vscode';
import * as zlib from 'zlib';
import type { RinxApi } from '../../extension';
import { waitFor } from './wait';

/** The fixture workspace's `intersphinx/` project, whose `conf.py` each test writes. */
function project(file: string): vscode.Uri {
    const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
    return vscode.Uri.file(path.join(root, 'intersphinx', file));
}

/** An `objects.inv` listing the Python class `dict`, in Sphinx's version 2 format. */
function inventory(): Buffer {
    const header =
        '# Sphinx inventory version 2\n# Project: Python\n# Version: 3\n' +
        '# The remainder of this file is compressed using zlib.\n';
    return Buffer.concat([
        Buffer.from(header),
        zlib.deflateSync(Buffer.from('dict py:class 1 library/stdtypes.html#$ -\n')),
    ]);
}

/** The text of every hover at `position` in `uri`. */
async function hoverTexts(uri: vscode.Uri, position: vscode.Position): Promise<string[]> {
    const hovers = await vscode.commands.executeCommand<vscode.Hover[]>('vscode.executeHoverProvider', uri, position);
    return hovers.flatMap((hover) =>
        hover.contents.map((content) => (typeof content === 'string' ? content : content.value)),
    );
}

suite('Intersphinx (e2e)', () => {
    let server: http.Server;
    let base = '';

    suiteSetup(async () => {
        // A local site standing in for docs.python.org, on a free port.
        server = http.createServer((request, response) => {
            if (request.url === '/objects.inv') {
                response.writeHead(200, { 'Content-Type': 'application/octet-stream' });
                response.end(inventory());
            } else {
                response.writeHead(404);
                response.end();
            }
        });
        await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
        const address = server.address();
        assert.ok(address !== null && typeof address === 'object', 'the server listens on a port');
        base = `http://127.0.0.1:${address.port}/`;
    });

    suiteTeardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
        await new Promise((resolve) => server.close(resolve));
        // Leave the folder as the other suites expect it: `intersphinx/` no
        // project of its own.
        fs.rmSync(project('conf.py').fsPath, { force: true });
        const api = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx')?.exports;
        await waitFor('the intersphinx project to be gone', () =>
            api
                ?.indexStatus()
                ?.projects?.every((status) => status.kind !== 'sphinx' || !status.conf.startsWith('intersphinx')),
        );
    });

    test('a reference into a downloaded inventory resolves, and hover shows its address', async () => {
        // Given — `index.rst` references `dict`, which no document defines
        await vscode.window.showTextDocument(project('index.rst'));
        await waitFor('the reference to render broken', () =>
            vscode.languages.getDiagnostics(project('index.rst')).length > 0 ? true : undefined,
        );

        // When — the project declares an inventory that has it
        fs.writeFileSync(
            project('conf.py').fsPath,
            `extensions = ['sphinx.ext.intersphinx']\nintersphinx_mapping = {'python': ('${base}', None)}\n`,
        );

        // Then
        await waitFor('the reference to resolve', () =>
            vscode.languages.getDiagnostics(project('index.rst')).length === 0 ? true : undefined,
        );
        const texts = await hoverTexts(project('index.rst'), new vscode.Position(3, 16));
        assert.ok(
            texts.some((text) => text.includes(`${base}library/stdtypes.html#dict`)),
            JSON.stringify(texts),
        );
    });
});
