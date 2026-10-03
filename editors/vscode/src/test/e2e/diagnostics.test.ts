import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import { waitFor } from './wait';

/** A diagnostic's code as the text the server sent. */
function codeOf(diagnostic: vscode.Diagnostic): string | undefined {
    const code = diagnostic.code;
    return typeof code === 'object' ? String(code.value) : code === undefined ? undefined : String(code);
}

/** The diagnostics on `uri`, once there are some. */
function diagnosticsOn(uri: vscode.Uri): Promise<vscode.Diagnostic[]> {
    return waitFor(`diagnostics on ${uri.toString()}`, () => {
        const found = vscode.languages.getDiagnostics(uri);
        return found.length > 0 && found;
    });
}

/** Resolves once `uri` has no diagnostics left. */
async function noDiagnosticsOn(uri: vscode.Uri): Promise<void> {
    await waitFor(`no diagnostics on ${uri.toString()}`, () => vscode.languages.getDiagnostics(uri).length === 0);
}

async function openUntitled(content: string): Promise<vscode.TextDocument> {
    const document = await vscode.workspace.openTextDocument({ language: 'restructuredtext', content });
    await vscode.window.showTextDocument(document);
    return document;
}

/** The file `name` at the root of the fixture workspace. */
function workspaceFile(name: string): vscode.Uri {
    const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
    return vscode.Uri.file(path.join(root, name));
}

/** Replaces the whole text of `document` with `text`, leaving it unsaved. */
async function replaceAll(document: vscode.TextDocument, text: string): Promise<void> {
    const edit = new vscode.WorkspaceEdit();
    edit.replace(document.uri, new vscode.Range(0, 0, document.lineCount, 0), text);
    await vscode.workspace.applyEdit(edit);
}

/** Closes the active editor, discarding its changes, so nothing prompts. */
async function discardActiveEditor(): Promise<void> {
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
}

suite('Language server diagnostics (e2e)', () => {
    suiteSetup(() => {
        const binary = vscode.workspace.getConfiguration('rinx').get<string>('binaryPath') ?? '';
        assert.ok(
            fs.existsSync(binary),
            `no rinx binary at '${binary}': run \`cargo build\` at the repository root, or set RINX_BINARY`,
        );
    });

    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('an unknown directive is underlined with its code', async () => {
        // Given / When
        const document = await openUntitled('Prose.\n\n.. foo::\n');

        // Then
        const [diagnostic] = await diagnosticsOn(document.uri);
        assert.strictEqual(codeOf(diagnostic), 'directive.unknown');
        assert.strictEqual(diagnostic.source, 'rinx');
        assert.strictEqual(diagnostic.range.start.line, 2);
        await discardActiveEditor();
    });

    test('fixing the text clears its diagnostic', async () => {
        // Given
        const document = await openUntitled('.. foo::\n');
        await diagnosticsOn(document.uri);

        // When
        const edit = new vscode.WorkspaceEdit();
        edit.replace(document.uri, new vscode.Range(0, 0, document.lineCount, 0), 'Fixed.\n');
        await vscode.workspace.applyEdit(edit);

        // Then
        await noDiagnosticsOn(document.uri);
        await discardActiveEditor();
    });

    test('closing a document clears its diagnostics', async () => {
        // Given
        const document = await openUntitled('.. foo::\n');
        await diagnosticsOn(document.uri);

        // When
        await discardActiveEditor();

        // Then
        await noDiagnosticsOn(document.uri);
    });

    test('a column after an astral character is counted in UTF-16 units', async () => {
        // Given — a crab is two UTF-16 units, so the reference starts at 3
        const document = await openUntitled('🦀 |nosub|\n');

        // When
        const [diagnostic] = await diagnosticsOn(document.uri);

        // Then
        assert.strictEqual(codeOf(diagnostic), 'substitution.undefined');
        assert.deepStrictEqual([diagnostic.range.start.line, diagnostic.range.start.character], [0, 3]);
        await discardActiveEditor();
    });

    test('a saved document reads the files it includes from disk', async () => {
        // Given — a document including a fragment beside it
        const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
        const uri = vscode.Uri.file(path.join(root, 'with_include.rst'));

        // When
        await vscode.window.showTextDocument(uri);

        // Then — only the unknown directive, no unreadable include
        const diagnostics = await diagnosticsOn(uri);
        assert.deepStrictEqual(diagnostics.map(codeOf), ['directive.unknown']);
        assert.strictEqual(diagnostics[0].range.start.line, 5);
    });

    test('a mistake in an included file is underlined in that file', async () => {
        // Given — a document including a fragment whose third line is broken
        const includer = workspaceFile('broken_includer.rst');
        const fragment = workspaceFile('broken_fragment.rst');

        // When
        await vscode.window.showTextDocument(includer);

        // Then — on the fragment, which is not even open
        const diagnostics = await diagnosticsOn(fragment);
        assert.deepStrictEqual(diagnostics.map(codeOf), ['directive.unknown']);
        assert.strictEqual(diagnostics[0].range.start.line, 2);
    });

    test('an include that brings a problem in summarizes it and links to it', async () => {
        // Given
        const includer = workspaceFile('broken_includer.rst');
        const fragment = workspaceFile('broken_fragment.rst');

        // When
        await vscode.window.showTextDocument(includer);

        // Then — information on the include line, linking into the fragment
        const [summary] = await diagnosticsOn(includer);
        assert.strictEqual(summary.severity, vscode.DiagnosticSeverity.Information);
        assert.strictEqual(summary.range.start.line, 3);
        assert.match(summary.message, /problem in included file 'broken_fragment\.rst'/);
        const [related] = summary.relatedInformation ?? [];
        assert.strictEqual(related.location.uri.toString(), fragment.toString());
        assert.strictEqual(related.location.range.start.line, 2);
    });

    test('fixing an included file in its buffer clears it there', async () => {
        // Given
        await vscode.window.showTextDocument(workspaceFile('broken_includer.rst'));
        const fragment = await vscode.workspace.openTextDocument(workspaceFile('broken_fragment.rst'));
        await vscode.window.showTextDocument(fragment);
        await diagnosticsOn(fragment.uri);

        // When — unsaved
        await replaceAll(fragment, 'Fine.\n');

        // Then
        await noDiagnosticsOn(fragment.uri);
        await discardActiveEditor();
    });

    test('editing an included file re-diagnoses the document including it', async () => {
        // Given — a document using a substitution its fragment defines
        const user = workspaceFile('substitution_user.rst');
        await vscode.window.showTextDocument(user);
        const fragment = await vscode.workspace.openTextDocument(workspaceFile('substitution.rst'));
        await vscode.window.showTextDocument(fragment);

        // When — the definition is removed from the fragment's buffer
        await replaceAll(fragment, 'No definitions left.\n');

        // Then — the includer, untouched, now reports the reference
        const diagnostics = await diagnosticsOn(user);
        assert.deepStrictEqual(diagnostics.map(codeOf), ['substitution.undefined']);
        await discardActiveEditor();
    });
});
