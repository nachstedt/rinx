import * as assert from 'assert';
import { statusText, statusTooltip } from '../../status';

suite('Index status Test Suite', () => {
    test('statusText: a scan under way spins', () => {
        assert.strictEqual(statusText({ state: 'indexing', documents: 0 }), '$(sync~spin) rinx: indexing…');
    });

    test('statusText: a ready index counts its documents and the time taken', () => {
        assert.strictEqual(
            statusText({ state: 'ready', documents: 512, elapsedMs: 812 }),
            'rinx: 512 docs indexed (0.8 s)',
        );
    });

    test('statusText: one document is a doc', () => {
        assert.strictEqual(statusText({ state: 'ready', documents: 1, elapsedMs: 40 }), 'rinx: 1 doc indexed (0.0 s)');
    });

    test('statusText: a missing time reads as none', () => {
        assert.strictEqual(statusText({ state: 'ready', documents: 3 }), 'rinx: 3 docs indexed (0.0 s)');
    });

    test('statusText: one Sphinx project is named by its conf.py', () => {
        assert.strictEqual(
            statusText({
                state: 'ready',
                documents: 528,
                elapsedMs: 450,
                projects: [
                    { kind: 'folder', root: 'cpython' },
                    { kind: 'sphinx', conf: 'Doc/conf.py' },
                ],
            }),
            'rinx: Sphinx project (Doc/conf.py) · 528 docs (0.5 s)',
        );
    });

    test('statusText: several Sphinx projects are counted', () => {
        assert.strictEqual(
            statusText({
                state: 'ready',
                documents: 1,
                elapsedMs: 0,
                projects: [
                    { kind: 'sphinx', conf: 'a/conf.py' },
                    { kind: 'sphinx', conf: 'b/conf.py' },
                ],
            }),
            'rinx: 2 Sphinx projects · 1 doc (0.0 s)',
        );
    });

    test('statusText: folders alone read as before', () => {
        assert.strictEqual(
            statusText({ state: 'ready', documents: 2, elapsedMs: 0, projects: [{ kind: 'folder', root: 'docs' }] }),
            'rinx: 2 docs indexed (0.0 s)',
        );
    });

    test('statusTooltip: lists every project', () => {
        assert.strictEqual(
            statusTooltip({
                state: 'ready',
                documents: 2,
                projects: [
                    { kind: 'folder', root: 'docs' },
                    { kind: 'sphinx', conf: 'api/conf.py' },
                ],
            }),
            'The rinx language server’s index of this workspace\nFolder: docs\nSphinx project: api/conf.py',
        );
    });

    test('statusTooltip: without projects says what the item is', () => {
        assert.strictEqual(
            statusTooltip({ state: 'indexing', documents: 0 }),
            'The rinx language server’s index of this workspace',
        );
    });
});
