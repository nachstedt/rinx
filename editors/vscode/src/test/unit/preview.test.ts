import * as assert from 'assert';
import * as vscode from 'vscode';
import {
    escapeHtml,
    isExternalHref,
    LatestRequest,
    messagePage,
    NO_SERVER_PAGE,
    PREVIEW_LINK_METHOD,
    PREVIEW_METHOD,
    PreviewClient,
    PreviewLinkClient,
    PreviewLinkParams,
    PreviewLinkResult,
    PreviewParams,
    PageDelivery,
    renderPage,
    resolveLink,
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

    test('renderPage: asks for the page under the resource root', async () => {
        // Given
        const asked: PreviewParams[] = [];
        const client: PreviewClient = {
            sendRequest: async (_method, params) => {
                asked.push(params);
                return { html: '<p>page</p>' };
            },
        };
        const document = vscode.Uri.file('/docs/index.rst');

        // When
        await renderPage(client, document, 'https://webview/');

        // Then
        assert.deepStrictEqual(asked, [
            { textDocument: { uri: document.toString() }, resourceRoot: 'https://webview/' },
        ]);
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

    test('withPageUpdater: names no anchor without one', () => {
        // When
        const html = withPageUpdater('<p>x</p>', 'file:///docs/a.rst');

        // Then
        assert.ok(html.includes('const fragment = null;'), html);
    });

    test('withPageUpdater: names the anchor the page is shown at, escaped', () => {
        // When
        const html = withPageUpdater('<p>x</p>', 'file:///docs/a.rst', '</script>install');

        // Then
        assert.strictEqual(html.match(/<\/script>/g)?.length, 1, html);
        assert.ok(html.includes('const fragment = "\\u003c/script>install";'), html);
    });

    test('isExternalHref: a web or mail address opens in the browser', () => {
        // When
        const answers = ['https://x.org/', 'HTTP://x', 'mailto:a@b.c', 'setup.html', '#top', 'data:x'].map(
            isExternalHref,
        );

        // Then
        assert.deepStrictEqual(answers, [true, true, true, false, false, false]);
    });
});

/** A client answering every `rinx/previewLink` request with `answer`, recording what it was asked. */
function linkClient(answer: PreviewLinkResult | null, asked: [string, PreviewLinkParams][] = []): PreviewLinkClient {
    return {
        sendRequest: async (method, params) => {
            asked.push([method, params]);
            return answer;
        },
    };
}

suite('resolveLink Test Suite', () => {
    const document = vscode.Uri.file('/docs/index.rst');

    test('asks the server where a link into the site leads', async () => {
        // Given
        const asked: [string, PreviewLinkParams][] = [];
        const setup = vscode.Uri.file('/docs/setup.rst').toString();
        const client = linkClient({ kind: 'document', uri: setup, fragment: 'install' }, asked);

        // When
        const action = await resolveLink(client, document, 'setup.html#install');

        // Then
        assert.deepStrictEqual(action, { kind: 'open', uri: setup, fragment: 'install' });
        assert.deepStrictEqual(asked, [
            [PREVIEW_LINK_METHOD, { textDocument: { uri: document.toString() }, href: 'setup.html#install' }],
        ]);
    });

    test('scrolls when the link leads to the page shown', async () => {
        // Given
        const client = linkClient({ kind: 'document', uri: document.toString(), fragment: 'usage' });

        // When / Then
        assert.deepStrictEqual(await resolveLink(client, document, 'index.html#usage'), {
            kind: 'scroll',
            fragment: 'usage',
        });
    });

    test('opens a downloaded file', async () => {
        // Given
        const data = vscode.Uri.file('/docs/data.csv').toString();
        const client = linkClient({ kind: 'file', uri: data });

        // When / Then
        assert.deepStrictEqual(await resolveLink(client, document, '_downloads/data.csv'), {
            kind: 'file',
            uri: data,
        });
    });

    test('does nothing for a link the server finds no source for', async () => {
        // When / Then
        assert.deepStrictEqual(await resolveLink(linkClient(null), document, 'genindex.html'), { kind: 'none' });
    });

    test('does nothing when the server cannot answer, or none runs', async () => {
        // Given
        const failing: PreviewLinkClient = {
            sendRequest: async () => {
                throw new Error('the document is not open');
            },
        };

        // When
        const actions = [
            await resolveLink(failing, document, 'setup.html'),
            await resolveLink(undefined, document, 'setup.html'),
        ];

        // Then
        assert.deepStrictEqual(actions, [{ kind: 'none' }, { kind: 'none' }]);
    });

    test('opens a web address in the browser without asking the server', async () => {
        // Given
        const asked: [string, PreviewLinkParams][] = [];

        // When
        const action = await resolveLink(linkClient(null, asked), document, 'https://docs.python.org/3/#x');

        // Then
        assert.deepStrictEqual(action, { kind: 'external', uri: 'https://docs.python.org/3/#x' });
        assert.deepStrictEqual(asked, []);
    });

    test('ignores a link with any other scheme', async () => {
        // Given
        const asked: [string, PreviewLinkParams][] = [];

        // When
        const action = await resolveLink(linkClient(null, asked), document, 'javascript:alert(1)');

        // Then
        assert.deepStrictEqual(action, { kind: 'none' });
        assert.deepStrictEqual(asked, []);
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

    test('a page shown at an anchor is posted with it', () => {
        // Given
        const delivery = new PageDelivery();
        delivery.show(pageOf('one'), true);
        delivery.onReady();

        // When / Then
        assert.deepStrictEqual(delivery.show({ ...pageOf('two'), fragment: 'install' }, true), {
            action: 'post',
            message: { type: 'page', html: 'two', document: 'file:///docs/a.rst', fragment: 'install' },
        });
    });

    test('a page shown at an anchor is loaded at it', () => {
        // When
        const delivery = new PageDelivery().show({ ...pageOf('<body>one</body>'), fragment: 'install' }, true);

        // Then
        assert.ok(
            delivery.action === 'assign' && delivery.html.includes('const fragment = "install";'),
            JSON.stringify(delivery),
        );
    });

    test("a page shown at an anchor and loaded again keeps the reader's place instead", () => {
        // Given — posted at an anchor, then the panel hidden
        const delivery = new PageDelivery();
        delivery.show(pageOf('one'), true);
        delivery.onReady();
        delivery.show({ ...pageOf('two'), fragment: 'install' }, true);
        delivery.onViewState(false);

        // When
        const shown = delivery.onViewState(true);

        // Then
        assert.ok(shown.action === 'assign' && shown.html.includes('const fragment = null;'), JSON.stringify(shown));
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
