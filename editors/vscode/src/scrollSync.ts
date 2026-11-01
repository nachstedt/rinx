// Scrolling the preview and the editor together, as VS Code's Markdown
// preview does (roadmap #34, ADR-048): the page asked for with `sourceLines`
// names on each block the 0-based line it was written on
// (`data-source-line`), and a literal block its last line as well
// (`data-source-end-line`); this module maps between those lines and the
// page's vertical positions, interpolating between two marked blocks, or
// through the lines of a literal block.
//
// Four behaviours, each a setting as Markdown's are, all on by default:
// the editor's scrolling scrolls the page, the page's scrolling scrolls the
// editor, the line holding the cursor is marked on the page, and a
// double-click on the page moves the cursor to the line it was written on.
// The cursor itself does not scroll the page — it is only marked.

/** The class marking the block that holds the editor's cursor. */
export const ACTIVE_LINE_CLASS = 'code-active-line';

/** The part of a page's element the scroll sync reads. */
export interface LinedElement {
    getAttribute(name: string): string | null;
    getBoundingClientRect(): { top: number; height: number };
    classList: { add(token: string): void; remove(token: string): void };
}

/** The part of a page's document the scroll sync reads. */
export interface LinedDocument {
    querySelectorAll(selector: string): Iterable<LinedElement>;
}

/** The part of a page's window the scroll sync reads and scrolls. */
export interface LinedWindow {
    scrollX: number;
    scrollY: number;
    scrollTo(x: number, y: number): void;
}

/** What the page updater asks of the scroll sync. */
export interface ScrollSync {
    /** Scrolls the page so the source line `line`, fractional, is at its top. */
    revealLine(line: number): void;
    /** The source line, fractional, written at the page's vertical position `offset`, or `null` with no block marked. */
    lineAtOffset(offset: number): number | null;
    /** Marks the block holding the source line `line`, or none for `null`; says which line it marked. */
    markLine(line: number | null): number | null;
}

/**
 * The scroll sync of the page `document` shows in `window`, reading the
 * lines its blocks name afresh on every call, so a page swapped in needs no
 * new one.
 *
 * It runs in the webview, loaded as its own source ({@link SCROLL_SYNC}), so
 * it names nothing outside itself.
 */
export function scrollSync(window: LinedWindow, document: LinedDocument): ScrollSync {
    interface Block {
        element: LinedElement;
        line: number;
        /** The line after the block's last, for one whose lines show one to one. */
        after?: number;
        /** Its top, from the page's top. */
        top: number;
        height: number;
    }
    // The visible blocks in page order, keeping a line once it is reached
    // so that the lines only grow: a block a container draws ahead of where
    // it was written is passed over.
    const blocks = (): Block[] => {
        const found: Block[] = [];
        let reached = -1;
        for (const element of document.querySelectorAll('[data-source-line]')) {
            const line = Number(element.getAttribute('data-source-line'));
            if (!Number.isInteger(line) || line < reached) continue;
            const { top, height } = element.getBoundingClientRect();
            // An element in a closed dropdown, or one drawing nothing.
            if (height <= 0) continue;
            const end = Number(element.getAttribute('data-source-end-line') ?? Number.NaN);
            const block: Block = { element, line, top: top + window.scrollY, height };
            if (Number.isInteger(end) && end > line) block.after = end + 1;
            found.push(block);
            reached = line;
        }
        return found;
    };
    // The last block starting at or before `line`, and the first after it.
    const around = (line: number): { previous?: Block; next?: Block } => {
        let previous: Block | undefined;
        for (const block of blocks()) {
            if (block.line > line) return { previous, next: block };
            if (previous === undefined || block.line > previous.line) previous = block;
        }
        return { previous };
    };
    // The page's vertical position of the source line `line`, fractional:
    // through a literal block's lines, between two blocks' tops, or into
    // the last block; above the first block, the page's top.
    const offsetOfLine = (line: number): number => {
        const { previous, next } = around(Math.floor(line));
        if (previous === undefined) return 0;
        if (previous.after !== undefined && line < previous.after) {
            return previous.top + (previous.height * (line - previous.line)) / (previous.after - previous.line);
        }
        if (next !== undefined) {
            return previous.top + ((next.top - previous.top) * (line - previous.line)) / (next.line - previous.line);
        }
        return previous.top + previous.height * Math.min(1, line - previous.line);
    };
    let marked: LinedElement | undefined;
    return {
        revealLine(line) {
            window.scrollTo(window.scrollX, Math.max(0, offsetOfLine(line)));
        },
        lineAtOffset(offset) {
            let previous: Block | undefined;
            let next: Block | undefined;
            for (const block of blocks()) {
                if (block.top > offset) {
                    next = block;
                    break;
                }
                previous = block;
            }
            if (previous === undefined) return next === undefined ? null : 0;
            const into = offset - previous.top;
            if (previous.after !== undefined && into < previous.height) {
                return previous.line + ((previous.after - previous.line) * into) / previous.height;
            }
            if (next !== undefined) {
                return previous.line + ((next.line - previous.line) * into) / (next.top - previous.top);
            }
            return previous.line + Math.min(1, into / previous.height);
        },
        markLine(line) {
            marked?.classList.remove('code-active-line');
            marked = line === null ? undefined : around(line).previous?.element;
            marked?.classList.add('code-active-line');
            return marked === undefined ? null : Number(marked.getAttribute('data-source-line'));
        },
    };
}

