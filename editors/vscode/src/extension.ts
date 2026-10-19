import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import { LanguageClient } from 'vscode-languageclient/node';
import { BazelScanner, DiscoveredConfig } from './bazel';
import { bundledBinaryPath, chooseBinaryPath, explicitValue } from './binary';
import { createLanguageClient, resolveServerBinary } from './client';
import {
    chooseInterpreter,
    CONF_INTERPRETER_CHANGED_METHOD,
    CONF_INTERPRETER_METHOD,
    ConfInterpreterParams,
    InterpreterDependencies,
    PythonExtensionApi,
    selectedByPythonExtension,
} from './conf';
import { errorMessage } from './errors';
import {
    CONSENT_KEY,
    DownloadSetting,
    downloadWithFetch,
    FETCH_INVENTORY_METHOD,
    FetchInventoryParams,
    InventoryFetcher,
    RememberedConsent,
} from './inventories';
import {
    answerManifests,
    BAZEL_MANIFESTS_CHANGED_METHOD,
    BAZEL_MANIFESTS_METHOD,
    BazelManifestsParams,
    debounced,
    ManifestDependencies,
} from './manifests';
import { IndexStatus, IndexStatusItem, STATUS_METHOD } from './status';

// A log channel rather than a plain one: the language client writes its own
// messages and traces into it, and requires one. It also timestamps each line.
const outputChannel = vscode.window.createOutputChannel('Rinx', { log: true });

function log(msg: string) {
    outputChannel.info(msg);
}

let client: LanguageClient | undefined;
// The binary the language server was started from, once it has been.
let serverBinary: string | undefined;
// The site the preview was last pointed at, and whom to tell when it
// changes: a document several sites list belongs to it, unless `rinx.site`
// names another.
let previewSite: string | undefined;
let onPreviewSiteChanged: (() => void) | undefined;

/** `rinx.site`, the site the user chose for this workspace, if set. */
function configuredSite(): string | undefined {
    return vscode.workspace.getConfiguration('rinx').get<string>('site') || undefined;
}

/** What the extension offers other extensions — and its own end-to-end tests. */
export interface RinxApi {
    /** The last status the language server reported for its workspace index. */
    indexStatus(): IndexStatus | undefined;
    /** The binary the language server was started from, once it has been. */
    serverBinary(): string | undefined;
}

export function activate(context: vscode.ExtensionContext): RinxApi {
    const statusItem = new IndexStatusItem();
    const bundled = bundledBinaryPath(context.extensionPath, process.platform, fs.existsSync);
    context.subscriptions.push(
        outputChannel,
        statusItem,
        vscode.commands.registerCommand('rinx.showPreview', () => {
            PreviewPanel.createOrShow(context.extensionUri, bundled);
        }),
        vscode.commands.registerCommand('rinx.showLogs', () => {
            outputChannel.show();
        }),
    );
    // Not awaited: finding the binary may build it with Bazel, and the
    // preview command must not wait for that.
    void startServer(statusItem, bundled, inventoryFetcher(context), context.subscriptions);
    return { indexStatus: () => statusItem.last, serverBinary: () => serverBinary };
}

export async function deactivate(): Promise<void> {
    await client?.stop();
    client = undefined;
}

/** The fetcher answering the server's inventory requests, caching in `context`'s global storage. */
function inventoryFetcher(context: vscode.ExtensionContext): InventoryFetcher {
    return new InventoryFetcher({
        storageDir: context.globalStorageUri.fsPath,
        setting: () => vscode.workspace.getConfiguration('rinx').get<DownloadSetting>('inventories.download') ?? 'ask',
        remembered: () => context.workspaceState.get<RememberedConsent>(CONSENT_KEY),
        remember: (consent) => context.workspaceState.update(CONSENT_KEY, consent),
        prompt: (message, ...choices) => vscode.window.showInformationMessage(message, ...choices),
        download: downloadWithFetch,
        now: () => Date.now(),
        log,
    });
}

/** The `rinx` settings that apply to the file at `uri`. */
function settingsFor(uri: string): vscode.WorkspaceConfiguration {
    return vscode.workspace.getConfiguration('rinx', vscode.Uri.parse(uri));
}

/**
 * Chooses the interpreter of each `conf.py` the server asks about (see
 * conf.ts), and tells the server through `notify` whenever the choice may have
 * changed: trust granted, a `rinx.conf.*` setting changed, or another Python
 * environment selected.
 */
