import * as assert from 'assert';
import * as path from 'path';
import * as vscode from 'vscode';

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
});
