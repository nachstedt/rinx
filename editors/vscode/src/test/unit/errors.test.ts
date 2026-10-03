import * as assert from 'assert';
import { errorMessage } from '../../errors';

suite('errorMessage Test Suite', () => {
    test('errorMessage: an Error gives its message', () => {
        assert.strictEqual(errorMessage(new Error('bazel not found')), 'bazel not found');
    });

    test('errorMessage: anything else is shown as text', () => {
        assert.strictEqual(errorMessage('plain string'), 'plain string');
        assert.strictEqual(errorMessage(42), '42');
        assert.strictEqual(errorMessage(undefined), 'undefined');
    });
});
