import * as assert from 'assert';
import { BazelConfigCache } from '../../extension';

suite('BazelConfigCache Test Suite', () => {
    test('get/set works', () => {
        const cache = new BazelConfigCache();
        const config = {
            siteTarget: '//Doc:site',
            binaryPath: '/bin/rinx',
            configPath: '/path/to/config',
            templatePath: '/path/to/template',
            indexPath: '/path/to/index'
        };
        
        cache.set('/root', config);
        assert.deepStrictEqual(cache.get('/root'), config);
    });

    test('returns undefined for unknown root', () => {
        const cache = new BazelConfigCache();
        assert.strictEqual(cache.get('/unknown'), undefined);
    });

    test('returns null for failed discovery', () => {
        const cache = new BazelConfigCache();
        cache.set('/root', null);
        assert.strictEqual(cache.get('/root'), null);
    });
});
