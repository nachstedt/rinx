// The report of what a project writes that rinx does not support: a panel
// drawn from the language server's `rinx/analyseProject` answer
// (crates/lsp/src/analysis.rs).

import * as vscode from 'vscode';
import { errorMessage } from './errors';
import { escapeHtml, LatestRequest, messagePage, NO_SERVER_PAGE } from './preview';
import type { ProjectStatus } from './status';

/** The request asking the server for the report. */
export const ANALYSE_PROJECT_METHOD = 'rinx/analyseProject';

/** The parameters of `rinx/analyseProject`. */
export interface AnalyseProjectParams {
    /** A document of the project asked about; without one, every project. */
    textDocument?: { uri: string };
}

/** How far rinx supports a declared extension. */
export type ExtensionSupport = 'complete' | 'incomplete' | 'unknown';

/** One extension a project declares. */
export interface ExtensionReport {
    name: string;
    support: ExtensionSupport;
    /** Where the compatibility page speaks of it, if it does. */
    link?: string;
}

/** What kind of thing a construct's count counts. */
export type ConstructKind =
    'unknownDirective' | 'unsupportedOption' | 'unsupportedRole' | 'unknownRole' | 'ignoredRole' | 'malformedDirective';

/** A place a construct was written, 1-based. */
export interface ExampleSite {
    uri: string;
    line: number;
    column: number;
}

/** One construct a project's documents wrote. */
export interface ConstructReport {
    kind: ConstructKind;
    name: string;
    count: number;
    /** The declared extension providing it, when one does. */
    provider?: string;
    /** Where the compatibility page speaks of it. */
    link?: string;
    /** Whether the link is its own row, rather than the section it would be listed in. */
    listed: boolean;
    examples: ExampleSite[];
}

/** What migrating a Sphinx project into a rinx build writes, and leaves out. */
export interface MigrationReport {
    /** The conf.py the migration is offered on. */
    conf: string;
    /** The files it writes. */
    files: string[];
    /** The files it would overwrite, which keep it from being offered. */
    conflicts: string[];
    /** What it leaves out, each a sentence. */
    findings: string[];
}

/** One project's part of the report. */
export interface ProjectReport {
    project: ProjectStatus;
    documents: number;
    extensions: ExtensionReport[];
    constructs: ConstructReport[];
    /** For a scanned Sphinx project. */
    migration?: MigrationReport;
}

/** The answer to `rinx/analyseProject`. */
export interface AnalysisReport {
    /** Whether the workspace has been scanned; until then a count may grow. */
    complete: boolean;
    projects: ProjectReport[];
}

/** What the panel needs of the language client. */
export interface AnalysisClient {
    sendRequest(method: string, params: AnalyseProjectParams): Promise<AnalysisReport>;
}

/** Each kind's section of the report, in the order the server lists them. */
const SECTIONS: { kind: ConstructKind; heading: string; explanation: string }[] = [
    {
        kind: 'unknownDirective',
        heading: 'Unknown directives',
        explanation: 'Drawn as an error block quoting the source; what they contain is not read.',
    },
    { kind: 'unsupportedOption', heading: 'Unsupported options', explanation: 'Ignored, as directive:option.' },
    {
        kind: 'unsupportedRole',
        heading: 'Unsupported roles',
        explanation: 'Roles of docutils or Sphinx rinx does not implement yet: shown as written.',
    },
    { kind: 'unknownRole', heading: 'Unknown roles', explanation: 'A typo, or an extension’s: shown as written.' },
    {
        kind: 'ignoredRole',
        heading: 'Ignored roles',
        explanation: 'Named by the project’s rinx_parser.toml: shown as their text, and not reported.',
    },
    {
        kind: 'malformedDirective',
        heading: 'Malformed directives',
        explanation: 'rinx implements these, and could not accept what this project wrote.',
    },
];

/** What the report says of each level of an extension's support. */
const SUPPORT_TEXT: Record<ExtensionSupport, string> = {
    complete: 'supported',
    incomplete: 'not fully supported',
    unknown: 'unknown to rinx',
};

/** How the report names `project`. */
export function projectTitle(project: ProjectStatus): string {
    if (project.kind === 'sphinx') return `Sphinx project ${project.conf}`;
    if (project.kind === 'bazel') return `rinx project ${project.site}`;
    return `Folder ${project.root}`;
}

/** The path of `site` relative to the workspace, and its line, as a place is named. */
function siteName(site: ExampleSite): string {
    return `${vscode.workspace.asRelativePath(vscode.Uri.parse(site.uri))}:${site.line}`;
}

/** The link to the compatibility page for `construct`, if it has one. */
function constructLink(construct: ConstructReport): string {
    if (!construct.link) return '';
    const text = construct.listed ? 'compatibility' : 'not listed';
    return `<a href="#" data-link="${escapeHtml(construct.link)}">${text}</a>`;
}

