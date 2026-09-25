import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import { BazelScanner, DiscoveredConfig } from './bazel';

const outputChannel = vscode.window.createOutputChannel('Rinx');

function log(msg: string) {
    outputChannel.appendLine(`[${new Date().toISOString()}] ${msg}`);
}

export function activate(context: vscode.ExtensionContext) {
    context.subscriptions.push(
        outputChannel,
        vscode.commands.registerCommand('rinx.showPreview', () => {
            PreviewPanel.createOrShow(context.extensionUri);
        })
    );
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
            new vscode.RelativePattern(workspaceRoot, '**/{BUILD,BUILD.bazel,WORKSPACE,WORKSPACE.bazel,MODULE.bazel}')
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
    private _disposables: vscode.Disposable[] = [];
    private static _configCache = new BazelConfigCache();
    private _scanner = new BazelScanner(log);
    private _debounceTimer: NodeJS.Timeout | undefined;

    public static createOrShow(extensionUri: vscode.Uri) {
        const column = vscode.window.activeTextEditor
            ? vscode.window.activeTextEditor.viewColumn
            : undefined;

        if (PreviewPanel.currentPanel) {
            PreviewPanel.currentPanel._panel.reveal(column);
            return;
        }

        const panel = vscode.window.createWebviewPanel(
            'rinxPreview',
            'Rinx Preview',
            column || vscode.ViewColumn.One,
            {
                enableScripts: true,
            }
        );

        PreviewPanel.currentPanel = new PreviewPanel(panel, extensionUri);
    }

    private constructor(panel: vscode.WebviewPanel, extensionUri: vscode.Uri) {
        this._panel = panel;
        this._extensionUri = extensionUri;

        this._update();

        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);

        vscode.workspace.onDidChangeTextDocument(e => {
            if (e.document === vscode.window.activeTextEditor?.document) {
                const config = vscode.workspace.getConfiguration('rinx');
                if (config.get('previewMode') === 'onType') {
                    this._scheduleUpdate();
                }
            }
        }, null, this._disposables);

        vscode.workspace.onDidSaveTextDocument(e => {
            if (vscode.window.activeTextEditor && e === vscode.window.activeTextEditor.document) {
                this._scheduleUpdate();
            }
        }, null, this._disposables);

        vscode.window.onDidChangeActiveTextEditor(() => {
            this._scheduleUpdate();
        }, null, this._disposables);

        this._disposables.push(PreviewPanel._configCache);
    }

    public dispose() {
        PreviewPanel.currentPanel = undefined;
        this._panel.dispose();
        while (this._disposables.length) {
            const x = this._disposables.pop();
            if (x) {
                x.dispose();
            }
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

        this._renderRst(editor.document);
    }

    private async _resolveConfig(document: vscode.TextDocument, workspaceRoot: string): Promise<{
        binaryPath: string,
        configPath: string,
        templatePath: string,
        indexPath: string
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

            if (targets.length === 1) {
                selectedTarget = targets[0];
            } else if (targets.length > 1) {
                selectedTarget = await vscode.window.showQuickPick(targets, {
                    placeHolder: 'Multiple Rinx sites found. Select one for preview:'
                });
            }

            if (selectedTarget) {
                log(`Using site target: ${selectedTarget}`);
                const configLabel = await this._scanner.queryAttribute(selectedTarget, 'config', workspaceRoot);
                const templateLabel = await this._scanner.queryAttribute(selectedTarget, 'template', workspaceRoot);
                log(`Config label: ${configLabel || '(not found)'}`);
                log(`Template label: ${templateLabel || '(not found)'}`);
                
                const configPath = configLabel ? await this._scanner.labelToFilesystemPath(configLabel, workspaceRoot) : undefined;
                const templatePath = templateLabel ? await this._scanner.labelToFilesystemPath(templateLabel, workspaceRoot) : undefined;
                const indexPath = this._scanner.deriveIndexPath(selectedTarget, workspaceRoot);
                const binaryPath = await this._scanner.deriveBinaryPath(selectedTarget, workspaceRoot);
                log(`Resolved paths — binary: ${binaryPath || '(not found)'}, config: ${configPath || '(not found)'}, template: ${templatePath || '(not found)'}, index: ${indexPath}`);

                if (configPath && templatePath && binaryPath) {
                    discovered = {
                        siteTarget: selectedTarget,
                        binaryPath,
                        configPath,
                        templatePath,
                        indexPath
                    };
                    log(`Auto-discovery succeeded: ${selectedTarget}`);
                    vscode.window.setStatusBarMessage(`Rinx: Using ${selectedTarget}`, 3000);
                } else {
                    log('Auto-discovery failed: one or more paths could not be resolved');
                    discovered = null;
                }
            } else {
                log(targets.length === 0
                    ? 'Auto-discovery failed: no rinx_site target found (is this a Bazel workspace?)'
                    : 'Auto-discovery cancelled: user dismissed site selection');
                discovered = null;
            }
            PreviewPanel._configCache.set(workspaceRoot, discovered);

            if (discovered === null) {
                const action = await vscode.window.showWarningMessage(
                    'Rinx: Auto-discovery failed. Falling back to settings. Check the Output panel for details.',
                    'Show Logs'
                );
                if (action === 'Show Logs') {
                    outputChannel.show();
                }
            }
        }

        return {
            binaryPath: discovered?.binaryPath || settings.get<string>('binaryPath') || 'rinx',
            configPath: discovered?.configPath || this._getAbsolutePath(settings.get<string>('configPath') || 'rinx.toml', workspaceRoot),
            templatePath: discovered?.templatePath || this._getAbsolutePath(settings.get<string>('templatePath') || 'templates/default.html', workspaceRoot),
            indexPath: discovered?.indexPath || this._getAbsolutePath(settings.get<string>('indexPath') || 'bazel-bin/Doc/site.project.index', workspaceRoot)
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

        const args = [
            'preview',
            '--doc-path', relDocPath,
            '--config', configPath,
            '--template', templatePath,
        ];

        if (fs.existsSync(indexPath)) {
            args.push('--index', indexPath);
        }

        const cmdLine = `${binaryPath} ${args.join(' ')}`;
        const child = cp.spawn(binaryPath, args, { cwd: workspaceRoot });

        let spawnErrorOccurred = false;

        child.on('error', err => {
            spawnErrorOccurred = true;
            if ((err as any).code === 'ENOENT') {
                this._panel.webview.html = `<h1>Binary Not Found</h1>
                    <p>The <code>rinx</code> binary was not found at <code>${binaryPath}</code>.</p>
                    <p>Please ensure <code>rinx</code> is in your PATH or set the <code>rinx.binaryPath</code> setting to the absolute path of the binary.</p>
                    <hr>
                    <p>Current Workspace Root: <code>${workspaceRoot}</code></p>`;
            } else {
                this._panel.webview.html = `<h1>Spawn Error</h1><pre>${err.message}</pre>
                    <h2>Command</h2><pre>${cmdLine}</pre>`;
            }
        });

        let html = '';
        let stderr = '';

        child.stdout.on('data', data => {
            html += data.toString();
        });

        child.stderr.on('data', data => {
            stderr += data.toString();
        });

        child.on('close', (code, signal) => {
            if (spawnErrorOccurred) return;
            if (code === 0) {
                this._panel.webview.html = html;
            } else {
                const escapedStderr = stderr.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
                this._panel.webview.html = `<h1>Render Error</h1>
                    <p>Exit code: <code>${code}</code> | Signal: <code>${signal || 'none'}</code></p>
                    <h2>stderr</h2><pre>${escapedStderr || '(empty)'}</pre>
                    <h2>Command</h2><pre>${cmdLine}</pre>
                    <h2>Working Directory</h2><pre>${workspaceRoot}</pre>`;
            }
        });

        child.stdin.on('error', () => {
            // Ignore EPIPE — the process may have exited before we finished writing
        });
        child.stdin.write(document.getText());
        child.stdin.end();
    }
}
