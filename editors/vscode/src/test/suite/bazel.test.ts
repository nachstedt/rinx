import * as assert from 'assert';
import * as path from 'path';
import { BazelScanner } from '../../bazel';

suite('BazelScanner Test Suite', () => {
    const workspaceRoot = '/test/workspace';

    test('labelToFilesystemPath: local label', async () => {
        const scanner = new BazelScanner();
        const p = await scanner.labelToFilesystemPath('//Doc:rusty_sphinx.toml', workspaceRoot);
        assert.strictEqual(p, path.join(workspaceRoot, 'Doc', 'rusty_sphinx.toml'));
    });

    test('labelToFilesystemPath: external Bzlmod label', async () => {
        const mockExecutor = async (cmd: string) => {
            if (cmd === 'bazel info output_base') {
                return { stdout: '/output/base', stderr: '' };
            }
            return { stdout: '', stderr: '' };
        };
        const scanner = new BazelScanner(() => {}, mockExecutor);
        const p = await scanner.labelToFilesystemPath('@@rusty_sphinx//templates:default.html', workspaceRoot);
        assert.strictEqual(p, path.join('/output/base', 'external', 'rusty_sphinx', 'templates', 'default.html'));
    });

    test('deriveIndexPath: standard target', () => {
        const scanner = new BazelScanner();
        const p = scanner.deriveIndexPath('//Doc:site', workspaceRoot);
        assert.strictEqual(p, path.join(workspaceRoot, 'bazel-bin', 'Doc', 'site.project.index'));
    });

    test('deriveIndexPath: root target', () => {
        const scanner = new BazelScanner();
        const p = scanner.deriveIndexPath('//:site', workspaceRoot);
        assert.strictEqual(p, path.join(workspaceRoot, 'bazel-bin', 'site.project.index'));
    });

    test('deriveBinaryPath: finds and builds worker', async () => {
        const calls: string[] = [];
        const workerLabel = '@rusty_sphinx//:rusty_sphinx_worker';
        const mockExecutor = async (cmd: string) => {
            calls.push(cmd);
            if (cmd.includes('cquery --output=files')) {
                // Return path but it won't exist on disk (as fs is not mocked to return true)
                return { stdout: '/test/workspace/bazel-bin/rusty_sphinx_worker', stderr: '' };
            }
            if (cmd.includes('bazel build')) {
                return { stdout: '', stderr: '' };
            }
            return { stdout: '', stderr: '' };
        };

        const scanner = new BazelScanner(() => {}, mockExecutor);
        
        await scanner.deriveBinaryPath('//Doc:site', workspaceRoot);
        
        assert.ok(calls.some(c => c.includes(`bazel build ${workerLabel}`)), `Should have triggered build for ${workerLabel}`);
        assert.ok(calls.filter(c => c.includes('cquery')).length >= 1, 'Should have queried for path');
    });

    test('findSiteTargets: returns targets', async () => {
        const mockExecutor = async () => ({ stdout: '//Doc:site\n//examples:site', stderr: '' });
        const scanner = new BazelScanner(() => {}, mockExecutor);
        const targets = await scanner.findSiteTargets(workspaceRoot);
        assert.deepStrictEqual(targets, ['//Doc:site', '//examples:site']);
    });
});