/** One table row for `construct`. */
function constructRow(construct: ConstructReport): string {
    const examples = construct.examples
        .map(
            (site) =>
                `<a href="#" data-uri="${escapeHtml(site.uri)}" data-line="${site.line}" data-column="${site.column}">${escapeHtml(siteName(site))}</a>`,
        )
        .join(', ');
    return `<tr>
    <td><code>${escapeHtml(construct.name)}</code></td>
    <td class="count">${construct.count}</td>
    <td>${escapeHtml(construct.provider ?? '')}</td>
    <td>${constructLink(construct)}</td>
    <td class="places">${examples}</td>
  </tr>`;
}

/** The section of `constructs` of one kind, or nothing when it has none. */
function constructSection(section: (typeof SECTIONS)[number], constructs: ConstructReport[]): string {
    const rows = constructs.filter((construct) => construct.kind === section.kind);
    if (rows.length === 0) return '';
    const total = rows.reduce((sum, construct) => sum + construct.count, 0);
    return `<h3>${escapeHtml(section.heading)} (${rows.length} distinct, ${total} uses)</h3>
  <p>${escapeHtml(section.explanation)}</p>
  <table>
    <thead><tr><th>Name</th><th class="count">Uses</th><th>Provided by</th><th>Status</th><th>First written at</th></tr></thead>
    <tbody>${rows.map(constructRow).join('')}</tbody>
  </table>`;
}

/** The list of what `project` declares, or nothing when it declares nothing. */
function extensionList(project: ProjectReport): string {
    if (project.extensions.length === 0) return '';
    const items = project.extensions.map((extension) => {
        const link = extension.link ? ` (<a href="#" data-link="${escapeHtml(extension.link)}">compatibility</a>)` : '';
        return `<li><code>${escapeHtml(extension.name)}</code>: ${SUPPORT_TEXT[extension.support]}${link}</li>`;
    });
    return `<h3>Declared extensions</h3><ul>${items.join('')}</ul>`;
}

/** A bulleted list of `items`, each already HTML. */
function htmlList(items: string[]): string {
    return `<ul>${items.map((item) => `<li>${item}</li>`).join('')}</ul>`;
}

/** The path of the file at `uri` relative to the workspace. */
function fileName(uri: string): string {
    return vscode.workspace.asRelativePath(vscode.Uri.parse(uri));
}

/** What migrating `project` writes and leaves out, with the button doing it — or nothing for a project it is not offered for. */
function migrationSection(project: ProjectReport): string {
    const migration = project.migration;
    if (!migration) return '';
    const files = htmlList(migration.files.map((uri) => `<code>${escapeHtml(fileName(uri))}</code>`));
    const action =
        migration.conflicts.length === 0
            ? `<button data-conf="${escapeHtml(migration.conf)}">Migrate to rinx</button>`
            : `<p class="notice">Not offered: it would overwrite ${migration.conflicts
                  .map((uri) => `<code>${escapeHtml(fileName(uri))}</code>`)
                  .join(', ')}.</p>`;
    const findings =
        migration.findings.length === 0 ? '' : `<p>Left out:</p>${htmlList(migration.findings.map(escapeHtml))}`;
    return `<h3>Migrating to rinx</h3>
  <p>Writes the project's Bazel build:</p>${files}${findings}${action}`;
}

/** One project's part of the page. */
function projectSection(project: ProjectReport): string {
    const docs = project.documents === 1 ? 'document' : 'documents';
    const constructs = SECTIONS.map((section) => constructSection(section, project.constructs)).join('');
    return `<section>
  <h2>${escapeHtml(projectTitle(project.project))}</h2>
  <p>${project.documents} ${docs}.${project.constructs.length === 0 ? ' Nothing rinx does not support.' : ''}</p>
  ${extensionList(project)}
  ${constructs}
  ${migrationSection(project)}
</section>`;
}

/** The webview's script, which sends what the reader clicks to the extension. */
const PANEL_SCRIPT = `
const vscode = acquireVsCodeApi();
document.addEventListener('click', (event) => {
    const target = event.target.closest('a, button');
    if (!target) return;
    event.preventDefault();
    if (target.id === 'refresh') {
        vscode.postMessage({ type: 'refresh' });
    } else if (target.dataset.conf) {
        vscode.postMessage({ type: 'migrate', conf: target.dataset.conf });
    } else if (target.dataset.link) {
        vscode.postMessage({ type: 'link', href: target.dataset.link });
    } else if (target.dataset.uri) {
        vscode.postMessage({
            type: 'example',
            uri: target.dataset.uri,
            line: Number(target.dataset.line),
            column: Number(target.dataset.column),
        });
    }
});`;

