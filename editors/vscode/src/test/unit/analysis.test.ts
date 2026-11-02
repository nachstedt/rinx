import * as assert from 'assert';
import {
    ANALYSE_PROJECT_METHOD,
    AnalysisClient,
    analysisPage,
    AnalysisReport,
    ConstructReport,
    projectTitle,
    reportPage,
} from '../../analysis';
import { NO_SERVER_PAGE } from '../../preview';

const AUTOMODULE: ConstructReport = {
    kind: 'unknownDirective',
    name: 'automodule',
    count: 2,
    provider: 'sphinx.ext.autodoc',
    link: 'https://example.com/compatibility.html#compatibility-autodoc',
    listed: true,
    examples: [{ uri: 'file:///docs/my%20page.rst', line: 4, column: 1 }],
};

/** A report of one Sphinx project writing `constructs`. */
function report(constructs: ConstructReport[], complete = true): AnalysisReport {
    return {
        complete,
        projects: [
            {
                project: { kind: 'sphinx', conf: 'docs/conf.py' },
                documents: 1,
                extensions: [
                    { name: 'sphinx.ext.autodoc', support: 'incomplete', link: 'https://example.com/#autodoc' },
                    { name: 'sphinx.ext.napoleon', support: 'complete' },
                    { name: 'my_ext', support: 'unknown' },
                ],
                constructs,
            },
        ],
    };
}

suite('Analysis report Test Suite', () => {
    test('projectTitle names each kind of project', () => {
        assert.strictEqual(projectTitle({ kind: 'sphinx', conf: 'docs/conf.py' }), 'Sphinx project docs/conf.py');
        assert.strictEqual(projectTitle({ kind: 'bazel', site: '//docs:site' }), 'rinx project //docs:site');
        assert.strictEqual(projectTitle({ kind: 'folder', root: 'docs' }), 'Folder docs');
    });

    test('reportPage: a construct shows its count, provider, link and first places', () => {
        // Given / When
        const page = reportPage(report([AUTOMODULE]), 'abc');

        // Then
        assert.match(page, /Unknown directives \(1 distinct, 2 uses\)/);
        assert.match(page, /<code>automodule<\/code>/);
        assert.match(page, /sphinx\.ext\.autodoc/);
        assert.match(
            page,
            /data-link="https:\/\/example\.com\/compatibility\.html#compatibility-autodoc">compatibility</,
        );
        // No workspace is open here, so the whole path is shown.
        assert.match(
            page,
            /data-uri="file:\/\/\/docs\/my%20page\.rst" data-line="4" data-column="1">\/docs\/my page\.rst:4</,
        );
    });

    test('reportPage: a construct no row names says it is not listed', () => {
        // Given / When
        const page = reportPage(report([{ ...AUTOMODULE, listed: false }]), 'abc');

        // Then
        assert.match(page, />not listed</);
    });

    test('reportPage: a kind nothing wrote has no section', () => {
        // Given / When
        const page = reportPage(report([AUTOMODULE]), 'abc');

        // Then
        assert.doesNotMatch(page, /Ignored roles/);
    });

    test('reportPage: an ignored role is listed apart', () => {
        // Given / When
        const page = reportPage(report([{ ...AUTOMODULE, kind: 'ignoredRole', name: 'issue' }]), 'abc');

        // Then
        assert.match(page, /Ignored roles/);
        assert.doesNotMatch(page, /Unknown directives/);
    });

    test('reportPage: extensions say how far each is supported', () => {
        // Given / When
        const page = reportPage(report([]), 'abc');

        // Then
        assert.match(page, /sphinx\.ext\.autodoc<\/code>: not fully supported/);
        assert.match(page, /sphinx\.ext\.napoleon<\/code>: supported/);
        assert.match(page, /my_ext<\/code>: unknown to rinx/);
    });

    test('reportPage: what a document wrote is escaped', () => {
        // Given / When
        const page = reportPage(report([{ ...AUTOMODULE, name: '<img src=x>' }]), 'abc');

        // Then
        assert.doesNotMatch(page, /<img src=x>/);
        assert.match(page, /&lt;img src=x&gt;/);
    });

    test('reportPage: a scan under way is said, and the script runs under the nonce', () => {
        // Given / When
        const scanning = reportPage(report([], false), 'abc');
        const done = reportPage(report([]), 'abc');

        // Then
        assert.match(scanning, /still being scanned/);
        assert.doesNotMatch(done, /still being scanned/);
        assert.match(done, /script-src 'nonce-abc'/);
        assert.match(done, /<script nonce="abc">/);
    });

    test('reportPage: no project is said', () => {
        // Given / When
        const page = reportPage({ complete: true, projects: [] }, 'abc');

        // Then
        assert.match(page, /No project holds any documents yet/);
    });

    test('analysisPage: without a client it says the server is not running', async () => {
        assert.strictEqual(await analysisPage(undefined, 'abc'), NO_SERVER_PAGE);
    });

    test('analysisPage: asks the client for every project', async () => {
        // Given
        const asked: string[] = [];
        const client: AnalysisClient = {
            sendRequest: (method) => {
                asked.push(method);
                return Promise.resolve(report([AUTOMODULE]));
            },
        };

        // When
        const page = await analysisPage(client, 'abc');

        // Then
        assert.deepStrictEqual(asked, [ANALYSE_PROJECT_METHOD]);
        assert.match(page, /automodule/);
    });

    test('analysisPage: a failure is shown', async () => {
        // Given
        const client: AnalysisClient = { sendRequest: () => Promise.reject(new Error('it broke')) };

        // When
        const page = await analysisPage(client, 'abc');

        // Then
        assert.match(page, /could not be analysed/);
        assert.match(page, /it broke/);
    });
});
