// The preview panel: shows the page the language server renders for the
// active reStructuredText document, with a `rinx/preview` request.
//
// The server holds every document's latest text and the project's current
// index, so the page is never stale: a title changed in another file shows at
// once, and no process is started per edit (see docs/vscode.rst).
//
// A page is loaded into the webview once; every later page of the same panel
// is posted to it and swapped into the open document by the script loaded
// with the first (`PAGE_UPDATER`), so the reader keeps their place. Assigning
// `webview.html` instead reloads the document, which VS Code then scrolls
// back down — visibly, under the theme's smooth scrolling.

import * as vscode from 'vscode';
import { errorMessage } from './errors';

/** The request the server answers with a document's page. */
export const PREVIEW_METHOD = 'rinx/preview';

/** What a `rinx/preview` request asks for. */
export interface PreviewParams {
    textDocument: { uri: string };
}

/** The server's answer to a `rinx/preview` request. */
export interface PreviewResult {
    html: string;
}

/** What the panel needs of the language client. */
export interface PreviewClient {
    sendRequest(method: string, params: PreviewParams): Promise<PreviewResult>;
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
}

/**
 * The page for `document`, asked of `client`, or a page saying why there is
 * none.
 */
export async function renderPage(client: PreviewClient | undefined, document: vscode.Uri): Promise<Shown> {
    const uri = document.toString();
    if (!client) return { kind: 'message', html: NO_SERVER_PAGE, document: uri };
    try {
        const result = await client.sendRequest(PREVIEW_METHOD, { textDocument: { uri } });
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
}

/**
 * The script a page is loaded with. It swaps each posted page into the open
 * document, keeping the scroll position while the document is the same one,
 * and remembers the position in the webview's state, so a reload — the panel
 * hidden and shown again — restores it at once rather than scrolling there.
 * It says `ready` once it listens, and `shown` after each swap.
 */
const PAGE_UPDATER = `(() => {
    const vscode = acquireVsCodeApi();
    let current = DOCUMENT;
    // A jump, not the theme's smooth scroll, whenever the position is restored.
    document.documentElement.style.scrollBehavior = 'auto';
    const saved = vscode.getState();
    if (saved && saved.document === current) window.scrollTo(0, saved.y);
    const remember = () => vscode.setState({ document: current, y: window.scrollY });
    window.addEventListener('scroll', remember, { passive: true });
    window.addEventListener('message', (event) => {
        const message = event.data;
        if (!message || message.type !== 'page') return;
        const y = message.document === current ? window.scrollY : 0;
        const next = new DOMParser().parseFromString(message.html, 'text/html');
        // An unchanged head is kept, so the stylesheet is not applied again.
        if (next.head.innerHTML !== document.head.innerHTML) {
            document.head.replaceWith(document.adoptNode(next.head));
        }
        document.body.replaceWith(document.adoptNode(next.body));
        current = message.document;
        window.scrollTo(0, y);
        remember();
        vscode.postMessage({ type: 'shown' });
    });
    vscode.postMessage({ type: 'ready' });
})();`;

/**
 * `page`, the page of `document`, with the script that swaps later pages into
 * it, placed last in its body.
 */
export function withPageUpdater(page: string, document: string): string {
    // `<` escaped, so no URI can end the script early.
    const script = `<script>${PAGE_UPDATER.replace('DOCUMENT', () => JSON.stringify(document).replace(/</g, '\\u003c'))}</script>`;
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

    /** How to show `shown` in a webview that is `visible`. */
    show(shown: Shown, visible: boolean): Delivery {
        this.latest = shown;
        this.visible = visible;
        if (shown.kind === 'page' && visible && this.ready) {
            this.assigned = false;
            return { action: 'post', message: { type: 'page', html: shown.html, document: shown.document } };
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
        return { action: 'post', message: { type: 'page', html: pending.html, document: pending.document } };
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
        return this.assign(this.latest);
    }

    private assign(shown: Shown): Delivery {
        this.assigned = true;
        this.pending = undefined;
        this.ready = false;
        this.loading = shown.kind === 'page';
        const html = shown.kind === 'page' ? withPageUpdater(shown.html, shown.document) : shown.html;
        return { action: 'assign', html, shown };
    }
}

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
    private readonly delivery = new PageDelivery();
    private document: vscode.Uri | undefined;
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

    /**
     * Reveals the panel, creating it if there is none — beside the editor,
     * which keeps the focus, as Markdown's preview to the side opens, so the
     * page can be watched while typing; `client` gives the running language
     * client, if any.
     */
    public static createOrShow(client: () => PreviewClient | undefined): void {
        const placement = { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true };
        if (PreviewPanel.currentPanel) {
            PreviewPanel.currentPanel.panel.reveal(placement.viewColumn, placement.preserveFocus);
            return;
        }
        const panel = vscode.window.createWebviewPanel('rinxPreview', 'Rinx Preview', placement, {
            enableScripts: true,
        });
        PreviewPanel.currentPanel = new PreviewPanel(panel, client);
    }

    private constructor(
        private readonly panel: vscode.WebviewPanel,
        private readonly client: () => PreviewClient | undefined,
    ) {
        this.followEditor(vscode.window.activeTextEditor);
        this.panel.onDidDispose(() => this.dispose(), null, this.disposables);
        this.panel.webview.onDidReceiveMessage(
            (message: { type?: string }) => {
                if (message.type === 'ready') {
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
        void this.update();
    }

    /**
     * The page the webview shows, as the server rendered it — only once the
     * webview says it shows it, so the end-to-end tests see what a reader does.
     */
    public get html(): string | undefined {
        return this.displayed;
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
        this.scheduleUpdate();
    }

    private scheduleUpdate(): void {
        clearTimeout(this.debounceTimer);
        this.debounceTimer = setTimeout(() => void this.update(), PREVIEW_DELAY_MS);
    }

    private async update(): Promise<void> {
        const document = this.document;
        if (!document) return;
        const request = this.requests.next();
        const shown = await renderPage(this.client(), document);
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
