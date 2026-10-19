import * as assert from 'assert';
import { answerManifests, debounced, ManifestDependencies, NO_SITE, NOT_DISCOVERING } from '../../manifests';

/** Dependencies answering for `/work`, with `overrides`. */
function deps(overrides: Partial<ManifestDependencies> = {}): ManifestDependencies {
    return {
        autoDiscover: () => true,
        preferredSite: () => undefined,
        findSiteTargets: async () => ['//docs:site', '//examples:site'],
        buildManifests: async (_root, sites) => sites.map((site) => `/out/${site.replace(/[/:]+/g, '_')}.json`),
        executionRoot: async () => '/cache/execroot/_main',
        ...overrides,
    };
}

const params = { folder: 'file:///work' };

suite('Bazel manifests', () => {
    test('answers with every site built, and the roots its files are under', async () => {
        const built: string[][] = [];
        const result = await answerManifests(
            params,
            deps({
                buildManifests: async (root, sites) => {
                    built.push([root, ...sites]);
                    return ['/out/docs.json', '/out/examples.json'];
                },
            }),
        );

        assert.deepStrictEqual(built, [['/work', '//docs:site', '//examples:site']]);
        assert.deepStrictEqual(result, {
            manifests: ['/out/docs.json', '/out/examples.json'],
            workspaceRoot: '/work',
            executionRoot: '/cache/execroot/_main',
        });
    });

    test('names the preferred site when there is one', async () => {
        const result = await answerManifests(params, deps({ preferredSite: () => '//examples:site' }));

        assert.strictEqual(result.preferredSite, '//examples:site');
    });

    test('runs no Bazel when discovery is off', async () => {
        let asked = false;
        const result = await answerManifests(
            params,
            deps({
                autoDiscover: () => false,
                findSiteTargets: async () => {
                    asked = true;
                    return [];
                },
            }),
        );

        assert.deepStrictEqual(result, { unavailable: NOT_DISCOVERING });
        assert.strictEqual(asked, false);
    });

    test('says so when the workspace has no site', async () => {
        const result = await answerManifests(params, deps({ findSiteTargets: async () => [] }));

        assert.deepStrictEqual(result, { unavailable: NO_SITE });
    });

    test('says why when the build fails', async () => {
        const result = await answerManifests(
            params,
            deps({
                buildManifests: async () => {
                    throw new Error('ERROR: no such package');
                },
            }),
        );

        assert.ok(result.unavailable?.includes('no such package'), result.unavailable);
    });

    test('debounced notifies once after a burst, and not after cancel', async () => {
        let notified = 0;
        const trigger = debounced(() => (notified += 1), 10);

        trigger();
        trigger();
        trigger();
        await new Promise((resolve) => setTimeout(resolve, 40));
        trigger();
        trigger.cancel();
        await new Promise((resolve) => setTimeout(resolve, 40));

        assert.strictEqual(notified, 1);
    });
});
