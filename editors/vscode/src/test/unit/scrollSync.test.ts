import * as assert from 'assert';
import {
    ACTIVE_LINE_CLASS,
    LinedElement,
    SCROLL_SYNC,
    THROTTLED,
    revealedPosition,
    scrollSync,
    syncEnabled,
    throttled,
    visibleLine,
} from '../../scrollSync';

/** A block of the fake page: where it lies on the page, and the lines it names. */
interface Placed {
    top: number;
    height: number;
    line?: number;
    end?: number;
}

/** A window scrolled to `scrollY`, recording where it is scrolled to. */
class FakeWindow {
    scrollX = 0;
    scrollY = 0;
    scrolled: number[] = [];

    scrollTo(_x: number, y: number): void {
        this.scrollY = y;
        this.scrolled.push(y);
    }
}

/** An element of the fake page, answering for its box as the window is scrolled. */
class FakeElement implements LinedElement {
    readonly classes = new Set<string>();
    readonly classList = {
        add: (token: string) => this.classes.add(token),
        remove: (token: string) => this.classes.delete(token),
    };

    constructor(
        private readonly window: FakeWindow,
        private readonly placed: Placed,
    ) {}

    getAttribute(name: string): string | null {
        const value =
            name === 'data-source-line'
                ? this.placed.line
                : name === 'data-source-end-line'
                  ? this.placed.end
                  : undefined;
        return value === undefined ? null : String(value);
    }

    getBoundingClientRect(): { top: number; height: number } {
        return { top: this.placed.top - this.window.scrollY, height: this.placed.height };
    }
}

/** A page of `blocks`, in page order, and the scroll sync over it. */
function pageOf(blocks: Placed[]): {
    window: FakeWindow;
    elements: FakeElement[];
    sync: ReturnType<typeof scrollSync>;
} {
    const window = new FakeWindow();
    const elements = blocks.map((placed) => new FakeElement(window, placed));
    const document = {
        querySelectorAll: (selector: string) => {
            assert.strictEqual(selector, '[data-source-line]');
            return elements.filter((element) => element.getAttribute('data-source-line') !== null);
        },
    };
    return { window, elements, sync: scrollSync(window, document) };
}

/** A heading at line 0, paragraphs at lines 3 and 6, and a literal block on lines 9 to 12. */
const BLOCKS: Placed[] = [
    { top: 0, height: 40, line: 0 },
    { top: 100, height: 50, line: 3 },
    { top: 200, height: 50, line: 6 },
    { top: 300, height: 100, line: 9, end: 12 },
];

