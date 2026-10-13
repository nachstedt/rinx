import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import type { RinxApi } from '../../extension';
import { waitFor } from '../e2e/wait';

suite('The packaged extension (e2e-vsix)', () => {
    test('the extension under test is the installed VSIX, not the checkout', () => {
        // Given
        const extension = vscode.extensions.getExtension('nachstedt.rinx');
        assert.ok(extension, 'the VSIX was not installed');
        const checkout = path.resolve(__dirname, '../../..');

        // When / Then — the rest of the e2e suite tests what users install
        assert.notStrictEqual(path.resolve(extension.extensionPath), checkout);
        const manifest: unknown = extension.packageJSON;
        const main: unknown =
            typeof manifest === 'object' && manifest !== null ? Reflect.get(manifest, 'main') : undefined;
        assert.strictEqual(main, './dist/extension.js');
    });

    test('a bundled VSIX starts the language server from the binary inside it', async function () {
        if (process.env.RINX_VSIX_BUNDLED !== '1') {
            this.skip();
        }
        // Given — no rinx.binaryPath, no Bazel discovery, no rinx on the PATH
        const extension = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx');
        assert.ok(extension, 'the VSIX was not installed');
        const api = extension.isActive ? extension.exports : await extension.activate();

        // When
        const binary = await waitFor('the language server to start', () => api.serverBinary());

        // Then — the binary is the VSIX's own, still executable once installed
        assert.strictEqual(binary, path.join(extension.extensionPath, 'bin', 'rinx'));
        fs.accessSync(binary, fs.constants.X_OK);
    });
});