/** {@link scrollSync}'s source, which the page updater calls with the webview's `window` and `document`. */
export const SCROLL_SYNC = `(${scrollSync.toString()})`;

/**
 * `run`, called at most once per `ms` with the latest value it was given:
 * at once when idle, and once more at the end of a busy period with the last
 * value, so the final position always arrives. Runs in the webview, as
 * {@link THROTTLED}.
 */
export function throttled<T>(
    run: (value: T) => void,
    ms: number,
    schedule: (next: () => void, ms: number) => unknown = setTimeout,
): (value: T) => void {
    let waiting = false;
    let pending: { value: T } | undefined;
    const flush = (): void => {
        if (pending === undefined) {
            waiting = false;
            return;
        }
        const { value } = pending;
        pending = undefined;
        run(value);
        schedule(flush, ms);
    };
    return (value) => {
        pending = { value };
        if (!waiting) {
            waiting = true;
            flush();
        }
    };
}

/** {@link throttled}'s source. */
export const THROTTLED = `(${throttled.toString()})`;

/** The marker of the line holding the cursor: a bar left of its block, as Markdown's preview draws it. */
export const ACTIVE_LINE_STYLE = `.${ACTIVE_LINE_CLASS} { position: relative; }
.${ACTIVE_LINE_CLASS}::before {
    content: "";
    position: absolute;
    top: 0;
    left: -12px;
    height: 100%;
    border-left: 3px solid rgba(128, 128, 128, 0.5);
}`;

/** The part of an editor the visible line is read from. */
export interface ScrolledEditor {
    visibleRanges: readonly { start: { line: number; character: number } }[];
    document: { lineAt(line: number): { text: string } };
}

/**
 * The line, fractional, at the top of `editor`, as Markdown's preview reads
 * it: the first visible line, and how far into it the view starts.
 */
export function visibleLine(editor: ScrolledEditor): number | undefined {
    const start = editor.visibleRanges[0]?.start;
    if (start === undefined) return undefined;
    const text = editor.document.lineAt(start.line).text;
    return start.line + start.character / (text.length + 2);
}

/**
 * Where an editor of `document` is scrolled to bring the line `line`,
 * fractional, to its top: that line, from as far into it as the fraction
 * says, within the document's lines.
 */
export function revealedPosition(
    document: { lineCount: number; lineAt(line: number): { text: string } },
    line: number,
): { line: number; character: number } {
    const whole = Math.min(Math.max(0, Math.floor(line)), document.lineCount - 1);
    const text = document.lineAt(whole).text;
    return { line: whole, character: Math.floor((line - Math.floor(line)) * text.length) };
}

/** The settings the four behaviours are switched by, under `rinx.preview`. */
export type SyncSetting =
    'scrollPreviewWithEditor' | 'scrollEditorWithPreview' | 'markEditorSelection' | 'doubleClickToSwitchToEditor';

/** The part of the `rinx` configuration a setting is read from. */
export interface SyncConfiguration {
    get(section: string, defaultValue: boolean): boolean;
}

/** Whether `setting` is on, read from `configuration` when asked, so changing it needs no reload. */
export function syncEnabled(configuration: SyncConfiguration, setting: SyncSetting): boolean {
    return configuration.get(`preview.${setting}`, true);
}
