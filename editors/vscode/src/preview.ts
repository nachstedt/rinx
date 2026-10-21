// The preview panel: shows the page the language server renders for the
// active reStructuredText document, with a `rinx/preview` request.
//
// The server holds every document's latest text and the project's current
// index, so the page is never stale: a title changed in another file shows at
// once, and no process is started per edit (see docs/vscode.rst).
//
// The page links its stylesheet and images where they lie, under the address
// the webview serves the file system at (`resourceRoot`), which reaches the
// workspace folders alone (`localResourceRoots`) — the server links nothing
// outside them.
//
// A page is loaded into the webview once; every later page of the same panel
// is posted to it and swapped into the open document by the script loaded
// with the first (`PAGE_UPDATER`), so the reader keeps their place. Assigning
// `webview.html` instead reloads the document, which VS Code then scrolls
// back down — visibly, under the theme's smooth scrolling.
//
// A click on a link is the panel's to follow, since the page's addresses are
// the site's, which the webview has none of (roadmap #40). A `#fragment`
// scrolls the page in the webview itself; an `http(s):` or `mailto:` link
// opens in the browser; any other is sent back to the server with a
// `rinx/previewLink` request, which names the source it leads to — a
// document, opened beside the panel and previewed at the link's anchor, or a
// `:download:` file — or none, for a page the build writes on its own.
//
// A diagram arrives as its PlantUML text, which the page draws itself with
// PlantUML's JavaScript build, loaded from the extension (`diagrams.ts`).

import * as vscode from 'vscode';
import {
    DIAGRAM_DRAWER,
    DiagramEngine,
    DiagramsMessage,
    diagramEngine,
    engineDirectory,
    engineScripts,
    isDiagramsMessage,
} from './diagrams';
import { errorMessage } from './errors';

/** The request the server answers with a document's page. */
export const PREVIEW_METHOD = 'rinx/preview';

/** What a `rinx/preview` request asks for. */
export interface PreviewParams {
    textDocument: { uri: string };
    /** The address the webview serves the file system's root under. */
    resourceRoot?: string;
}

/** The server's answer to a `rinx/preview` request. */
export interface PreviewResult {
    html: string;
}

/** The request the server answers with where a link of a page leads. */
export const PREVIEW_LINK_METHOD = 'rinx/previewLink';

/** What a `rinx/previewLink` request asks for. */
export interface PreviewLinkParams {
    textDocument: { uri: string };
    /** The link's `href`, as the page writes it. */
    href: string;
}

/** The server's answer to a `rinx/previewLink` request, when the link leads to a source. */
export interface PreviewLinkResult {
    kind: 'document' | 'file';
    uri: string;
    fragment?: string;
}

/** What the panel needs of the language client to show a page. */
export interface PreviewClient {
    sendRequest(method: string, params: PreviewParams): Promise<PreviewResult>;
}

/** What the panel needs of the language client to follow a link. */
export interface PreviewLinkClient {
    sendRequest(method: string, params: PreviewLinkParams): Promise<PreviewLinkResult | null>;
}

/** How long after the last edit the page is asked for again. */
export const PREVIEW_DELAY_MS = 150;

