import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
    ALLOW,
    cacheKey,
    consentMessage,
    decideConsent,
    DownloadSetting,
    FetcherDependencies,
    hostsOf,
    InventoryFetcher,
    isFresh,
    NEVER,
    NOT_NOW,
    RememberedConsent,
} from '../../inventories';

const DAY = 24 * 60 * 60 * 1000;
const PYTHON = 'https://docs.python.org/3/objects.inv';
const NUMPY = 'https://numpy.org/doc/stable/objects.inv';

/** A fetcher over a fresh cache directory, recording what it asked and downloaded. */
function fetcher(options: {
    setting?: DownloadSetting;
    choice?: string | undefined;
    download?: (url: string) => Promise<Uint8Array>;
    now?: () => number;
    storageDir?: string;
}) {
    const prompts: string[] = [];
    const downloads: string[] = [];
    let remembered: RememberedConsent | undefined;
    const storageDir = options.storageDir ?? fs.mkdtempSync(path.join(os.tmpdir(), 'rinx-inventories-'));
    const deps: FetcherDependencies = {
        storageDir,
        setting: () => options.setting ?? 'ask',
        remembered: () => remembered,
        remember: (consent) => {
            remembered = consent;
            return Promise.resolve();
        },
        prompt: (message) => {
            prompts.push(message);
            return Promise.resolve(options.choice);
        },
        download: (url) => {
            downloads.push(url);
            return options.download ? options.download(url) : Promise.resolve(new TextEncoder().encode(`inv ${url}`));
        },
        now: options.now ?? (() => 10 * DAY),
        log: () => {},
    };
    return { fetcher: new InventoryFetcher(deps), prompts, downloads, storageDir, remembered: () => remembered };
}

