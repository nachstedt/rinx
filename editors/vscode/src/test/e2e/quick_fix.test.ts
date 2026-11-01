import * as assert from 'assert';
import * as vscode from 'vscode';
import { waitFor } from './wait';

/** The quick fixes VS Code collects for the start of `line` of `document`. */
async function quickFixesAt(document: vscode.TextDocument, line: number): Promise<vscode.CodeAction[]> {
    const at = new vscode.Range(line, 0, line, 0);
    const found = await vscode.commands.executeCommand<(vscode.CodeAction | vscode.Command)[]>(
        'vscode.executeCodeActionProvider',
        document.uri,
        at,
        vscode.CodeActionKind.QuickFix.value,
    );
    // A `Command` names its command by a string, an action by an object.
    return (found ?? []).filter((action): action is vscode.CodeAction => typeof action.command !== 'string');
}

suite('Language server quick fixes (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('a noqa quick fix silences the diagnostic it is offered for', async () => {
        // Given — an unknown directive inside a note's body
        const document = await vscode.workspace.openTextDocument({
            language: 'restructuredtext',
            content: '.. note::\n\n   First.\n\n   .. foo::\n',
        });
        await vscode.window.showTextDocument(document);
        await waitFor('the unknown directive', () => vscode.languages.getDiagnostics(document.uri).length > 0);

        // When
        // VS Code adds fixes of its own (its chat's "Fix"), so ours is found by title.
        const fixes = await quickFixesAt(document, 4);
        const fix = fixes.find((action) => action.title === 'Suppress directive.unknown with .. noqa:');
        assert.ok(
            fix?.edit,
            `the noqa fix with its edit, among ${JSON.stringify(fixes.map((action) => action.title))}`,
        );
        await vscode.workspace.applyEdit(fix.edit);

        // Then — written at the body's indentation, and the squiggle gone
        assert.strictEqual(
            document.getText(),
            '.. note::\n\n   First.\n\n   .. noqa: directive.unknown\n\n   .. foo::\n',
        );
        await waitFor('no diagnostics', () => vscode.languages.getDiagnostics(document.uri).length === 0);
        await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    });
});
