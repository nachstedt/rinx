import * as assert from 'assert';
import { chooseBinaryPath, explicitValue } from '../../binary';

suite('Binary lookup Test Suite', () => {
    test('chooseBinaryPath: a configured path wins over a discovered one', () => {
        assert.strictEqual(chooseBinaryPath('/dev/rinx', '/bazel/rinx'), '/dev/rinx');
    });

    test('chooseBinaryPath: the discovered path is used when nothing is configured', () => {
        assert.strictEqual(chooseBinaryPath(undefined, '/bazel/rinx'), '/bazel/rinx');
    });

    test('chooseBinaryPath: falls back to rinx on the PATH', () => {
        assert.strictEqual(chooseBinaryPath(undefined, undefined), 'rinx');
    });

    test('chooseBinaryPath: an empty setting counts as unset', () => {
        assert.strictEqual(chooseBinaryPath('', undefined), 'rinx');
    });

    test('explicitValue: undefined when only the default applies', () => {
        assert.strictEqual(explicitValue<string>({}), undefined);
        assert.strictEqual(explicitValue<string>(undefined), undefined);
    });

    test('explicitValue: the most specific scope wins', () => {
        assert.strictEqual(
            explicitValue({ globalValue: 'global', workspaceValue: 'workspace', workspaceFolderValue: 'folder' }),
            'folder'
        );
        assert.strictEqual(explicitValue({ globalValue: 'global', workspaceValue: 'workspace' }), 'workspace');
        assert.strictEqual(explicitValue({ globalValue: 'global' }), 'global');
    });
});
