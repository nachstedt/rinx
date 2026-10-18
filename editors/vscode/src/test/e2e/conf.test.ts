import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import type { RinxApi } from '../../extension';
import { waitFor } from './wait';

/** A file of the project this suite writes, `conf_run/`. */
function project(file: string): vscode.Uri {
    const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
    return vscode.Uri.file(path.join(root, 'conf_run', file));
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

/** The interpreter the test runs `conf.py` in: RINX_PYTHON, or `python3` on the PATH. */
const PYTHON = process.env.RINX_PYTHON ?? 'python3';

suite('Running conf.py (e2e)', () => {
    suiteTeardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
        // Leave the folder as the other suites expect it: nothing run, and
        // `conf_run/` gone.
        const settings = vscode.workspace.getConfiguration('rinx');
        await settings.update('conf.execute', undefined, vscode.ConfigurationTarget.Workspace);
        await settings.update('conf.pythonPath', undefined, vscode.ConfigurationTarget.Workspace);
        fs.rmSync(path.dirname(project('conf.py').fsPath), { recursive: true, force: true });
        const api = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx')?.exports;
        await waitFor('the project to be gone', () =>
            api
                ?.indexStatus()
                ?.projects?.every((status) => status.kind !== 'sphinx' || !status.conf.startsWith('conf_run')),
        );
    });

    test('a computed setting is exact once rinx.conf.execute lets the server run conf.py', async () => {
        // Given — a project computing its epilog, which reading cannot see
        fs.mkdirSync(path.dirname(project('conf.py').fsPath), { recursive: true });
        fs.writeFileSync(
            project('conf.py').fsPath,
            "version = '.'.join(['2', '7'])\nrst_epilog = '.. |version| replace:: ' + version + '\\n'\n",
        );
        fs.writeFileSync(project('index.rst').fsPath, 'Computed\n========\n\nVersion |version|.\n');
        await vscode.window.showTextDocument(project('index.rst'));
        await waitFor('the substitution to be undefined', () =>
            codesFor(project('index.rst')).includes('substitution.undefined'),
        );

        // When
        const settings = vscode.workspace.getConfiguration('rinx');
        await settings.update('conf.pythonPath', PYTHON, vscode.ConfigurationTarget.Workspace);
        await settings.update('conf.execute', true, vscode.ConfigurationTarget.Workspace);

        // Then — the run's epilog defines it, and the status says so
        await waitFor('the substitution to be defined', () => codesFor(project('index.rst')).length === 0);
        const api = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx')?.exports;
        const ran = await waitFor('the status to name the interpreter', () =>
            api
                ?.indexStatus()
                ?.projects?.find((status) => status.kind === 'sphinx' && status.conf === 'conf_run/conf.py'),
        );
        assert.deepStrictEqual(ran, { kind: 'sphinx', conf: 'conf_run/conf.py', executedWith: PYTHON });
        assert.deepStrictEqual(codesFor(project('conf.py')), []);
    });
});