/** `text` with the characters HTML gives a meaning escaped. */
export function escapeHtml(text: string): string {
    return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

/** A page telling the reader why there is no preview: a heading and a detail. */
export function messagePage(heading: string, detail: string): string {
    return `<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"></head>
<body>
  <h1>${escapeHtml(heading)}</h1>
  <p>${escapeHtml(detail)}</p>
  <p>The Rinx output channel has the details (<em>Rinx: Show Logs</em>).</p>
</body>
</html>`;
}

/** The page shown while no language server runs. */
export const NO_SERVER_PAGE = messagePage(
    'The Rinx language server is not running',
    'The preview is rendered by the language server. It is starting, or could not be started from rinx.binaryPath.',
);

/**
 * Hands out a number per request and says whether a reply is the latest's,
 * so a slow reply to an older request never overwrites a newer page.
 */
export class LatestRequest {
    private issued = 0;

    /** The number of a request about to be sent. */
    next(): number {
        return ++this.issued;
    }

    /** Whether the request numbered `request` is the latest sent. */
    isLatest(request: number): boolean {
        return request === this.issued;
    }
}

/** What the panel shows: a document's page, or a page saying why there is none. */
export interface Shown {
    kind: 'page' | 'message';
    html: string;
    /** The document, as a URI. */
    document: string;
    /** The anchor the page is shown at, when a link named one. */
    fragment?: string;
}

/**
 * The page for `document`, asked of `client`, or a page saying why there is
 * none; its files are linked under `resourceRoot` when given.
 */
export async function renderPage(
    client: PreviewClient | undefined,
    document: vscode.Uri,
    resourceRoot?: string,
): Promise<Shown> {
    const uri = document.toString();
    if (!client) return { kind: 'message', html: NO_SERVER_PAGE, document: uri };
    const params: PreviewParams = { textDocument: { uri } };
    if (resourceRoot !== undefined) params.resourceRoot = resourceRoot;
    try {
        const result = await client.sendRequest(PREVIEW_METHOD, params);
        return { kind: 'page', html: result.html, document: uri };
    } catch (error: unknown) {
        return {
            kind: 'message',
            html: messagePage('The page could not be rendered', errorMessage(error)),
            document: uri,
        };
    }
}

/** The message posting a later page to the webview. */
export interface PageMessage {
    type: 'page';
    html: string;
    document: string;
    /** The anchor to scroll to, rather than keeping the position. */
    fragment?: string;
}

/** The message scrolling the page the webview shows to an anchor, or to its top. */
export interface ScrollMessage {
    type: 'scroll';
    fragment?: string;
}

/** What following a link of a page does. */
export type LinkAction =
    | { kind: 'external'; uri: string }
    | { kind: 'scroll'; fragment?: string }
    | { kind: 'open'; uri: string; fragment?: string }
    | { kind: 'file'; uri: string }
    | { kind: 'none' };

/** Whether `href` opens in the browser: an `http(s):` or `mailto:` link. */
export function isExternalHref(href: string): boolean {
    return /^(https?|mailto):/i.test(href);
}

/** Whether `href` starts with a URI scheme, which no link into the site does. */
function hasScheme(href: string): boolean {
    return /^[a-z][a-z0-9+.-]*:/i.test(href);
}

/**
 * What following `href`, a link of the page of `document`, does: asked of
 * `client` unless the link leaves the site. A link the server finds no source
 * for, or cannot answer, does nothing.
 */
export async function resolveLink(
    client: PreviewLinkClient | undefined,
    document: vscode.Uri,
    href: string,
): Promise<LinkAction> {
    if (isExternalHref(href)) return { kind: 'external', uri: href };
    if (hasScheme(href) || !client) return { kind: 'none' };
    let target: PreviewLinkResult | null;
    try {
        target = await client.sendRequest(PREVIEW_LINK_METHOD, { textDocument: { uri: document.toString() }, href });
    } catch {
        return { kind: 'none' };
    }
    if (!target) return { kind: 'none' };
    if (target.kind === 'file') return { kind: 'file', uri: target.uri };
    if (vscode.Uri.parse(target.uri).toString() === document.toString()) {
        return { kind: 'scroll', fragment: target.fragment };
    }
    return { kind: 'open', uri: target.uri, fragment: target.fragment };
}

/**
 * The script a page is loaded with. It swaps each posted page into the open
 * document, keeping the scroll position while the document is the same one,
 * and remembers the position in the webview's state, so a reload — the panel
 * hidden and shown again — restores it at once rather than scrolling there.
 * A page shown at an anchor scrolls to it instead. It says `ready` once it
 * listens, and `shown` after each swap.
 *
 * A click on a link is never the webview's to follow: a `#fragment` scrolls
 * the page, and any other link is posted to the panel as `link`.
 *
 * It draws the page's diagrams after each page, through the drawer, which the
 * engine calls too once it has loaded.
 */
const PAGE_UPDATER = `(() => {
    const vscode = acquireVsCodeApi();
    const drawDiagrams = DRAWER((message) => vscode.postMessage(message), window, document);
    window.rinxDrawDiagrams = drawDiagrams;
    let current = DOCUMENT;
    // A jump, not the theme's smooth scroll, whenever the position is restored.
    document.documentElement.style.scrollBehavior = 'auto';
    // The element an anchor names — written as is, or percent-encoded.
    const anchored = (fragment) => {
        const element = document.getElementById(fragment);
        if (element) return element;
        try {
            return document.getElementById(decodeURIComponent(fragment));
        } catch {
            return null;
        }
    };
    // Scrolls to the anchor named fragment, or to the top without one.
    const scrollToAnchor = (fragment) => {
        const element = fragment ? anchored(fragment) : null;
        if (element) element.scrollIntoView();
        else window.scrollTo(0, 0);
    };
    const remember = () => vscode.setState({ document: current, y: window.scrollY });
    const fragment = FRAGMENT;
    const saved = vscode.getState();
    if (fragment) scrollToAnchor(fragment);
    else if (saved && saved.document === current) window.scrollTo(0, saved.y);
    window.addEventListener('scroll', remember, { passive: true });
    document.addEventListener('click', (event) => {
        const link = event.target instanceof Element ? event.target.closest('a[href]') : null;
        if (!link) return;
        event.preventDefault();
        const href = link.getAttribute('href');
        if (href.startsWith('#')) scrollToAnchor(href.slice(1));
        else vscode.postMessage({ type: 'link', href });
    });
    window.addEventListener('message', (event) => {
        const message = event.data;
        if (message && message.type === 'scroll') {
            scrollToAnchor(message.fragment);
            return;
        }
        if (!message || message.type !== 'page') return;
        const y = message.document === current ? window.scrollY : 0;
        const next = new DOMParser().parseFromString(message.html, 'text/html');
        // An unchanged head is kept, so the stylesheet is not applied again.
        if (next.head.innerHTML !== document.head.innerHTML) {
            document.head.replaceWith(document.adoptNode(next.head));
        }
        document.body.replaceWith(document.adoptNode(next.body));
        current = message.document;
        if (message.fragment) scrollToAnchor(message.fragment);
        else window.scrollTo(0, y);
        remember();
        drawDiagrams();
        vscode.postMessage({ type: 'shown' });
    });
    drawDiagrams();
    vscode.postMessage({ type: 'ready' });
})();`;

/** `value` as a script's literal, with `<` escaped so it cannot end the script early. */
function scriptLiteral(value: string | null): string {
    return JSON.stringify(value).replace(/</g, '\\u003c');
}

/**
 * `page`, the page of `document`, with the script that swaps later pages into
 * it, placed last in its body, and shown at the anchor `fragment` when given;
 * `engine`, when given, is loaded after it to draw the page's diagrams.
 */
export function withPageUpdater(page: string, document: string, fragment?: string, engine?: DiagramEngine): string {
    const updater = PAGE_UPDATER.replace('DRAWER', () => DIAGRAM_DRAWER)
        .replace('DOCUMENT', () => scriptLiteral(document))
        .replace('FRAGMENT', () => scriptLiteral(fragment ?? null));
    const script = `<script>${updater}</script>${engine ? engineScripts(engine) : ''}`;
    const end = page.lastIndexOf('</body>');
    return end < 0 ? page + script : page.slice(0, end) + script + page.slice(end);
}

/** How to bring the webview up to date. */
export type Delivery =
    { action: 'assign'; html: string; shown: Shown } | { action: 'post'; message: PageMessage } | { action: 'none' };

/**
 * Decides, for each page, whether the webview loads it — assigning
 * `webview.html` — or has it posted to the page it already shows, and keeps
 * what that decision depends on: whether the webview's page listens, and
 * whether what `webview.html` holds is still the latest, which VS Code
 * reloads when a hidden panel is shown again.
 *
 * As VS Code's own Markdown preview does, a page is loaded the first time, and
 * whenever the panel is hidden — its page then discarded — and posted
 * otherwise.
 */
export class PageDelivery {
    /** Whether the panel was visible when last told; a new panel is. */
    private visible = true;
    /** The webview shows a page whose script said it is ready. */
    private ready = false;
    /** The webview is loading a page assigned to it. */
    private loading = false;
    /** What was last shown, and whether `webview.html` holds it. */
    private latest: Shown | undefined;
    private assigned = true;
    /** A page held back while the webview's page loads. */
    private pending: Shown | undefined;

    /** `engine`, when given, draws the diagrams of every page loaded. */
    constructor(private readonly engine?: DiagramEngine) {}

    /** How to show `shown` in a webview that is `visible`. */
    show(shown: Shown, visible: boolean): Delivery {
        this.latest = shown;
        this.visible = visible;
        if (shown.kind === 'page' && visible && this.ready) {
            this.assigned = false;
            return { action: 'post', message: pageMessage(shown) };
        }
        if (shown.kind === 'page' && visible && this.loading) {
            this.pending = shown;
            this.assigned = false;
            return { action: 'none' };
        }
        return this.assign(shown);
    }

    /** The webview's page says it listens: what was held back is posted. */
    onReady(): Delivery {
        this.ready = true;
        this.loading = false;
        const pending = this.pending;
        this.pending = undefined;
        if (!pending) return { action: 'none' };
        return { action: 'post', message: pageMessage(pending) };
    }

    /**
     * The panel's view state changed, leaving it `visible` or not. Only a
     * change of visibility matters — the event also fires when the panel
     * gains or loses the focus, which leaves its page as it was. Hidden, the
     * page is discarded; shown again, VS Code reloads `webview.html`, so the
     * latest page is loaded instead when that is older.
     */
    onViewState(visible: boolean): Delivery {
        if (visible === this.visible) return { action: 'none' };
        this.visible = visible;
        this.ready = false;
        this.pending = undefined;
        this.loading = visible && this.latest?.kind === 'page';
        if (!visible || this.assigned || !this.latest) return { action: 'none' };
        // Shown again, the page keeps the reader's place, not the link's anchor.
        return this.assign({ ...this.latest, fragment: undefined });
    }

    private assign(shown: Shown): Delivery {
        this.assigned = true;
        this.pending = undefined;
        this.ready = false;
        this.loading = shown.kind === 'page';
        const html =
            shown.kind === 'page'
                ? withPageUpdater(shown.html, shown.document, shown.fragment, this.engine)
                : shown.html;
        return { action: 'assign', html, shown };
    }
}

/** The message posting `shown`, a page, to the webview. */
function pageMessage(shown: Shown): PageMessage {
    const message: PageMessage = { type: 'page', html: shown.html, document: shown.document };
    if (shown.fragment !== undefined) message.fragment = shown.fragment;
    return message;
}

/**
 * The folders a page's files may be loaded from: the workspace's, and the
 * extension's directory holding the diagram engine.
 */
function resourceRoots(extensionUri: vscode.Uri): vscode.Uri[] {
    return [...(vscode.workspace.workspaceFolders ?? []).map((folder) => folder.uri), engineDirectory(extensionUri)];
}

/** What the panel asks of the language client: pages, and where their links lead. */
export type PanelClient = PreviewClient & PreviewLinkClient;

/** Whether `document` is one the preview can show. */
function isRestructuredText(document: vscode.TextDocument): boolean {
    return document.languageId === 'restructuredtext';
}

/**
 * The one preview panel, showing the reStructuredText document last active
 * in an editor — still while the panel itself has the focus.
 */
export class PreviewPanel {
    public static currentPanel: PreviewPanel | undefined;
    private readonly disposables: vscode.Disposable[] = [];
    private readonly requests = new LatestRequest();
    private readonly delivery: PageDelivery;
    private document: vscode.Uri | undefined;
    /** The editor column the shown document was last active in. */
    private sourceColumn: vscode.ViewColumn | undefined;
    private debounceTimer: NodeJS.Timeout | undefined;
    // Set once the panel is closed. A render already running when the user
    // closes it finishes later, and must not write into the disposed webview.
    private disposed = false;
    /** The page the webview shows, as the server rendered it. */
    private displayed: string | undefined;
    /** A page assigned to the webview, shown once its script is ready. */
    private loadingPage: string | undefined;
    /** The page last posted, shown once the webview says so. */
    private postedPage: string | undefined;
    /** What the webview last reported of the diagrams it drew. */
    private drawnDiagrams: DiagramsMessage | undefined;

    /**
     * Reveals the panel, creating it if there is none — beside the editor,
     * which keeps the focus, as Markdown's preview to the side opens, so the
     * page can be watched while typing; `client` gives the running language
     * client, if any, and `extensionUri` the extension, whose diagram engine
     * the page loads.
     */
    public static createOrShow(client: () => PanelClient | undefined, extensionUri: vscode.Uri): void {
        const placement = { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true };
        if (PreviewPanel.currentPanel) {
            PreviewPanel.currentPanel.panel.reveal(placement.viewColumn, placement.preserveFocus);
            return;
        }
        const panel = vscode.window.createWebviewPanel('rinxPreview', 'Rinx Preview', placement, {
            enableScripts: true,
            localResourceRoots: resourceRoots(extensionUri),
        });
        PreviewPanel.currentPanel = new PreviewPanel(panel, client, extensionUri);
    }

    private constructor(
        private readonly panel: vscode.WebviewPanel,
        private readonly client: () => PanelClient | undefined,
        private readonly extensionUri: vscode.Uri,
    ) {
        this.delivery = new PageDelivery(diagramEngine(panel.webview, extensionUri));
        this.followEditor(vscode.window.activeTextEditor);
        this.panel.onDidDispose(() => this.dispose(), null, this.disposables);
        this.panel.webview.onDidReceiveMessage(
            (message: { type?: string; href?: unknown }) => {
                if (message.type === 'link' && typeof message.href === 'string') {
                    void this.followLink(message.href);
                } else if (isDiagramsMessage(message)) {
                    this.drawnDiagrams = message;
                } else if (message.type === 'ready') {
                    this.displayed = this.loadingPage ?? this.displayed;
                    this.loadingPage = undefined;
                    this.deliver(this.delivery.onReady());
                } else if (message.type === 'shown') {
                    this.displayed = this.postedPage;
                }
            },
            null,
            this.disposables,
        );
        this.panel.onDidChangeViewState(
            ({ webviewPanel }) => {
                this.deliver(this.delivery.onViewState(webviewPanel.visible));
            },
            null,
            this.disposables,
        );
        // Any document may change the page — a title in the sidebar, a
        // label's text — so an edit or save of any of them asks again.
        vscode.workspace.onDidChangeTextDocument(
            (event) => {
                const onType = vscode.workspace.getConfiguration('rinx').get('previewMode') === 'onType';
                if (onType && isRestructuredText(event.document)) this.scheduleUpdate();
            },
            null,
            this.disposables,
        );
        vscode.workspace.onDidSaveTextDocument(
            (document) => {
                if (isRestructuredText(document)) this.scheduleUpdate();
            },
            null,
            this.disposables,
        );
        vscode.window.onDidChangeActiveTextEditor((editor) => this.followEditor(editor), null, this.disposables);
        vscode.workspace.onDidChangeWorkspaceFolders(
            () => {
                this.panel.webview.options = {
                    ...this.panel.webview.options,
                    localResourceRoots: resourceRoots(this.extensionUri),
                };
                this.scheduleUpdate();
            },
            null,
            this.disposables,
        );
        void this.update();
    }

    /**
     * The page the webview shows, as the server rendered it — only once the
     * webview says it shows it, so the end-to-end tests see what a reader does.
     */
    public get html(): string | undefined {
        return this.displayed;
    }

    /** What the webview last reported of the diagrams it drew, if anything. */
    public get diagrams(): DiagramsMessage | undefined {
        return this.drawnDiagrams;
    }

    /**
     * Follows `href`, a link of the page shown: opens a document beside the
     * panel and shows its page at the link's anchor, opens a downloaded file,
     * scrolls the page, or opens the browser; says so when the link leads to
     * no source.
     */
    public async followLink(href: string): Promise<void> {
        const document = this.document;
        if (!document) return;
        const action = await resolveLink(this.client(), document, href);
        if (this.disposed) return;
        const placement = { viewColumn: this.sourceColumn ?? vscode.ViewColumn.One, preserveFocus: true };
        switch (action.kind) {
            case 'external':
                await vscode.env.openExternal(vscode.Uri.parse(action.uri));
                break;
            case 'scroll': {
                const message: ScrollMessage = { type: 'scroll', fragment: action.fragment };
                void this.panel.webview.postMessage(message);
                break;
            }
            case 'open': {
                const target = vscode.Uri.parse(action.uri);
                await vscode.window.showTextDocument(target, placement);
                if (this.disposed) return;
                this.document = target;
                clearTimeout(this.debounceTimer);
                await this.update(action.fragment);
                break;
            }
            case 'file':
                await vscode.commands.executeCommand('vscode.open', vscode.Uri.parse(action.uri), placement);
                break;
            case 'none':
                vscode.window.setStatusBarMessage(`Rinx: ${href} leads to no source to open`, 5000);
                break;
        }
    }

    /** Asks for the page again — when the language server has started. */
    public refresh(): void {
        void this.update();
    }

    public dispose(): void {
        this.disposed = true;
        clearTimeout(this.debounceTimer);
        PreviewPanel.currentPanel = undefined;
        this.panel.dispose();
        for (const disposable of this.disposables.splice(0)) disposable.dispose();
    }

    /** Shows the document of `editor`, if it is one the preview can show. */
    private followEditor(editor: vscode.TextEditor | undefined): void {
        if (!editor || !isRestructuredText(editor.document)) return;
        this.document = editor.document.uri;
        this.sourceColumn = editor.viewColumn ?? this.sourceColumn;
        this.scheduleUpdate();
    }

    private scheduleUpdate(): void {
        clearTimeout(this.debounceTimer);
        this.debounceTimer = setTimeout(() => void this.update(), PREVIEW_DELAY_MS);
    }

    /** Asks for the page of the document shown, at the anchor `fragment` when given. */
    private async update(fragment?: string): Promise<void> {
        const document = this.document;
        if (!document) return;
        const request = this.requests.next();
        const resourceRoot = this.panel.webview.asWebviewUri(vscode.Uri.file('/')).toString();
        const shown = await renderPage(this.client(), document, resourceRoot);
        if (fragment !== undefined && shown.kind === 'page') shown.fragment = fragment;
        if (this.disposed || !this.requests.isLatest(request)) return;
        this.deliver(this.delivery.show(shown, this.panel.visible));
    }

    private deliver(delivery: Delivery): void {
        if (this.disposed) return;
        if (delivery.action === 'assign') {
            this.panel.webview.html = delivery.html;
            // A message page has no script to say it is shown.
            if (delivery.shown.kind === 'message') this.displayed = delivery.shown.html;
            else this.loadingPage = delivery.shown.html;
        } else if (delivery.action === 'post') {
            this.postedPage = delivery.message.html;
            void this.panel.webview.postMessage(delivery.message);
        }
    }
}