/** The page drawing `report`, whose script runs under `nonce`. */
export function reportPage(report: AnalysisReport, nonce: string): string {
    const scanning = report.complete
        ? ''
        : '<p class="notice">The workspace is still being scanned: these counts may grow.</p>';
    const projects =
        report.projects.length === 0
            ? '<p>No project holds any documents yet.</p>'
            : report.projects.map(projectSection).join('');
    return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'nonce-${nonce}'">
<title>Rinx: Project Analysis</title>
<style>
  body { color: var(--vscode-foreground); font-family: var(--vscode-font-family); }
  a { color: var(--vscode-textLink-foreground); }
  table { border-collapse: collapse; width: 100%; }
  th, td { border-bottom: 1px solid var(--vscode-panel-border); padding: 2px 8px; text-align: left; vertical-align: top; }
  .count { text-align: right; }
  .places { overflow-wrap: anywhere; }
  .notice { color: var(--vscode-editorWarning-foreground); }
</style>
</head>
<body>
  <h1>What rinx does not support here</h1>
  <button id="refresh">Refresh</button>
  ${scanning}
  ${projects}
  <script nonce="${nonce}">${PANEL_SCRIPT}</script>
</body>
</html>`;
}

/** The page for the report asked of `client`, or one saying why there is none. */
export async function analysisPage(client: AnalysisClient | undefined, nonce: string): Promise<string> {
    if (!client) return NO_SERVER_PAGE;
    try {
        return reportPage(await client.sendRequest(ANALYSE_PROJECT_METHOD, {}), nonce);
    } catch (error: unknown) {
        return messagePage('The project could not be analysed', errorMessage(error));
    }
}

/** A value for a page's script nonce. */
function newNonce(): string {
    return Array.from({ length: 32 }, () => Math.floor(Math.random() * 16).toString(16)).join('');
}

/** The one report panel. */
export class AnalysisPanel {
    public static currentPanel: AnalysisPanel | undefined;
    private readonly disposables: vscode.Disposable[] = [];
    private readonly requests = new LatestRequest();
    private disposed = false;
    private page: string | undefined;

    /** Reveals the panel, creating it if there is none; `client` gives the running language client, if any. */
    public static createOrShow(client: () => AnalysisClient | undefined): void {
        if (AnalysisPanel.currentPanel) {
            AnalysisPanel.currentPanel.panel.reveal();
            void AnalysisPanel.currentPanel.refresh();
            return;
        }
        const panel = vscode.window.createWebviewPanel(
            'rinxAnalysis',
            'Rinx: Project Analysis',
            vscode.ViewColumn.Active,
            {
                enableScripts: true,
                localResourceRoots: [],
            },
        );
        AnalysisPanel.currentPanel = new AnalysisPanel(panel, client);
    }

    private constructor(
        private readonly panel: vscode.WebviewPanel,
        private readonly client: () => AnalysisClient | undefined,
    ) {
        this.panel.onDidDispose(() => this.dispose(), null, this.disposables);
        this.panel.webview.onDidReceiveMessage(
            (message: {
                type?: string;
                href?: unknown;
                uri?: unknown;
                line?: unknown;
                column?: unknown;
                conf?: unknown;
            }) => {
                if (message.type === 'refresh') {
                    void this.refresh();
                } else if (message.type === 'migrate' && typeof message.conf === 'string') {
                    void this.migrate(message.conf);
                } else if (message.type === 'link' && typeof message.href === 'string') {
                    void this.openLink(message.href);
                } else if (
                    message.type === 'example' &&
                    typeof message.uri === 'string' &&
                    typeof message.line === 'number' &&
                    typeof message.column === 'number'
                ) {
                    void openExample({ uri: message.uri, line: message.line, column: message.column });
                }
            },
            null,
            this.disposables,
        );
        void this.refresh();
    }

    /** The page the panel shows, once it has one. */
    public get html(): string | undefined {
        return this.page;
    }

    /** Asks for the report again and shows it. */
    public async refresh(): Promise<void> {
        const request = this.requests.next();
        const page = await analysisPage(this.client(), newNonce());
        if (this.disposed || !this.requests.isLatest(request)) return;
        this.page = page;
        this.panel.webview.html = page;
    }

    /** Migrates the project configured by `conf`, then shows what is left. */
    private async migrate(conf: string): Promise<void> {
        await vscode.commands.executeCommand('rinx.migrateToRinx', vscode.Uri.parse(conf));
        await this.refresh();
    }

    /** Opens a compatibility link in the browser. */
    private async openLink(href: string): Promise<void> {
        await vscode.env.openExternal(vscode.Uri.parse(href));
    }

    private dispose(): void {
        this.disposed = true;
        AnalysisPanel.currentPanel = undefined;
        for (const disposable of this.disposables) disposable.dispose();
    }
}

/** Opens the source at `site`. */
export async function openExample(site: ExampleSite): Promise<void> {
    const position = new vscode.Position(site.line - 1, site.column - 1);
    await vscode.window.showTextDocument(vscode.Uri.parse(site.uri), {
        selection: new vscode.Range(position, position),
    });
}
