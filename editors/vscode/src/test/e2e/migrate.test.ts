import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import type { RinxApi } from '../../extension';
import type { MigrationOutcome } from '../../migrate';
import { waitFor } from './wait';

/** The fixture workspace's Sphinx project, `sphinx/`. */
function project(file: string): string {
    const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
    return path.join(root, 'sphinx', file);
}

/** The files a migration of `sphinx/` writes. */
const WRITTEN = ['BUILD.bazel', 'rinx.toml', 'rst_prolog.txt', 'MODULE.bazel'];

/** The extension's API, once it is active. */
async function rinx(): Promise<RinxApi> {
    const extension = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx');
    assert.ok(extension, 'the extension is installed');
    return extension.isActive ? extension.exports : await extension.activate();
}

suite('Migration (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
        // The other suites read sphinx/ as the Sphinx project it was.
        for (const file of WRITTEN) fs.rmSync(project(file), { force: true });
    });

    test('the command writes the Sphinx project’s Bazel build from its conf.py', async () => {
        // Given
        await vscode.window.showTextDocument(vscode.Uri.file(project('conf.py')));
        const api = await rinx();
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');

        // When
        const outcome = await vscode.commands.executeCommand<MigrationOutcome | undefined>('rinx.migrateToRinx');

        // Then
        assert.strictEqual(outcome?.kind, 'migrated');
        for (const file of WRITTEN) assert.ok(fs.existsSync(project(file)), `${file} is written`);
        const build = fs.readFileSync(project('BUILD.bazel'), 'utf8');
        assert.match(build, /"drafts\/\*\*"/);
        assert.match(build, /images = \["logo\.svg"\]/);
        assert.match(build, /rst_prolog = "rst_prolog\.txt"/);
        assert.strictEqual(fs.readFileSync(project('rst_prolog.txt'), 'utf8'), '.. |release| replace:: 1.0\n');
    });
});
