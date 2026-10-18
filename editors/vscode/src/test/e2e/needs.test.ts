import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import type { RinxApi } from '../../extension';
import { waitFor } from './wait';

/** The fixture workspace's `needs/` project, whose `conf.py` the suite writes. */
function project(file: string): vscode.Uri {
    const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
    return vscode.Uri.file(path.join(root, 'needs', file));
}

/** The code of `diagnostic`, as the server sent it. */
function codeOf(diagnostic: vscode.Diagnostic): string {
    const code = diagnostic.code;
    return typeof code === 'object' ? String(code.value) : String(code);
}

/** The codes of what the editor shows for `uri`. */
function codesFor(uri: vscode.Uri): string[] {
    return vscode.languages.getDiagnostics(uri).map(codeOf);
}

/** An item's label as the editor shows it, whichever form the server sent. */
function labelOf(item: vscode.CompletionItem): string {
    return typeof item.label === 'string' ? item.label : item.label.label;
}

suite('sphinx-needs project (e2e)', () => {
    suiteTeardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
        // Leave the folder as the other suites expect it: `needs/` no
        // project of its own.
        fs.rmSync(project('conf.py').fsPath, { force: true });
        const api = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx')?.exports;
        await waitFor('the needs project to be gone', () =>
            api
                ?.indexStatus()
                ?.projects?.every((status) => status.kind !== 'sphinx' || !status.conf.startsWith('needs')),
        );
    });

    test('a need type is a directive once conf.py declares sphinx-needs, its links are checked, and its ids complete', async () => {
        // Given — with no `conf.py`, `.. req::` is an unknown directive
        await vscode.window.showTextDocument(project('index.rst'));
        await waitFor('the need to be unknown', () => codesFor(project('index.rst')).includes('directive.unknown'));

        // When — the project declares sphinx-needs and its `ubproject.toml`
        fs.writeFileSync(
            project('conf.py').fsPath,
            "extensions = ['sphinx_needs']\nneeds_from_toml = 'ubproject.toml'\n",
        );

        // Then the directives are known — what is left is the spec's link to
        // a requirement no document declares, which only the whole project
        // reveals — and `:need:` completes the ids
        await waitFor('the needs to be known', () => {
            const codes = codesFor(project('index.rst'));
            return codes.length === 1 && codes[0] === 'entity.unknown-target';
        });
        const list = await vscode.commands.executeCommand<vscode.CompletionList>(
            'vscode.executeCompletionItemProvider',
            project('index.rst'),
            new vscode.Position(6, 11),
        );
        const need = list.items.find((item) => labelOf(item) === 'R_1');
        assert.ok(need, `R_1 among ${JSON.stringify(list.items.map(labelOf))}`);
        assert.strictEqual(need.detail, 'Brakes');
    });
});
