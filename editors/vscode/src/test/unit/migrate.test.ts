import * as assert from 'assert';
import * as vscode from 'vscode';
import { editedFiles, MIGRATE_KIND, MigrationDependencies, migrateProject, outcomeMessage } from '../../migrate';

const CONF = vscode.Uri.file('/docs/conf.py');
const BUILD = vscode.Uri.file('/docs/BUILD.bazel');
const MODULE = vscode.Uri.file('/docs/MODULE.bazel');

/** The edit the server's action carries: creating and writing two files. */
function migrationEdit(): vscode.WorkspaceEdit {
    const edit = new vscode.WorkspaceEdit();
    edit.createFile(BUILD, { overwrite: false });
    edit.insert(BUILD, new vscode.Position(0, 0), 'rinx_library()\n');
    edit.createFile(MODULE, { overwrite: false });
    edit.insert(MODULE, new vscode.Position(0, 0), 'module()\n');
    return edit;
}

/** Dependencies offering `actions`, applying an edit as `applies` says, and recording what they did. */
function dependencies(actions: vscode.CodeAction[], applies = true) {
    const done = { asked: [] as string[], applied: 0, saved: [] as string[] };
    const deps: MigrationDependencies = {
        codeActions: (conf, kind) => {
            done.asked.push(`${conf.path} ${kind}`);
            return Promise.resolve(actions);
        },
        applyEdit: () => {
            done.applied += 1;
            return Promise.resolve(applies);
        },
        save: (uri) => {
            done.saved.push(uri.path);
            return Promise.resolve();
        },
    };
    return { deps, done };
}

/** The server's migration, carrying `edit`. */
function migration(edit: vscode.WorkspaceEdit): vscode.CodeAction {
    const action = new vscode.CodeAction('Migrate to rinx', vscode.CodeActionKind.Empty.append(MIGRATE_KIND));
    action.edit = edit;
    return action;
}

suite('Migration Test Suite', () => {
    test('editedFiles lists each file an edit writes once, in order', () => {
        // Given / When
        const files = editedFiles(migrationEdit());

        // Then
        assert.deepStrictEqual(
            files.map((file) => file.path),
            ['/docs/BUILD.bazel', '/docs/MODULE.bazel'],
        );
    });

    test('migrateProject applies the server’s action and saves every file it wrote', async () => {
        // Given
        const { deps, done } = dependencies([migration(migrationEdit())]);

        // When
        const outcome = await migrateProject(CONF, deps);

        // Then
        assert.deepStrictEqual(done.asked, [`/docs/conf.py ${MIGRATE_KIND}`]);
        assert.strictEqual(done.applied, 1);
        assert.deepStrictEqual(done.saved, ['/docs/BUILD.bazel', '/docs/MODULE.bazel']);
        assert.strictEqual(outcome.kind, 'migrated');
    });

    test('migrateProject says when the server offers no migration', async () => {
        // Given — only a quick fix
        const { deps, done } = dependencies([new vscode.CodeAction('Fix', vscode.CodeActionKind.QuickFix)]);

        // When
        const outcome = await migrateProject(CONF, deps);

        // Then
        assert.deepStrictEqual(outcome, { kind: 'notOffered' });
        assert.strictEqual(done.applied, 0);
    });

    test('migrateProject saves nothing when the edit is not applied', async () => {
        // Given
        const { deps, done } = dependencies([migration(migrationEdit())], false);

        // When
        const outcome = await migrateProject(CONF, deps);

        // Then
        assert.deepStrictEqual(outcome, { kind: 'notApplied' });
        assert.deepStrictEqual(done.saved, []);
    });

    test('outcomeMessage names the files and what switches the project', () => {
        // Given
        const outcome = { kind: 'migrated' as const, files: [BUILD] };

        // When / Then
        assert.match(outcomeMessage(outcome, true), /wrote \/docs\/BUILD\.bazel\. rinx now builds its manifests/);
        assert.match(outcomeMessage(outcome, false), /Turn on rinx\.autoDiscover/);
        assert.match(outcomeMessage({ kind: 'notOffered' }, true), /cannot be migrated now/);
    });
});
