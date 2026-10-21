// Drawing the preview's diagrams in the webview, with PlantUML's own engine
// compiled to JavaScript (`@plantuml/core`, which scripts/copy-plantuml.mjs
// copies into dist/plantuml/). The language server compiles nothing
// (ADR-042): it puts each diagram's PlantUML text in a
// `<pre data-rinx-plantuml>` where the picture would be, and the drawer the
// page is loaded with replaces it with the picture — or, when PlantUML refuses
// the text, shows PlantUML's message above it.
//
// The engine lays a diagram out with Graphviz, where the build uses ELK, so a
// picture may be arranged differently from the published one.

import * as vscode from 'vscode';

/** Where the engine's files lie in the extension. */
export const ENGINE_DIRECTORY = ['dist', 'plantuml'] as const;

/**
 * The classic scripts loaded ahead of the engine: Graphviz, which it requires
 * as a global, then the themes and icons it looks up as globals.
 */
export const ENGINE_GLOBALS = ['viz-global.js', 'themes.js', 'openiconic.js'] as const;

/** The engine itself, an ES module. */
export const ENGINE_MODULE = 'plantuml.js';

/** The webview addresses of the engine's scripts. */
export interface DiagramEngine {
    globals: string[];
    module: string;
}

/** The directory of the extension at `extensionUri` that holds the engine. */
export function engineDirectory(extensionUri: vscode.Uri): vscode.Uri {
    return vscode.Uri.joinPath(extensionUri, ...ENGINE_DIRECTORY);
}

/** The engine's scripts as `webview` addresses them. */
export function diagramEngine(webview: Pick<vscode.Webview, 'asWebviewUri'>, extensionUri: vscode.Uri): DiagramEngine {
    const address = (file: string) =>
        webview.asWebviewUri(vscode.Uri.joinPath(engineDirectory(extensionUri), file)).toString();
    return { globals: ENGINE_GLOBALS.map(address), module: address(ENGINE_MODULE) };
}

/** `value` as an HTML attribute's value, quoted. */
function attribute(value: string): string {
    return `"${value.replace(/&/g, '&amp;').replace(/"/g, '&quot;')}"`;
}

/**
 * The tags loading `engine`: the globals first, then the engine, which hands
 * its `renderToString` to the drawer and asks it to draw what the page holds.
 */
export function engineScripts(engine: DiagramEngine): string {
    const globals = engine.globals.map((src) => `<script src=${attribute(src)}></script>`).join('');
    const module =
        `import { renderToString } from ${JSON.stringify(engine.module).replace(/</g, '\\u003c')};` +
        'window.rinxRenderPlantuml = renderToString;' +
        'if (window.rinxDrawDiagrams) window.rinxDrawDiagrams();';
    return `${globals}<script type="module">${module}</script>`;
}

/** What the webview reports once every diagram of its page is drawn. */
export interface DiagramsMessage {
    type: 'diagrams';
    /** How many pictures the page shows. */
    drawn: number;
    /** PlantUML's message for each diagram it refused. */
    failed: string[];
}

/** Whether `message`, posted by the webview, is a {@link DiagramsMessage}. */
export function isDiagramsMessage(message: unknown): message is DiagramsMessage {
    if (typeof message !== 'object' || message === null) return false;
    if (!('type' in message && 'drawn' in message && 'failed' in message)) return false;
    const { type, drawn, failed } = message;
    return (
        type === 'diagrams' &&
        typeof drawn === 'number' &&
        Array.isArray(failed) &&
        failed.every((entry) => typeof entry === 'string')
    );
}

/** PlantUML's `renderToString`: the SVG for `lines`, or why there is none. */
export type RenderPlantuml = (
    lines: string[],
    onSuccess: (svg: string) => void,
    onError: (error: string) => void,
) => void;

/** The part of a page's element the drawer touches. */
export interface DrawnElement {
    className: string;
    textContent: string | null;
    innerHTML: string;
    getAttribute(name: string): string | null;
    setAttribute(name: string, value: string): void;
    removeAttribute(name: string): void;
    replaceWith(element: DrawnElement): void;
    append(...elements: DrawnElement[]): void;
}

/** The part of a page's document the drawer touches. */
export interface DrawnDocument {
    querySelectorAll(selector: string): Iterable<DrawnElement>;
    querySelector(selector: string): unknown;
    createElement(tag: string): DrawnElement;
}

/** The part of a page's window the drawer touches: the engine, once loaded. */
export interface DrawnWindow {
    rinxRenderPlantuml?: RenderPlantuml;
}

/**
 * The function that draws the diagrams of the page `document` shows, with
 * the engine `window` holds once loaded, posting a {@link DiagramsMessage}
 * through `post` once nothing is left to draw. The page updater calls it after
 * each page is loaded or swapped in, and the engine once it has loaded.
 *
 * Each text is drawn once and the result kept, so a page swapped in on every
 * pause in typing redraws nothing that did not change — and one at a time,
 * since the engine's drawing is asynchronous. A result arriving after a newer
 * page was swapped in fills the diagrams of that text still on the page.
 *
 * It runs in the webview, loaded as its own source ({@link DIAGRAM_DRAWER}),
 * so it names nothing outside itself.
 */
export function diagramDrawer(
    post: (message: DiagramsMessage) => void,
    window: DrawnWindow,
    document: DrawnDocument,
): () => void {
    const results = new Map<string, { svg: string } | { error: string }>();
    let drawing: string | null = null;
    const place = (pre: DrawnElement, result: { svg: string } | { error: string }) => {
        if ('svg' in result) {
            const picture = document.createElement('div');
            picture.className = 'plantuml-picture';
            const style = pre.getAttribute('style');
            if (style) picture.setAttribute('style', style);
            picture.innerHTML = result.svg;
            pre.replaceWith(picture);
            return;
        }
        const failure = document.createElement('div');
        failure.className = 'directive-error plantuml-failure';
        const message = document.createElement('p');
        message.className = 'directive-error-message';
        message.textContent = `PlantUML: ${result.error}`;
        pre.removeAttribute('data-rinx-plantuml');
        pre.replaceWith(failure);
        failure.append(message, pre);
    };
    const draw = (): void => {
        const render = window.rinxRenderPlantuml;
        if (!render) return;
        for (const pre of document.querySelectorAll('pre[data-rinx-plantuml]')) {
            const text = pre.textContent ?? '';
            const result = results.get(text);
            if (result) {
                place(pre, result);
            } else if (drawing === null) {
                drawing = text;
                const settle = (outcome: { svg: string } | { error: string }) => {
                    if (results.size >= 256) results.clear();
                    results.set(text, outcome);
                    drawing = null;
                    draw();
                };
                try {
                    render(
                        text.split('\n'),
                        (svg) => settle({ svg }),
                        (error) => settle({ error }),
                    );
                } catch (error) {
                    settle({ error: String(error) });
                }
            }
        }
        if (drawing !== null || !document.querySelector('.plantuml-diagram')) return;
        post({
            type: 'diagrams',
            drawn: [...document.querySelectorAll('.plantuml-picture')].length,
            failed: [...document.querySelectorAll('.plantuml-failure > .directive-error-message')].map((message) =>
                (message.textContent ?? '').replace(/^PlantUML: /, ''),
            ),
        });
    };
    return draw;
}

/**
 * {@link diagramDrawer}'s source, which the page updater calls with the
 * webview's `window` and `document`.
 */
export const DIAGRAM_DRAWER = `(${diagramDrawer.toString()})`;