class ConfInterpreters implements vscode.Disposable {
    private readonly listeners: vscode.Disposable[] = [];
    private python: Promise<PythonExtensionApi | undefined> | undefined;

    constructor(private readonly notify: () => void) {
        this.listeners.push(
            vscode.workspace.onDidGrantWorkspaceTrust(notify),
            vscode.workspace.onDidChangeConfiguration((event) => {
                if (event.affectsConfiguration('rinx.conf')) notify();
            }),
        );
    }

    readonly dependencies: InterpreterDependencies = {
        isTrusted: () => vscode.workspace.isTrusted,
        execute: (conf) => settingsFor(conf).get<boolean>('conf.execute') ?? false,
        pythonPath: (conf) => settingsFor(conf).get<string>('conf.pythonPath') || undefined,
        folder: (conf) => vscode.workspace.getWorkspaceFolder(vscode.Uri.parse(conf))?.uri.fsPath,
        selectedInterpreter: async (conf) => {
            const api = await this.pythonExtension();
            return api ? selectedByPythonExtension(api, vscode.Uri.parse(conf)) : undefined;
        },
    };

    /** The Python extension's API, activated and watched, if it is installed. */
    private pythonExtension(): Promise<PythonExtensionApi | undefined> {
        this.python ??= (async () => {
            const extension = vscode.extensions.getExtension<PythonExtensionApi>('ms-python.python');
            if (!extension) return undefined;
            try {
                const api = extension.isActive ? extension.exports : await extension.activate();
                this.listeners.push(api.environments.onDidChangeActiveEnvironmentPath(this.notify));
                return api;
            } catch (error: unknown) {
                log(`The Python extension could not be asked for an interpreter: ${errorMessage(error)}`);
                return undefined;
            }
        })();
        return this.python;
    }

    dispose(): void {
        for (const listener of this.listeners) listener.dispose();
    }
}

/** Starts the language server, reporting a failure rather than throwing. */
async function startServer(
    statusItem: IndexStatusItem,
    bundled: string | undefined,
    fetcher: InventoryFetcher,
    subscriptions: vscode.Disposable[],
): Promise<void> {
    let binary = 'rinx';
    try {
        binary = await resolveServerBinary(new BazelScanner(log), bundled, log);
        // Every handler is registered before the client starts: the server
        // asks for the Bazel manifests as soon as it has found the projects.
        client = createLanguageClient(binary, outputChannel);
        client.onNotification(STATUS_METHOD, (status: IndexStatus) => statusItem.update(status));
        client.onRequest(FETCH_INVENTORY_METHOD, (params: FetchInventoryParams) => fetcher.fetchInventory(params));
        const running = client;
        const notify = (method: string) => {
            if (running.isRunning()) void running.sendNotification(method);
        };
        const interpreters = new ConfInterpreters(() => notify(CONF_INTERPRETER_CHANGED_METHOD));
        subscriptions.push(interpreters);
        client.onRequest(CONF_INTERPRETER_METHOD, (params: ConfInterpreterParams) =>
            chooseInterpreter(params, interpreters.dependencies),
        );
        const manifests = new BazelManifests(() => notify(BAZEL_MANIFESTS_CHANGED_METHOD));
        subscriptions.push(manifests);
        client.onRequest(BAZEL_MANIFESTS_METHOD, (params: BazelManifestsParams) =>
            answerManifests(params, manifests.dependencies),
        );
        await client.start();
        serverBinary = binary;
    } catch (error: unknown) {
        log(`Language server failed to start: ${errorMessage(error)}`);
        const action = await vscode.window.showWarningMessage(
            `Rinx: the language server could not be started from '${binary}'. Set rinx.binaryPath to a rinx binary.`,
            'Show Logs',
        );
        if (action === 'Show Logs') {
            outputChannel.show();
        }
    }
}

/** How long after the last edit of a Bazel file the manifests are rebuilt. */
const MANIFEST_REBUILD_DELAY_MS = 2000;

/**
 * Builds the manifests the server asks for (see manifests.ts), and tells the
 * server through `notify` whenever they may have changed: a `BUILD`, `.bzl`,
 * `MODULE.bazel` or `WORKSPACE` file edited, `rinx.site` or
 * `rinx.autoDiscover` changed, or the preview pointed at another site.
 */