suite('Inventory download Test Suite', () => {
    test('cacheKey: a stable hex digest per address', () => {
        assert.match(cacheKey(PYTHON), /^[0-9a-f]{64}$/);
        assert.strictEqual(cacheKey(PYTHON), cacheKey(PYTHON));
        assert.notStrictEqual(cacheKey(PYTHON), cacheKey(NUMPY));
    });

    test('isFresh: within the limit, never past it, and always for a negative limit', () => {
        assert.strictEqual(isFresh(0, 4 * DAY, 5), true);
        assert.strictEqual(isFresh(0, 5 * DAY, 5), false);
        assert.strictEqual(isFresh(0, 1, 0), false);
        assert.strictEqual(isFresh(0, 1000 * DAY, -1), true);
    });

    test('decideConsent: the setting first, then what the workspace remembers', () => {
        assert.strictEqual(decideConsent('always', 'never'), 'allow');
        assert.strictEqual(decideConsent('never', 'allow'), 'deny');
        assert.strictEqual(decideConsent('ask', 'allow'), 'allow');
        assert.strictEqual(decideConsent('ask', 'never'), 'deny');
        assert.strictEqual(decideConsent('ask', undefined), 'ask');
    });

    test('hostsOf: each host once, in order', () => {
        assert.deepStrictEqual(hostsOf([PYTHON, NUMPY, 'https://docs.python.org/2/objects.inv']), [
            'docs.python.org',
            'numpy.org',
        ]);
    });

    test('consentMessage: names one host or several', () => {
        assert.match(consentMessage(['a.org']), /inventories of a\.org, so references into that site/);
        assert.match(consentMessage(['a.org', 'b.org', 'c.org']), /of a\.org, b\.org and c\.org, so .* those sites/);
    });

    test('fetchInventory: downloads once when allowed, then answers from the cache', async () => {
        // Given
        const { fetcher: inventories, downloads, prompts } = fetcher({ setting: 'always' });

        // When
        const first = await inventories.fetchInventory({ url: PYTHON, cacheLimitDays: 5 });
        const second = await inventories.fetchInventory({ url: PYTHON, cacheLimitDays: 5 });

        // Then
        assert.ok(first.path);
        assert.strictEqual(fs.readFileSync(first.path, 'utf8'), `inv ${PYTHON}`);
        assert.deepStrictEqual(second, first);
        assert.deepStrictEqual(downloads, [PYTHON]);
        assert.deepStrictEqual(prompts, []);
    });

    test('fetchInventory: downloads an expired copy again', async () => {
        // Given
        let now = 0;
        const { fetcher: inventories, downloads } = fetcher({ setting: 'always', now: () => now });
        await inventories.fetchInventory({ url: PYTHON, cacheLimitDays: 5 });

        // When
        now = 6 * DAY;
        await inventories.fetchInventory({ url: PYTHON, cacheLimitDays: 5 });

        // Then
        assert.deepStrictEqual(downloads, [PYTHON, PYTHON]);
    });

    test('fetchInventory: one prompt names every host asked for together, and Allow is remembered', async () => {
        // Given
        const { fetcher: inventories, prompts, downloads, remembered } = fetcher({ choice: ALLOW });

        // When
        const answers = await Promise.all([
            inventories.fetchInventory({ url: PYTHON, cacheLimitDays: 5 }),
            inventories.fetchInventory({ url: NUMPY, cacheLimitDays: 5 }),
        ]);

        // Then
        assert.strictEqual(prompts.length, 1);
        assert.match(prompts[0], /docs\.python\.org and numpy\.org/);
        assert.ok(answers.every((answer) => answer.path));
        assert.deepStrictEqual(new Set(downloads), new Set([PYTHON, NUMPY]));
        assert.strictEqual(remembered(), 'allow');
    });

    test('fetchInventory: Not Now declines for the session without asking again', async () => {
        // Given
        const { fetcher: inventories, prompts, downloads, remembered } = fetcher({ choice: NOT_NOW });

        // When
        const first = await inventories.fetchInventory({ url: PYTHON, cacheLimitDays: 5 });
        const second = await inventories.fetchInventory({ url: NUMPY, cacheLimitDays: 5 });

        // Then
        assert.ok(first.unavailable);
        assert.ok(second.unavailable);
        assert.strictEqual(prompts.length, 1);
        assert.deepStrictEqual(downloads, []);
        assert.strictEqual(remembered(), undefined);
    });

    test('fetchInventory: Never is remembered for the workspace', async () => {
        // Given
        const { fetcher: inventories, remembered } = fetcher({ choice: NEVER });

        // When
        const answer = await inventories.fetchInventory({ url: PYTHON, cacheLimitDays: 5 });

        // Then
        assert.ok(answer.unavailable);
        assert.strictEqual(remembered(), 'never');
    });

    test('fetchInventory: a failed download falls back to an expired copy, else says why', async () => {
        // Given — a copy downloaded long ago
        let now = 0;
        const storageDir = fs.mkdtempSync(path.join(os.tmpdir(), 'rinx-inventories-'));
        await fetcher({ setting: 'always', storageDir, now: () => now }).fetcher.fetchInventory({
            url: PYTHON,
            cacheLimitDays: 5,
        });
        now = 100 * DAY;
        const offline = () => Promise.reject(new Error('getaddrinfo ENOTFOUND'));
        const { fetcher: inventories } = fetcher({ setting: 'always', storageDir, now: () => now, download: offline });

        // When
        const stale = await inventories.fetchInventory({ url: PYTHON, cacheLimitDays: 5 });
        const missing = await inventories.fetchInventory({ url: NUMPY, cacheLimitDays: 5 });

        // Then
        assert.ok(stale.path);
        assert.strictEqual(fs.readFileSync(stale.path, 'utf8'), `inv ${PYTHON}`);
        assert.deepStrictEqual(missing, { unavailable: 'the download failed: getaddrinfo ENOTFOUND' });
    });

    test('fetchInventory: a declined download still answers with an expired copy', async () => {
        // Given
        let now = 0;
        const storageDir = fs.mkdtempSync(path.join(os.tmpdir(), 'rinx-inventories-'));
        await fetcher({ setting: 'always', storageDir, now: () => now }).fetcher.fetchInventory({
            url: PYTHON,
            cacheLimitDays: 5,
        });
        now = 100 * DAY;

        // When
        const answer = await fetcher({ setting: 'never', storageDir, now: () => now }).fetcher.fetchInventory({
            url: PYTHON,
            cacheLimitDays: 5,
        });

        // Then
        assert.ok(answer.path);
    });

    test('fetchInventory: concurrent requests for one address share one download', async () => {
        // Given
        const { fetcher: inventories, downloads } = fetcher({ setting: 'always' });

        // When
        await Promise.all([
            inventories.fetchInventory({ url: PYTHON, cacheLimitDays: 5 }),
            inventories.fetchInventory({ url: PYTHON, cacheLimitDays: 5 }),
        ]);

        // Then
        assert.deepStrictEqual(downloads, [PYTHON]);
    });
});
