import * as assert from 'assert';
import * as vscode from 'vscode';
import {
    escapeHtml,
    LatestRequest,
    messagePage,
    NO_SERVER_PAGE,
    PREVIEW_METHOD,
    PreviewClient,
    PreviewParams,
    PageDelivery,
    renderPage,
    Shown,
    withPageUpdater,
} from '../../preview';

suite('Preview Test Suite', () => {
    test('escapeHtml: escapes what HTML gives a meaning', () => {
        assert.strictEqual(escapeHtml('<a href="x">&</a>'), '&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;');
    });

    test('messagePage: shows the heading and the detail, escaped', () => {
        // When
        const page = messagePage('No page', 'cannot read `<page.html>`');

        // Then
        assert.ok(page.includes('<h1>No page</h1>'), page);
        assert.ok(page.includes('<p>cannot read `&lt;page.html&gt;`</p>'), page);
    });

    test('LatestRequest: only the request sent last is the latest', () => {
        // Given
        const requests = new LatestRequest();
        const first = requests.next();

        // When
        const second = requests.next();

        // Then
        assert.ok(!requests.isLatest(first));
        assert.ok(requests.isLatest(second));
    });

    test('renderPage: asks the server for the document and shows its page', async () => {
        // Given
        const asked: [string, PreviewParams][] = [];
        const client: PreviewClient = {
            sendRequest: async (method, params) => {
                asked.push([method, params]);
                return { html: '<p>page</p>' };
            },
        };
        const document = vscode.Uri.file('/docs/index.rst');

        // When
        const page = await renderPage(client, document);

        // Then
        assert.deepStrictEqual(page, { kind: 'page', html: '<p>page</p>', document: document.toString() });
        assert.deepStrictEqual(asked, [[PREVIEW_METHOD, { textDocument: { uri: document.toString() } }]]);
    });

    test('renderPage: says why the server rendered no page', async () => {
        // Given
        const client: PreviewClient = {
            sendRequest: async () => {
                throw new Error('cannot read `page.html`');
            },
        };

        // When
        const page = await renderPage(client, vscode.Uri.file('/docs/index.rst'));

        // Then
        assert.strictEqual(page.kind, 'message');
        assert.ok(page.html.includes('The page could not be rendered'), page.html);
        assert.ok(page.html.includes('cannot read `page.html`'), page.html);
    });

    test('renderPage: says no server runs when there is none', async () => {
        // When / Then
        const shown = await renderPage(undefined, vscode.Uri.file('/docs/index.rst'));
        assert.deepStrictEqual([shown.kind, shown.html], ['message', NO_SERVER_PAGE]);
    });

    test('withPageUpdater: places the script last in the body, naming the document', () => {
        // When
        const html = withPageUpdater('<html><body><p>x</p></body></html>', 'file:///docs/a.rst');

        // Then
        assert.ok(html.startsWith('<html><body><p>x</p><script>'), html);
        assert.ok(html.endsWith('</script></body></html>'), html);
        assert.ok(html.includes('let current = "file:///docs/a.rst";'), html);
    });

    test('withPageUpdater: no document name can end the script early', () => {
        // When
        const html = withPageUpdater('<p>x</p>', 'file:///docs/</script>.rst');

        // Then — appended without a body, and the name's `<` escaped
        assert.strictEqual(html.match(/<\/script>/g)?.length, 1, html);
        assert.ok(html.includes('"file:///docs/\\u003c/script>.rst"'), html);
    });
});

/** A page of `document` holding `html`. */
function pageOf(html: string, document = 'file:///docs/a.rst'): Shown {
    return { kind: 'page', html, document };
}

suite('PageDelivery Test Suite', () => {
    test('the first page is loaded, with the script', () => {
        // When
        const delivery = new PageDelivery().show(pageOf('<body>one</body>'), true);

        // Then
        assert.strictEqual(delivery.action, 'assign');
        assert.ok(delivery.action === 'assign' && delivery.html.includes('acquireVsCodeApi'));
    });

    test('a later page is posted once the loaded one listens', () => {
        // Given
        const delivery = new PageDelivery();
        delivery.show(pageOf('one'), true);
        delivery.onReady();

        // When / Then
        assert.deepStrictEqual(delivery.show(pageOf('two'), true), {
            action: 'post',
            message: { type: 'page', html: 'two', document: 'file:///docs/a.rst' },
        });
    });

    test('a page arriving while the loaded one starts is posted when it listens', () => {
        // Given
        const delivery = new PageDelivery();
        delivery.show(pageOf('one'), true);

        // When
        const held = delivery.show(pageOf('two'), true);
        const ready = delivery.onReady();

        // Then
        assert.deepStrictEqual(held, { action: 'none' });
        assert.deepStrictEqual(ready, {
            action: 'post',
            message: { type: 'page', html: 'two', document: 'file:///docs/a.rst' },
        });
    });

    test('a message page is loaded without the script, and the next page is loaded again', () => {
        // Given
        const delivery = new PageDelivery();
        delivery.show(pageOf('one'), true);
        delivery.onReady();

        // When
        const message = delivery.show({ kind: 'message', html: '<h1>No</h1>', document: 'file:///docs/a.rst' }, true);
        const next = delivery.show(pageOf('two'), true);

        // Then
        assert.deepStrictEqual(message, {
            action: 'assign',
            html: '<h1>No</h1>',
            shown: { kind: 'message', html: '<h1>No</h1>', document: 'file:///docs/a.rst' },
        });
        assert.strictEqual(next.action, 'assign');
    });

    test('the panel gaining or losing the focus changes nothing', () => {
        // Given — a loaded page, listening
        const delivery = new PageDelivery();
        delivery.show(pageOf('one'), true);
        delivery.onReady();

        // When — the focus moves to the editor and back, the panel visible throughout
        const focusChanges = [delivery.onViewState(true), delivery.onViewState(true)];

        // Then — nothing reloads, and the next page is still posted
        assert.deepStrictEqual(focusChanges, [{ action: 'none' }, { action: 'none' }]);
        assert.strictEqual(delivery.show(pageOf('two'), true).action, 'post');
    });

    test('a page posted before the panel was hidden is loaded when it is shown again', () => {
        // Given — `webview.html` still holds the first page
        const delivery = new PageDelivery();
        delivery.show(pageOf('one'), true);
        delivery.onReady();
        delivery.show(pageOf('two'), true);

        // When
        delivery.onViewState(false);
        const shown = delivery.onViewState(true);

        // Then
        assert.ok(shown.action === 'assign' && shown.html.startsWith('two'), JSON.stringify(shown));
    });

    test('a hidden panel is assigned its page, and shown again needs nothing more', () => {
        // Given
        const delivery = new PageDelivery();
        delivery.show(pageOf('one'), true);
        delivery.onReady();
        delivery.onViewState(false);

        // When
        const whileHidden = delivery.show(pageOf('two'), false);
        const shown = delivery.onViewState(true);

        // Then
        assert.strictEqual(whileHidden.action, 'assign');
        assert.deepStrictEqual(shown, { action: 'none' });
    });
});
