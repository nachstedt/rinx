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

/** The text of every hover the editor shows at `position` of `uri`. */
async function hoverText(uri: vscode.Uri, position: vscode.Position): Promise<string> {
    const hovers = await vscode.commands.executeCommand<vscode.Hover[]>('vscode.executeHoverProvider', uri, position);
    return hovers
        .flatMap((hover) => hover.contents)
        .map((content) => (typeof content === 'string' ? content : content.value))
        .join('\n');
}

/** Every place going to the definition at `position` of `uri` leads, as `file:line`. */
async function definitionsAt(uri: vscode.Uri, position: vscode.Position): Promise<string[]> {
    const definitions = await vscode.commands.executeCommand<(vscode.Location | vscode.LocationLink)[]>(
        'vscode.executeDefinitionProvider',
        uri,
        position,
    );
    return definitions.map((definition) => {
        const [file, range] =
            'targetUri' in definition
                ? [definition.targetUri, definition.targetSelectionRange ?? definition.targetRange]
                : [definition.uri, definition.range];
        return `${path.basename(file.fsPath)}:${range.start.line}`;
    });
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

    test('a need type is a directive once conf.py declares sphinx-needs, its links are checked, its ids complete, hover and lead to it', async () => {
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
        const unknown = await waitFor('the needs to be known', () => {
            const shown = vscode.languages.getDiagnostics(project('index.rst'));
            return shown.length === 1 && codeOf(shown[0]) === 'entity.unknown-target' && shown[0];
        });
        // On the `:implements:` line, the one to change.
        assert.strictEqual(unknown.range.start.line, 10);
        const list = await vscode.commands.executeCommand<vscode.CompletionList>(
            'vscode.executeCompletionItemProvider',
            project('index.rst'),
            new vscode.Position(6, 11),
        );
        const need = list.items.find((item) => labelOf(item) === 'R_1');
        assert.ok(need, `R_1 among ${JSON.stringify(list.items.map(labelOf))}`);
        assert.strictEqual(need.detail, 'Brakes');

        // A relation's ids complete too: at the start of `R_1, R_GHOST`,
        // `R_GHOST` is listed already and `S_1` is the spec itself.
        const relation = await vscode.commands.executeCommand<vscode.CompletionList>(
            'vscode.executeCompletionItemProvider',
            project('index.rst'),
            new vscode.Position(10, 17),
        );
        assert.deepStrictEqual(relation.items.map(labelOf), ['R_1']);

        // Hovering the need, by its role or in the relation, shows the spec
        // tracing to it — a back-link no line of `R_1` itself writes.
        for (const at of [new vscode.Position(6, 12), new vscode.Position(10, 18)]) {
            const shown = await hoverText(project('index.rst'), at);
            assert.ok(shown.includes('R\\_1 — Brakes'), shown);
            assert.ok(shown.includes('S\\_1'), shown);
        }

        // Going to the need's definition, by its role or in the relation,
        // lands on its directive, `.. req:: Brakes`.
        for (const at of [new vscode.Position(6, 12), new vscode.Position(10, 18)]) {
            assert.deepStrictEqual(await definitionsAt(project('index.rst'), at), ['index.rst:3']);
        }
    });
});