class BazelManifests implements vscode.Disposable {
    private readonly listeners: vscode.Disposable[] = [];
    private readonly scanner = new BazelScanner(log);
    private readonly rebuild: ReturnType<typeof debounced>;

    constructor(notify: () => void) {
        this.rebuild = debounced(notify, MANIFEST_REBUILD_DELAY_MS);
        const watcher = vscode.workspace.createFileSystemWatcher(
            '**/{BUILD,BUILD.bazel,*.bzl,MODULE.bazel,WORKSPACE,WORKSPACE.bazel}',
        );
        watcher.onDidChange(this.rebuild);
        watcher.onDidCreate(this.rebuild);
        watcher.onDidDelete(this.rebuild);
        this.listeners.push(
            watcher,
            vscode.workspace.onDidChangeConfiguration((event) => {
                if (event.affectsConfiguration('rinx.site') || event.affectsConfiguration('rinx.autoDiscover')) {
                    notify();
                }
            }),
        );
        onPreviewSiteChanged = () => {
            if (!configuredSite()) notify();
        };
    }

    readonly dependencies: ManifestDependencies = {
        autoDiscover: () => vscode.workspace.getConfiguration('rinx').get<boolean>('autoDiscover') ?? true,
        preferredSite: () => configuredSite() ?? previewSite,
        findSiteTargets: (root) => this.scanner.findSiteTargets(root),
        buildManifests: (root, sites) => this.scanner.buildManifests(root, sites),
        executionRoot: (root) => this.scanner.getExecutionRoot(root),
    };

    dispose(): void {
        this.rebuild.cancel();
        onPreviewSiteChanged = undefined;
        for (const listener of this.listeners) listener.dispose();
    }
}

export class BazelConfigCache {
    private cache = new Map<string, DiscoveredConfig | null>();
    private watcher: vscode.FileSystemWatcher | undefined;

    get(workspaceRoot: string): DiscoveredConfig | null | undefined {
        return this.cache.get(workspaceRoot);
    }

    set(workspaceRoot: string, config: DiscoveredConfig | null): void {
        this.cache.set(workspaceRoot, config);
    }

    watchForInvalidation(workspaceRoot: string, onInvalidate: () => void): void {
        if (this.watcher) return;

        // Watch for BUILD, WORKSPACE, MODULE.bazel changes
        this.watcher = vscode.workspace.createFileSystemWatcher(
            new vscode.RelativePattern(workspaceRoot, '**/{BUILD,BUILD.bazel,WORKSPACE,WORKSPACE.bazel,MODULE.bazel}'),
        );

        this.watcher.onDidChange(() => {
            this.cache.clear();
            onInvalidate();
        });
        this.watcher.onDidCreate(() => {
            this.cache.clear();
            onInvalidate();
        });
        this.watcher.onDidDelete(() => {
            this.cache.clear();
            onInvalidate();
        });
    }

    dispose() {
        this.watcher?.dispose();
    }
}

class PreviewPanel {
    public static currentPanel: PreviewPanel | undefined;
    private readonly _panel: vscode.WebviewPanel;
    private readonly _extensionUri: vscode.Uri;
    // The binary the VSIX carries, if any: see binary.ts.
    private readonly _bundledBinary: string | undefined;
    private _disposables: vscode.Disposable[] = [];
    private static _configCache = new BazelConfigCache();
    private _scanner = new BazelScanner(log);
    private _debounceTimer: NodeJS.Timeout | undefined;
    // Set once the panel is closed. A render already running when the user
    // closes it finishes later, and must not write into the disposed webview.
    private _disposed = false;

    public static createOrShow(extensionUri: vscode.Uri, bundledBinary: string | undefined) {
        const column = vscode.window.activeTextEditor ? vscode.window.activeTextEditor.viewColumn : undefined;

        if (PreviewPanel.currentPanel) {
            PreviewPanel.currentPanel._panel.reveal(column);
            return;
        }

        const panel = vscode.window.createWebviewPanel('rinxPreview', 'Rinx Preview', column || vscode.ViewColumn.One, {
            enableScripts: true,
        });

        PreviewPanel.currentPanel = new PreviewPanel(panel, extensionUri, bundledBinary);
    }

    private constructor(panel: vscode.WebviewPanel, extensionUri: vscode.Uri, bundledBinary: string | undefined) {
        this._panel = panel;
        this._extensionUri = extensionUri;
        this._bundledBinary = bundledBinary;

        this._update();

        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);

