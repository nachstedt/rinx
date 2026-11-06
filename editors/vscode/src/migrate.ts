// "Rinx: Migrate Project to rinx" (roadmap #37): applies the language
// server's "Migrate to rinx" source action on a Sphinx project's conf.py,
// writing the project's Bazel build. The server computes the edit
// (crates/lsp/src/code_action/migrate.rs), so every editor offered the
// action writes the same files; once they exist, the Bazel file watcher in
// extension.ts tells the server, which switches the project to rinx mode.

import * as vscode from 'vscode';

/** The kind the server offers the migration as. */
export const MIGRATE_KIND = 'source.migrateToRinx';

/** What the command needs of VS Code, so a test can stand in for it. */
export interface MigrationDependencies {
    /** The source actions of `kind` offered on `conf`. */
    codeActions(conf: vscode.Uri, kind: string): Promise<vscode.CodeAction[]>;
    /** Applies `edit`, answering whether it was applied. */
    applyEdit(edit: vscode.WorkspaceEdit): Promise<boolean>;
    /** Saves the file at `uri`, which the edit left open and unsaved. */
    save(uri: vscode.Uri): Promise<void>;
}

/** What became of a migration, as the user is told. */
export type MigrationOutcome =
    { kind: 'migrated'; files: vscode.Uri[] } | { kind: 'notOffered' } | { kind: 'notApplied' };

/** The files `edit` writes, each once, in the order it writes them. */
export function editedFiles(edit: vscode.WorkspaceEdit): vscode.Uri[] {
    const files: vscode.Uri[] = [];
    for (const [uri] of edit.entries()) {
        if (!files.some((known) => known.toString() === uri.toString())) files.push(uri);
    }
    return files;
}

/** Migrates the Sphinx project configured by `conf`. */
export async function migrateProject(conf: vscode.Uri, deps: MigrationDependencies): Promise<MigrationOutcome> {
    const actions = await deps.codeActions(conf, MIGRATE_KIND);
    const edit = actions.find((action) => action.kind?.value === MIGRATE_KIND)?.edit;
    if (!edit) return { kind: 'notOffered' };
    if (!(await deps.applyEdit(edit))) return { kind: 'notApplied' };
    const files = editedFiles(edit);
    await Promise.all(files.map((file) => deps.save(file)));
    return { kind: 'migrated', files };
}

/** What the user is told of `outcome`, the extension running Bazel when `autoDiscover`. */
export function outcomeMessage(outcome: MigrationOutcome, autoDiscover: boolean): string {
    if (outcome.kind === 'notOffered') {
        return (
            'Rinx: this project cannot be migrated now. Wait for the workspace scan to finish; ' +
            'a file the migration writes may exist already — see Rinx: Analyse Project.'
        );
    }
    if (outcome.kind === 'notApplied') return 'Rinx: the migration was not applied.';
    const names = outcome.files.map((file) => vscode.workspace.asRelativePath(file)).join(', ');
    const next = autoDiscover
        ? 'rinx now builds its manifests and reads the project as its Bazel build.'
        : 'Turn on rinx.autoDiscover to read the project as its Bazel build.';
    return `Rinx: wrote ${names}. ${next}`;
}

/** Whether the extension runs Bazel, which reading the migrated project as its build needs. */
function autoDiscovers(): boolean {
    return vscode.workspace.getConfiguration('rinx').get<boolean>('autoDiscover', true);
}

/** The conf.py to migrate: `given`, the active editor's, or the one the user picks. */
async function confToMigrate(given: vscode.Uri | undefined): Promise<vscode.Uri | undefined> {
    if (given) return given;
    const active = vscode.window.activeTextEditor?.document.uri;
    if (active?.path.endsWith('/conf.py')) return active;
    const found = await vscode.workspace.findFiles('**/conf.py', '**/{node_modules,.*,bazel-*}/**');
    if (found.length <= 1) return found[0];
    const picked = await vscode.window.showQuickPick(
        found.map((uri) => ({ label: vscode.workspace.asRelativePath(uri), uri })),
        { placeHolder: 'The Sphinx project to migrate to rinx' },
    );
    return picked?.uri;
}

/** The command: migrates `given`, or a project found as `confToMigrate` says. */
export async function migrateCommand(given?: vscode.Uri): Promise<MigrationOutcome | undefined> {
    const conf = await confToMigrate(given);
    if (!conf) {
        void vscode.window.showWarningMessage('Rinx: no Sphinx project (conf.py) to migrate.');
        return undefined;
    }
    const outcome = await migrateProject(conf, {
        codeActions: async (uri, kind) =>
            (await vscode.commands.executeCommand<vscode.CodeAction[] | undefined>(
                'vscode.executeCodeActionProvider',
                uri,
                new vscode.Range(0, 0, 0, 0),
                kind,
            )) ?? [],
        applyEdit: (edit) => Promise.resolve(vscode.workspace.applyEdit(edit)),
        save: async (uri) => {
            await (await vscode.workspace.openTextDocument(uri)).save();
        },
    });
    const message = outcomeMessage(outcome, autoDiscovers());
    if (outcome.kind === 'migrated') void vscode.window.showInformationMessage(message);
    else void vscode.window.showWarningMessage(message);
    return outcome;
}
