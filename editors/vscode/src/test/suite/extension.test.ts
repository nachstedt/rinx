import * as assert from 'assert';
import * as vscode from 'vscode';

suite('Extension Test Suite', () => {
	test('Extension should be present', () => {
		assert.ok(vscode.extensions.getExtension('nachstedt.rinx'));
	});

	test('Opening a reStructuredText document activates the extension', async () => {
		const extension = vscode.extensions.getExtension('nachstedt.rinx');
		assert.ok(extension);

		await vscode.workspace.openTextDocument({ language: 'restructuredtext', content: '.. foo::\n' });

		// Activation is asynchronous; wait for it rather than for a fixed time.
		for (let attempt = 0; attempt < 50 && !extension.isActive; attempt++) {
			await new Promise(resolve => setTimeout(resolve, 100));
		}
		assert.ok(extension.isActive);
	});
});
