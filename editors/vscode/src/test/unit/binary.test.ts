import * as assert from 'assert';
import * as path from 'path';
import { bundledBinaryPath, chooseBinaryPath, explicitValue } from '../../binary';

suite('Binary lookup Test Suite', () => {
    test('chooseBinaryPath: a configured path wins over a discovered and a bundled one', () => {
        assert.strictEqual(chooseBinaryPath('/dev/rinx', '/bazel/rinx', '/vsix/rinx'), '/dev/rinx');
    });

    test('chooseBinaryPath: the discovered path wins over the bundled one', () => {
        assert.strictEqual(chooseBinaryPath(undefined, '/bazel/rinx', '/vsix/rinx'), '/bazel/rinx');
    });

    test('chooseBinaryPath: the bundled path is used when nothing is configured or discovered', () => {
        assert.strictEqual(chooseBinaryPath(undefined, undefined, '/vsix/rinx'), '/vsix/rinx');
    });

    test('chooseBinaryPath: falls back to rinx on the PATH', () => {
        assert.strictEqual(chooseBinaryPath(undefined, undefined, undefined), 'rinx');
    });

    test('chooseBinaryPath: an empty setting counts as unset', () => {
        assert.strictEqual(chooseBinaryPath('', undefined, undefined), 'rinx');
    });

    test('bundledBinaryPath: the binary under bin/ when the extension carries one', () => {
        // Given
        const expected = path.join('/ext', 'bin', 'rinx');

        // When
        const found = bundledBinaryPath('/ext', 'linux', (file) => file === expected);

        // Then
        assert.strictEqual(found, expected);
    });

    test('bundledBinaryPath: undefined when the extension carries none', () => {
        // Given / When
        const found = bundledBinaryPath('/ext', 'darwin', () => false);

        // Then
        assert.strictEqual(found, undefined);
    });

    test('bundledBinaryPath: rinx.exe on Windows', () => {
        // Given
        const expected = path.join('/ext', 'bin', 'rinx.exe');

        // When
        const found = bundledBinaryPath('/ext', 'win32', (file) => file === expected);

        // Then
        assert.strictEqual(found, expected);
    });

    test('explicitValue: undefined when only the default applies', () => {
        assert.strictEqual(explicitValue<string>({}), undefined);
        assert.strictEqual(explicitValue<string>(undefined), undefined);
    });

    test('explicitValue: the most specific scope wins', () => {
        assert.strictEqual(
            explicitValue({ globalValue: 'global', workspaceValue: 'workspace', workspaceFolderValue: 'folder' }),
            'folder',
        );
        assert.strictEqual(explicitValue({ globalValue: 'global', workspaceValue: 'workspace' }), 'workspace');
        assert.strictEqual(explicitValue({ globalValue: 'global' }), 'global');
    });
});