        vscode.workspace.onDidChangeTextDocument(
            (e) => {
                if (e.document === vscode.window.activeTextEditor?.document) {
                    const config = vscode.workspace.getConfiguration('rinx');
                    if (config.get('previewMode') === 'onType') {
                        this._scheduleUpdate();
                    }
                }
            },
            null,
            this._disposables,
        );

        vscode.workspace.onDidSaveTextDocument(
            (e) => {
                if (vscode.window.activeTextEditor && e === vscode.window.activeTextEditor.document) {
                    this._scheduleUpdate();
                }
            },
            null,
            this._disposables,
        );

        vscode.window.onDidChangeActiveTextEditor(
            () => {
                this._scheduleUpdate();
            },
            null,
            this._disposables,
        );

        this._disposables.push(PreviewPanel._configCache);
    }

    public dispose() {
        this._disposed = true;
        clearTimeout(this._debounceTimer);
        PreviewPanel.currentPanel = undefined;
        this._panel.dispose();
        while (this._disposables.length) {
            const x = this._disposables.pop();
            if (x) {
                x.dispose();
            }
        }
    }

    /** Shows `html` in the panel, unless the panel has been closed meanwhile. */
    private _show(html: string) {
        if (!this._disposed) {
            this._panel.webview.html = html;
        }
    }

    private _scheduleUpdate() {
        if (this._debounceTimer) {
            clearTimeout(this._debounceTimer);
        }
        this._debounceTimer = setTimeout(() => this._update(), 150);
    }

    private _update() {
        const editor = vscode.window.activeTextEditor;
        if (!editor || editor.document.languageId !== 'restructuredtext') {
            return;
        }

        void this._renderRst(editor.document);
    }

    private async _resolveConfig(
        document: vscode.TextDocument,
        workspaceRoot: string,
    ): Promise<{
        binaryPath: string;
        configPath: string;
        templatePath: string;
        indexPath: string;
    }> {
        const settings = vscode.workspace.getConfiguration('rinx');
        const autoDiscover = settings.get<boolean>('autoDiscover') ?? true;

        let discovered = PreviewPanel._configCache.get(workspaceRoot);

        if (autoDiscover && discovered === undefined) {
            // Run discovery
            log(`Auto-discovery starting for workspace: ${workspaceRoot}`);
            PreviewPanel._configCache.watchForInvalidation(workspaceRoot, () => this._scheduleUpdate());

            const targets = await this._scanner.findSiteTargets(workspaceRoot);
            log(`Found ${targets.length} site target(s): ${targets.join(', ') || '(none)'}`);
            let selectedTarget: string | undefined;

            const configured = configuredSite();
            if (configured && targets.includes(configured)) {
                selectedTarget = configured;
            } else if (targets.length === 1) {
                selectedTarget = targets[0];
            } else if (targets.length > 1) {
                selectedTarget = await vscode.window.showQuickPick(targets, {
                    placeHolder: 'Multiple Rinx sites found. Select one for preview:',
                });
            }

            if (selectedTarget) {
                log(`Using site target: ${selectedTarget}`);
                if (selectedTarget !== previewSite) {
                    previewSite = selectedTarget;
                    onPreviewSiteChanged?.();
                }
                const configLabel = await this._scanner.queryAttribute(selectedTarget, 'config', workspaceRoot);
                const templateLabel = await this._scanner.queryAttribute(selectedTarget, 'template', workspaceRoot);
                log(`Config label: ${configLabel || '(not found)'}`);
                log(`Template label: ${templateLabel || '(not found)'}`);

                const configPath = configLabel
                    ? await this._scanner.labelToFilesystemPath(configLabel, workspaceRoot)
                    : undefined;
                const templatePath = templateLabel
                    ? await this._scanner.labelToFilesystemPath(templateLabel, workspaceRoot)
                    : undefined;
                const indexPath = this._scanner.deriveIndexPath(selectedTarget, workspaceRoot);
                const binaryPath = await this._scanner.deriveBinaryPath(workspaceRoot);
                log(
                    `Resolved paths — binary: ${binaryPath || '(not found)'}, config: ${configPath || '(not found)'}, template: ${templatePath || '(not found)'}, index: ${indexPath}`,
                );

                if (configPath && templatePath && binaryPath) {
                    discovered = {
                        siteTarget: selectedTarget,
                        binaryPath,
                        configPath,
                        templatePath,
                        indexPath,
                    };
                    log(`Auto-discovery succeeded: ${selectedTarget}`);
                    vscode.window.setStatusBarMessage(`Rinx: Using ${selectedTarget}`, 3000);
                } else {
                    log('Auto-discovery failed: one or more paths could not be resolved');
                    discovered = null;
                }
            } else {
                log(
                    targets.length === 0
                        ? 'Auto-discovery failed: no rinx_site target found (is this a Bazel workspace?)'
                        : 'Auto-discovery cancelled: user dismissed site selection',
                );
                discovered = null;
            }
            PreviewPanel._configCache.set(workspaceRoot, discovered);

            if (discovered === null) {
                const action = await vscode.window.showWarningMessage(
                    'Rinx: Auto-discovery failed. Falling back to settings. Check the Output panel for details.',
                    'Show Logs',
                );
                if (action === 'Show Logs') {
                    outputChannel.show();
                }
            }
        }

        return {
            binaryPath: chooseBinaryPath(
                explicitValue(settings.inspect<string>('binaryPath')),
                discovered?.binaryPath,
                this._bundledBinary,
            ),
            configPath:
                discovered?.configPath ||
                this._getAbsolutePath(settings.get<string>('configPath') || 'rinx.toml', workspaceRoot),
            templatePath:
                discovered?.templatePath ||
                this._getAbsolutePath(settings.get<string>('templatePath') || 'templates/default.html', workspaceRoot),
            indexPath:
                discovered?.indexPath ||
                this._getAbsolutePath(
                    settings.get<string>('indexPath') || 'bazel-bin/Doc/site.project.index',
                    workspaceRoot,
                ),
        };
    }

    private _getAbsolutePath(p: string, workspaceRoot: string): string {
        return path.isAbsolute(p) ? p : path.join(workspaceRoot, p);
    }

    private async _renderRst(document: vscode.TextDocument) {
        const workspaceRoot = vscode.workspace.workspaceFolders?.[0].uri.fsPath;

        if (!workspaceRoot) {
            return;
        }

        const { binaryPath, configPath, templatePath, indexPath } = await this._resolveConfig(document, workspaceRoot);
        const relDocPath = path.relative(workspaceRoot, document.uri.fsPath);

        const args = ['preview', '--doc-path', relDocPath, '--config', configPath, '--template', templatePath];

        if (fs.existsSync(indexPath)) {
            args.push('--index', indexPath);
        }

        const cmdLine = `${binaryPath} ${args.join(' ')}`;
        const child = cp.spawn(binaryPath, args, { cwd: workspaceRoot });

        let spawnErrorOccurred = false;

        child.on('error', (err) => {
            spawnErrorOccurred = true;
            if ('code' in err && err.code === 'ENOENT') {
                this._show(`<h1>Binary Not Found</h1>
                    <p>The <code>rinx</code> binary was not found at <code>${binaryPath}</code>.</p>
                    <p>Please ensure <code>rinx</code> is in your PATH or set the <code>rinx.binaryPath</code> setting to the absolute path of the binary.</p>
                    <hr>
                    <p>Current Workspace Root: <code>${workspaceRoot}</code></p>`);
            } else {
                this._show(`<h1>Spawn Error</h1><pre>${err.message}</pre>
                    <h2>Command</h2><pre>${cmdLine}</pre>`);
            }
        });

        let html = '';
        let stderr = '';

        child.stdout.on('data', (data: Buffer) => {
            html += data.toString();
        });

        child.stderr.on('data', (data: Buffer) => {
            stderr += data.toString();
        });

        child.on('close', (code, signal) => {
            if (spawnErrorOccurred) return;
            if (code === 0) {
                this._show(html);
            } else {
                const escapedStderr = stderr.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
                this._show(`<h1>Render Error</h1>
                    <p>Exit code: <code>${code}</code> | Signal: <code>${signal || 'none'}</code></p>
                    <h2>stderr</h2><pre>${escapedStderr || '(empty)'}</pre>
                    <h2>Command</h2><pre>${cmdLine}</pre>
                    <h2>Working Directory</h2><pre>${workspaceRoot}</pre>`);
            }
        });

        child.stdin.on('error', () => {
            // Ignore EPIPE — the process may have exited before we finished writing
        });
        child.stdin.write(document.getText());
        child.stdin.end();
    }
}
