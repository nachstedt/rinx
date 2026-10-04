import * as assert from 'assert';
import { statusText } from '../../status';

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
});
