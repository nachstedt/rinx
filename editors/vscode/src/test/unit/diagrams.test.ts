import * as assert from 'assert';
import * as vscode from 'vscode';
import {
    DIAGRAM_DRAWER,
    DiagramsMessage,
    diagramDrawer,
    diagramEngine,
    RenderPlantuml,
    engineDirectory,
    engineScripts,
    isDiagramsMessage,
} from '../../diagrams';
import { withPageUpdater } from '../../preview';

/** An element of the fake page the drawer is run on. */
class FakeElement {
    className = '';
    textContent = '';
    innerHTML = '';
    children: FakeElement[] = [];
    private readonly attributes = new Map<string, string>();

    constructor(
        private readonly page: FakePage,
        readonly tag: string,
    ) {}

    getAttribute(name: string): string | null {
        return this.attributes.get(name) ?? null;
    }
    setAttribute(name: string, value: string): void {
        this.attributes.set(name, value);
    }
    removeAttribute(name: string): void {
        this.attributes.delete(name);
    }
    hasAttribute(name: string): boolean {
        return this.attributes.has(name);
    }
    replaceWith(element: FakeElement): void {
        this.page.replace(this, element);
    }
    append(...elements: FakeElement[]): void {
        this.children.push(...elements);
    }
}

/**
 * A page of diagrams, holding no more of the DOM than the drawer reads: its
 * elements in a flat list, answering the four selectors the drawer asks.
 */
class FakePage {
    elements: FakeElement[] = [];

    /** A page of one wrapper per text, each holding its diagram's `pre`. */
    static of(texts: string[]): FakePage {
        const page = new FakePage();
        for (const text of texts) {
            const wrapper = page.createElement('div');
            wrapper.className = 'plantuml-diagram';
            const pre = page.createElement('pre');
            pre.setAttribute('data-rinx-plantuml', '');
            pre.textContent = text;
            page.elements.push(wrapper, pre);
        }
        return page;
    }

    createElement(tag: string): FakeElement {
        return new FakeElement(this, tag);
    }
    replace(old: FakeElement, next: FakeElement): void {
        const index = this.elements.indexOf(old);
        if (index >= 0) this.elements[index] = next;
    }
    querySelectorAll(selector: string): FakeElement[] {
        switch (selector) {
            case 'pre[data-rinx-plantuml]':
                return this.elements.filter((element) => element.hasAttribute('data-rinx-plantuml'));
            case '.plantuml-picture':
                return this.withClass('plantuml-picture');
            case '.plantuml-failure > .directive-error-message':
                return this.withClass('plantuml-failure').flatMap((failure) =>
                    failure.children.filter((child) => child.className === 'directive-error-message'),
                );
            default:
                throw new Error(`the fake page answers no '${selector}'`);
        }
    }
    querySelector(selector: string): FakeElement | null {
        if (selector !== '.plantuml-diagram') throw new Error(`the fake page answers no '${selector}'`);
        return this.withClass('plantuml-diagram')[0] ?? null;
    }
    private withClass(name: string): FakeElement[] {
        return this.elements.filter((element) => element.className.split(' ').includes(name));
    }
}

/** A stand-in for PlantUML's `renderToString`. */
type Render = RenderPlantuml;

/** Runs the drawer over `page` with `render` as the engine, returning what it posted. */
function drawOn(page: FakePage, render: Render | undefined): { draw: () => void; posted: DiagramsMessage[] } {
    const posted: DiagramsMessage[] = [];
    const draw = diagramDrawer((message) => posted.push(message), { rinxRenderPlantuml: render }, page);
    return { draw, posted };
}

/** An engine drawing each text at once, counting its calls. */
function countingEngine(): { render: Render; calls: string[] } {
    const calls: string[] = [];
    const render: Render = (lines, onSuccess, onError) => {
        const text = lines.join('\n');
        calls.push(text);
        if (text === 'broken') onError('Syntax Error?');
        else onSuccess(`<svg>${text}</svg>`);
    };
    return { render, calls };
}

