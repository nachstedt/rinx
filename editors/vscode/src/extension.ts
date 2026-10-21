import * as vscode from 'vscode';
import * as fs from 'fs';
import { LanguageClient } from 'vscode-languageclient/node';
import { BazelScanner } from './bazel';
import { bundledBinaryPath } from './binary';
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
import type { DiagramsMessage } from './diagrams';
import { PanelClient, PreviewPanel } from './preview';
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

/** The language client, while it runs: what the preview asks for pages and links. */
function runningClient(): PanelClient | undefined {
    return client?.isRunning() ? client : undefined;
}

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
    /** The page the preview panel shows, if it is open and shows one. */
    previewHtml(): string | undefined;
    /** Follows `href` as a click on it in the preview panel does, if one is open. */
    followPreviewLink(href: string): Promise<void>;
    /** What the preview panel last reported of the diagrams it drew, if it is open. */
    previewDiagrams(): DiagramsMessage | undefined;
}

export function activate(context: vscode.ExtensionContext): RinxApi {
    const statusItem = new IndexStatusItem();
    const bundled = bundledBinaryPath(context.extensionPath, process.platform, fs.existsSync);
    context.subscriptions.push(
        outputChannel,
        statusItem,
        vscode.commands.registerCommand('rinx.showPreview', () => {
            PreviewPanel.createOrShow(runningClient, context.extensionUri);
        }),
        vscode.commands.registerCommand('rinx.showLogs', () => {
            outputChannel.show();
        }),
    );
    // Not awaited: finding the binary may build it with Bazel, and the
    // preview command must not wait for that — it shows the server is not
    // running yet, and refreshes once it is.
    void startServer(statusItem, bundled, inventoryFetcher(context), context.subscriptions);
    return {
        indexStatus: () => statusItem.last,
        serverBinary: () => serverBinary,
        previewHtml: () => PreviewPanel.currentPanel?.html,
        followPreviewLink: async (href) => {
            await PreviewPanel.currentPanel?.followLink(href);
        },
        previewDiagrams: () => PreviewPanel.currentPanel?.diagrams,
    };
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
        // A preview opened while the server started shows that it was not
        // running yet.
        PreviewPanel.currentPanel?.refresh();
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
 * `MODULE.bazel` or `WORKSPACE` file edited, or `rinx.site` or
 * `rinx.autoDiscover` changed.
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
    }

    readonly dependencies: ManifestDependencies = {
        autoDiscover: () => vscode.workspace.getConfiguration('rinx').get<boolean>('autoDiscover') ?? true,
        preferredSite: configuredSite,
        findSiteTargets: (root) => this.scanner.findSiteTargets(root),
        buildManifests: (root, sites) => this.scanner.buildManifests(root, sites),
        executionRoot: (root) => this.scanner.getExecutionRoot(root),
    };

    dispose(): void {
        this.rebuild.cancel();
        for (const listener of this.listeners) listener.dispose();
    }
}
