import * as assert from 'assert';
import * as vscode from 'vscode';
import type { RinxApi } from '../../extension';
import { waitFor } from '../e2e/wait';

suite('Without a rinx binary (e2e)', () => {
    test('the extension still activates and keeps its preview command', async () => {
        // Given — rinx.binaryPath names a file that does not exist
        const extension = vscode.extensions.getExtension('nachstedt.rinx');
        assert.ok(extension);

        // When — a document triggers activation, which starts the server
        await vscode.workspace.openTextDocument({ language: 'restructuredtext', content: '.. foo::\n' });
        await extension.activate();

        // Then — the failed start is reported, not thrown
        assert.ok(extension.isActive);
        const commands = await vscode.commands.getCommands(true);
        assert.ok(commands.includes('rinx.showPreview'));
    });

    test('the preview says the language server is not running', async () => {
        // Given
        const extension = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx');
        assert.ok(extension);
        const api = extension.isActive ? extension.exports : await extension.activate();
        const document = await vscode.workspace.openTextDocument({ language: 'restructuredtext', content: 'Hi\n' });
        await vscode.window.showTextDocument(document);

        // When
        await vscode.commands.executeCommand('rinx.showPreview');

        // Then
        await waitFor('the page saying so', () => api.previewHtml()?.includes('language server is not running'));
    });
});