suite('Scroll Sync Test Suite', () => {
    test("revealLine: scrolls a block's line to its top", () => {
        // Given
        const { window, sync } = pageOf(BLOCKS);

        // When
        sync.revealLine(6);

        // Then
        assert.deepStrictEqual(window.scrolled, [200]);
    });

    test('revealLine: interpolates between two blocks, whatever the page is scrolled to', () => {
        // Given
        const { window, sync } = pageOf(BLOCKS);
        window.scrollY = 120;

        // When — a third of the way from line 3 to line 6
        sync.revealLine(4);

        // Then
        assert.deepStrictEqual(window.scrolled, [100 + 100 / 3]);
    });

    test("revealLine: interpolates through a literal block's lines", () => {
        // Given
        const { window, sync } = pageOf(BLOCKS);

        // When — the third of its four lines
        sync.revealLine(11);

        // Then
        assert.deepStrictEqual(window.scrolled, [350]);
    });

    test('revealLine: scrolls into the last block by the fraction of the line, and to the top above the first', () => {
        // Given
        const { window, sync } = pageOf(BLOCKS.slice(1, 3));

        // When
        sync.revealLine(6.5);
        sync.revealLine(1);

        // Then
        assert.deepStrictEqual(window.scrolled, [225, 0]);
    });

    test('revealLine: passes over a hidden block, and one drawn ahead of where it was written', () => {
        // Given — a block in a closed dropdown, and one naming an earlier line
        const { window, sync } = pageOf([
            { top: 0, height: 40, line: 0 },
            { top: 0, height: 0, line: 2 },
            { top: 100, height: 50, line: 4 },
            { top: 150, height: 50, line: 1 },
            { top: 300, height: 50, line: 8 },
        ]);

        // When — halfway from line 0 to line 4
        sync.revealLine(2);

        // Then
        assert.deepStrictEqual(window.scrolled, [50]);
    });

    test('lineAtOffset: reads a position back as the line it reveals', () => {
        // Given
        const { window, sync } = pageOf(BLOCKS);
        window.scrollY = 500;

        // When
        const lines = [0, 100 + 100 / 3, 200, 350, 380].map((offset) => sync.lineAtOffset(offset));

        // Then
        assert.deepStrictEqual(lines, [0, 4, 6, 11, 12.2]);
    });

    test('lineAtOffset: reads line 0 above the first block, and nothing on a page naming no line', () => {
        // Given
        const { sync } = pageOf(BLOCKS.slice(1));
        const { sync: unlined } = pageOf([{ top: 0, height: 40 }]);

        // When / Then
        assert.strictEqual(sync.lineAtOffset(50), 0);
        assert.strictEqual(unlined.lineAtOffset(50), null);
    });

    test('lineAtOffset: reads into the last block, and no further than its next line', () => {
        // Given
        const { sync } = pageOf(BLOCKS.slice(0, 3));

        // When / Then
        assert.strictEqual(sync.lineAtOffset(225), 6.5);
        assert.strictEqual(sync.lineAtOffset(900), 7);
    });

    test('markLine: marks the block holding the line, and only it', () => {
        // Given
        const { elements, sync } = pageOf(BLOCKS);

        // When
        const first = sync.markLine(4);
        const second = sync.markLine(10);

        // Then
        assert.deepStrictEqual([first, second], [3, 9]);
        assert.deepStrictEqual(
            elements.map((element) => element.classes.has(ACTIVE_LINE_CLASS)),
            [false, false, false, true],
        );
    });

    test('markLine: of two blocks on one line, marks the outer, written first', () => {
        // Given — a directive and the paragraph on its marker line
        const { elements, sync } = pageOf([
            { top: 0, height: 100, line: 2 },
            { top: 20, height: 40, line: 2 },
        ]);

        // When
        sync.markLine(2);

        // Then
        assert.deepStrictEqual(
            elements.map((element) => element.classes.has(ACTIVE_LINE_CLASS)),
            [true, false],
        );
    });

    test('markLine: null marks nothing, and so does a line above every block', () => {
        // Given
        const { elements, sync } = pageOf(BLOCKS.slice(1));
        sync.markLine(4);

        // When
        const cleared = sync.markLine(null);
        const above = sync.markLine(1);

        // Then
        assert.deepStrictEqual([cleared, above], [null, null]);
        assert.ok(elements.every((element) => !element.classes.has(ACTIVE_LINE_CLASS)));
    });

    test('SCROLL_SYNC and THROTTLED: are the functions, as the page updater loads them', () => {
        assert.ok(SCROLL_SYNC.startsWith('(function scrollSync('), SCROLL_SYNC);
        assert.ok(THROTTLED.startsWith('(function throttled('), THROTTLED);
    });

    test('throttled: runs at once, then once more with the last value of a busy period', () => {
        // Given
        const ran: number[] = [];
        const scheduled: (() => void)[] = [];
        const run = throttled(
            (value: number) => ran.push(value),
            50,
            (next) => scheduled.push(next),
        );

        // When
        run(1);
        run(2);
        run(3);
        scheduled.shift()?.();
        scheduled.shift()?.();
        run(4);

        // Then — 1 at once, 3 at the period's end, 4 at once again
        assert.deepStrictEqual(ran, [1, 3, 4]);
    });

    test('visibleLine: the first visible line, and how far into it the view starts', () => {
        // Given
        const editor = {
            visibleRanges: [{ start: { line: 4, character: 3 } }],
            document: { lineAt: () => ({ text: 'abcd' }) },
        };

        // When / Then
        assert.strictEqual(visibleLine(editor), 4.5);
        assert.strictEqual(visibleLine({ ...editor, visibleRanges: [] }), undefined);
    });

    test('revealedPosition: the line, from as far into it as the fraction says, within the document', () => {
        // Given
        const document = { lineCount: 10, lineAt: () => ({ text: 'abcdefgh' }) };

        // When / Then
        assert.deepStrictEqual(revealedPosition(document, 3.5), { line: 3, character: 4 });
        assert.deepStrictEqual(revealedPosition(document, 42.25), { line: 9, character: 2 });
        assert.deepStrictEqual(revealedPosition(document, -1), { line: 0, character: 0 });
    });

    test('syncEnabled: reads the setting, on unless turned off', () => {
        // Given
        const asked: string[] = [];
        const configuration = {
            get: (section: string, fallback: boolean): boolean => {
                asked.push(section);
                return section === 'preview.markEditorSelection' ? false : fallback;
            },
        };

        // When / Then
        assert.strictEqual(syncEnabled(configuration, 'markEditorSelection'), false);
        assert.strictEqual(syncEnabled(configuration, 'scrollPreviewWithEditor'), true);
        assert.deepStrictEqual(asked, ['preview.markEditorSelection', 'preview.scrollPreviewWithEditor']);
    });
});