suite('Diagrams Test Suite', () => {
    const extension = vscode.Uri.file('/ext');
    const webview = { asWebviewUri: (uri: vscode.Uri) => uri.with({ scheme: 'https', authority: 'webview' }) };

    test('DIAGRAM_DRAWER: is the drawer, called by the page updater with the window and document', () => {
        // When
        const html = withPageUpdater('<body></body>', 'file:///a.rst');

        // Then
        assert.ok(DIAGRAM_DRAWER.startsWith('(function diagramDrawer('), DIAGRAM_DRAWER);
        assert.ok(html.includes(`${DIAGRAM_DRAWER}((message) => vscode.postMessage(message), window, document)`), html);
    });

    test('engineDirectory: the engine lies under dist/plantuml', () => {
        assert.strictEqual(engineDirectory(extension).path, '/ext/dist/plantuml');
    });

    test('diagramEngine: names each script as the webview addresses it', () => {
        // When
        const engine = diagramEngine(webview, extension);

        // Then — Graphviz before the themes and icons, then the engine
        assert.deepStrictEqual(engine, {
            globals: [
                'https://webview/ext/dist/plantuml/viz-global.js',
                'https://webview/ext/dist/plantuml/themes.js',
                'https://webview/ext/dist/plantuml/openiconic.js',
            ],
            module: 'https://webview/ext/dist/plantuml/plantuml.js',
        });
    });

    test('engineScripts: loads the globals, then the engine as a module that asks for a drawing', () => {
        // When
        const html = engineScripts({ globals: ['a.js', 'b"c.js'], module: 'https://x/plantuml.js' });

        // Then
        assert.ok(html.startsWith('<script src="a.js"></script><script src="b&quot;c.js"></script>'), html);
        assert.ok(html.includes('import { renderToString } from "https://x/plantuml.js";'), html);
        assert.ok(html.endsWith('window.rinxDrawDiagrams();</script>'), html);
    });

    test('engineScripts: no address can end the module early', () => {
        // When
        const html = engineScripts({ globals: [], module: 'https://x/</script>.js' });

        // Then
        assert.strictEqual(html.match(/<\/script>/g)?.length, 1, html);
    });

    test('withPageUpdater: loads the engine after the updater, when given one', () => {
        // When
        const html = withPageUpdater('<body></body>', 'file:///a.rst', undefined, {
            globals: ['viz.js'],
            module: 'plantuml.js',
        });

        // Then
        assert.ok(html.indexOf('acquireVsCodeApi') < html.indexOf('<script src="viz.js">'), html);
        assert.ok(html.includes('window.rinxDrawDiagrams = drawDiagrams;'), html);
    });

    test('withPageUpdater: loads no engine without one', () => {
        // When
        const html = withPageUpdater('<body></body>', 'file:///a.rst');

        // Then
        assert.ok(!html.includes('<script src='), html);
    });

    test('isDiagramsMessage: tells a report of drawn diagrams from anything else', () => {
        // When
        const answers = [
            { type: 'diagrams', drawn: 2, failed: ['x'] },
            { type: 'diagrams', drawn: 2, failed: [1] },
            { type: 'diagrams', drawn: '2', failed: [] },
            { type: 'link', href: 'x' },
            null,
            'diagrams',
        ].map(isDiagramsMessage);

        // Then
        assert.deepStrictEqual(answers, [true, false, false, false, false, false]);
    });

    test('the drawer replaces each diagram with its picture, carrying its width over', () => {
        // Given
        const page = FakePage.of(['A -> B']);
        page.elements[1].setAttribute('style', 'width: 200px');
        const { render } = countingEngine();
        const { draw, posted } = drawOn(page, render);

        // When
        draw();

        // Then
        const picture = page.elements[1];
        assert.strictEqual(picture.className, 'plantuml-picture');
        assert.strictEqual(picture.innerHTML, '<svg>A -> B</svg>');
        assert.strictEqual(picture.getAttribute('style'), 'width: 200px');
        assert.deepStrictEqual(posted.at(-1), { type: 'diagrams', drawn: 1, failed: [] });
    });

    test("the drawer shows PlantUML's message above a diagram it refuses", () => {
        // Given
        const page = FakePage.of(['broken']);
        const { render } = countingEngine();
        const { draw, posted } = drawOn(page, render);

        // When
        draw();

        // Then — the source kept below the message, no longer to be drawn
        const failure = page.elements[1];
        assert.strictEqual(failure.className, 'directive-error plantuml-failure');
        assert.strictEqual(failure.children[0].textContent, 'PlantUML: Syntax Error?');
        assert.strictEqual(failure.children[1].hasAttribute('data-rinx-plantuml'), false);
        assert.deepStrictEqual(posted.at(-1), { type: 'diagrams', drawn: 0, failed: ['Syntax Error?'] });
    });

    test('the drawer draws a text once, however often a page holds it', () => {
        // Given — the same diagram twice, drawn
        const { render, calls } = countingEngine();
        const page = FakePage.of(['A -> B', 'A -> B']);
        const { draw, posted } = drawOn(page, render);
        draw();

        // When — the next page, swapped in after a pause in typing, holds it again
        page.elements = FakePage.of(['A -> B']).elements;
        draw();

        // Then — drawn once, and the new page shows the picture
        assert.deepStrictEqual(calls, ['A -> B']);
        assert.deepStrictEqual(posted.at(-1), { type: 'diagrams', drawn: 1, failed: [] });
    });

    test('the drawer draws one text at a time', () => {
        // Given — an engine that answers only when told to
        const pending: (() => void)[] = [];
        const render: Render = (lines, onSuccess) => {
            pending.push(() => onSuccess(`<svg>${lines.join('\n')}</svg>`));
        };
        const page = FakePage.of(['A -> B', 'B -> C']);
        const { draw, posted } = drawOn(page, render);

        // When
        draw();

        // Then — the second waits for the first, and nothing is reported yet
        assert.strictEqual(pending.length, 1);
        assert.deepStrictEqual(posted, []);
        pending[0]();
        assert.strictEqual(pending.length, 2);
        pending[1]();
        assert.deepStrictEqual(posted.at(-1), { type: 'diagrams', drawn: 2, failed: [] });
    });

    test('the drawer draws nothing before the engine has loaded', () => {
        // Given
        const page = FakePage.of(['A -> B']);
        const { draw, posted } = drawOn(page, undefined);

        // When
        draw();

        // Then — the text stays, for the engine's own call once it loads
        assert.ok(page.elements[1].hasAttribute('data-rinx-plantuml'));
        assert.deepStrictEqual(posted, []);
    });

    test('the drawer reports nothing for a page without diagrams', () => {
        // Given
        const { render } = countingEngine();
        const { draw, posted } = drawOn(new FakePage(), render);

        // When
        draw();

        // Then
        assert.deepStrictEqual(posted, []);
    });
});
