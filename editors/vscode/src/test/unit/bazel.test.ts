import * as assert from 'assert';
import * as path from 'path';
import { BazelScanner } from '../../bazel';

suite('BazelScanner Test Suite', () => {
    const workspaceRoot = '/test/workspace';

    test('deriveBinaryPath: finds and builds worker', async () => {
        const calls: string[] = [];
        const workerLabel = '@rinx//:rinx';
        const mockExecutor = async (cmd: string) => {
            calls.push(cmd);
            if (cmd.includes('cquery --output=files')) {
                // Return path but it won't exist on disk (as fs is not mocked to return true)
                return { stdout: '/test/workspace/bazel-bin/rinx', stderr: '' };
            }
            if (cmd.includes('bazel build')) {
                return { stdout: '', stderr: '' };
            }
            return { stdout: '', stderr: '' };
        };

        const scanner = new BazelScanner(() => {}, mockExecutor);

        await scanner.deriveBinaryPath(workspaceRoot);

        assert.ok(
            calls.some((c) => c.includes(`bazel build ${workerLabel}`)),
            `Should have triggered build for ${workerLabel}`,
        );
        assert.ok(calls.filter((c) => c.includes('cquery')).length >= 1, 'Should have queried for path');
    });

    test('findSiteTargets: returns targets', async () => {
        const mockExecutor = async () => ({ stdout: '//Doc:site\n//examples:site', stderr: '' });
        const scanner = new BazelScanner(() => {}, mockExecutor);
        const targets = await scanner.findSiteTargets(workspaceRoot);
        assert.deepStrictEqual(targets, ['//Doc:site', '//examples:site']);
    });

    test('buildManifests: builds the output group and keeps only the manifests', async () => {
        const calls: string[] = [];
        const mockExecutor = async (cmd: string) => {
            calls.push(cmd);
            if (cmd.includes('cquery')) {
                return {
                    stdout: 'bazel-out/k8/bin/docs/site.lsp_manifest.json\ndocs/rinx.toml\n/abs/b.lsp_manifest.json\n',
                    stderr: '',
                };
            }
            return { stdout: '', stderr: '' };
        };
        const scanner = new BazelScanner(() => {}, mockExecutor);

        const manifests = await scanner.buildManifests(workspaceRoot, ['//docs:site', '//b:site']);

        assert.deepStrictEqual(calls, [
            "bazel build --output_groups=lsp_manifest '//docs:site' '//b:site'",
            "bazel cquery --output=files --output_groups=lsp_manifest 'set(//docs:site //b:site)'",
        ]);
        assert.deepStrictEqual(manifests, [
            path.join(workspaceRoot, 'bazel-out/k8/bin/docs/site.lsp_manifest.json'),
            '/abs/b.lsp_manifest.json',
        ]);
    });

    test('buildManifests: builds nothing for no site', async () => {
        const calls: string[] = [];
        const scanner = new BazelScanner(
            () => {},
            async (cmd: string) => {
                calls.push(cmd);
                return { stdout: '', stderr: '' };
            },
        );

        assert.deepStrictEqual(await scanner.buildManifests(workspaceRoot, []), []);
        assert.deepStrictEqual(calls, []);
    });

    test('buildManifests: a failing build throws', async () => {
        const scanner = new BazelScanner(
            () => {},
            async () => {
                throw new Error('build failed');
            },
        );

        await assert.rejects(scanner.buildManifests(workspaceRoot, ['//docs:site']), /build failed/);
    });

    test('getExecutionRoot: asks Bazel once', async () => {
        let asked = 0;
        const scanner = new BazelScanner(
            () => {},
            async () => {
                asked += 1;
                return { stdout: '/cache/execroot/_main\n', stderr: '' };
            },
        );

        assert.strictEqual(await scanner.getExecutionRoot(workspaceRoot), '/cache/execroot/_main');
        assert.strictEqual(await scanner.getExecutionRoot(workspaceRoot), '/cache/execroot/_main');
        assert.strictEqual(asked, 1);
    });
});
