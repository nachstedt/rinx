// Downloading the intersphinx inventories the language server asks for.
//
// The server never touches the network (docs/decisions/038-language-server.md
// §6): for each `objects.inv` a project's `intersphinx_mapping` names by
// address, it sends a `rinx/fetchInventory` request, and reads the file this
// answers with. The download needs the user's consent, asked once per
// workspace and naming the hosts; `rinx.inventories.download` decides it
// instead when set to `always` or `never`. Downloads are cached in the
// extension's global storage, fresh for the project's `intersphinx_cache_limit`
// days, and an expired copy is still used when downloading it again fails.

import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';

/** The request the server asks for an inventory with. */
export const FETCH_INVENTORY_METHOD = 'rinx/fetchInventory';

/** The parameters of `rinx/fetchInventory`. */
export interface FetchInventoryParams {
    url: string;
    /** How many days a cached copy stays fresh; negative for ever. */
    cacheLimitDays: number;
}

/** The answer: the file the inventory was saved to, or why there is none. */
export interface FetchInventoryResult {
    path?: string;
    unavailable?: string;
}

/** The `rinx.inventories.download` setting. */
export type DownloadSetting = 'ask' | 'always' | 'never';

/** What the user answered the consent prompt with, as the workspace remembers it. */
export type RememberedConsent = 'allow' | 'never';

/** The key the workspace's answer is remembered under. */
export const CONSENT_KEY = 'rinx.inventories.consent';

/** How long an inventory download may take before it counts as failed. */
const DOWNLOAD_TIMEOUT_MS = 30_000;

/** How long requests are collected before one prompt names all their hosts. */
const PROMPT_DELAY_MS = 200;

const DAY_MS = 24 * 60 * 60 * 1000;

/** The file name an inventory downloaded from `url` is cached under, without its extension. */
export function cacheKey(url: string): string {
    return crypto.createHash('sha256').update(url).digest('hex');
}

/** Whether a copy fetched at `fetchedAt` is still fresh at `now`, for `limitDays` days. */
export function isFresh(fetchedAt: number, now: number, limitDays: number): boolean {
    return limitDays < 0 || now - fetchedAt < limitDays * DAY_MS;
}

/** Whether to download: decided by the setting, then by what the workspace remembers. */
export function decideConsent(
    setting: DownloadSetting,
    remembered: RememberedConsent | undefined,
): 'allow' | 'deny' | 'ask' {
    if (setting === 'always') return 'allow';
    if (setting === 'never') return 'deny';
    if (remembered === 'allow') return 'allow';
    if (remembered === 'never') return 'deny';
    return 'ask';
}

/** The distinct hosts of `urls`, in order. */
export function hostsOf(urls: Iterable<string>): string[] {
    const hosts = new Set<string>();
    for (const url of urls) {
        try {
            hosts.add(new URL(url).host);
        } catch {
            hosts.add(url);
        }
    }
    return [...hosts];
}

/** The prompt's text, naming `hosts`. */
export function consentMessage(hosts: string[]): string {
    const named = hosts.length === 1 ? hosts[0] : `${hosts.slice(0, -1).join(', ')} and ${hosts[hosts.length - 1]}`;
    return (
        `Rinx can download the intersphinx inventories of ${named}, so references into ` +
        `${hosts.length === 1 ? 'that site' : 'those sites'} resolve in the editor. Download them?`
    );
}

/** The prompt's choices. */
export const ALLOW = 'Allow';
export const NOT_NOW = 'Not Now';
export const NEVER = 'Never';

/** What the fetcher needs from VS Code and the network, so tests can stand in for them. */
export interface FetcherDependencies {
    /** Where the cache lives: the extension's global storage. */
    storageDir: string;
    /** `rinx.inventories.download`, read whenever it is needed. */
    setting: () => DownloadSetting;
    /** The workspace's remembered answer. */
    remembered: () => RememberedConsent | undefined;
    remember: (consent: RememberedConsent) => Thenable<void>;
    /** Shows the consent prompt, resolving to the choice, or `undefined` when dismissed. */
    prompt: (message: string, ...choices: string[]) => Thenable<string | undefined>;
    /** Downloads `url`. */
    download: (url: string) => Promise<Uint8Array>;
    now: () => number;
    log: (message: string) => void;
}

/** What the cache keeps about one download besides its bytes. */
interface CacheEntry {
    url: string;
    fetchedAt: number;
}

/** Answers `rinx/fetchInventory` requests: from the cache, or with the user's consent, from the network. */
export class InventoryFetcher {
    /** This session's answer when the user chose "Not Now" or dismissed the prompt. */
    private declinedThisSession = false;
    /** The prompt being collected, with the hosts it will name. */
    private pendingPrompt: { urls: string[]; answer: Promise<boolean> } | undefined;
    /** The downloads under way, by address. */
    private inFlight = new Map<string, Promise<FetchInventoryResult>>();

    constructor(private readonly deps: FetcherDependencies) {}

    /** The answer to one `rinx/fetchInventory` request. */
    async fetchInventory(params: FetchInventoryParams): Promise<FetchInventoryResult> {
        const running = this.inFlight.get(params.url);
        if (running) return running;
        const answer = this.answer(params).finally(() => this.inFlight.delete(params.url));
        this.inFlight.set(params.url, answer);
        return answer;
    }

    private async answer({ url, cacheLimitDays }: FetchInventoryParams): Promise<FetchInventoryResult> {
        const file = path.join(this.deps.storageDir, 'inventories', `${cacheKey(url)}.inv`);
        const entry = readEntry(`${file}.json`);
        const cached = entry !== undefined && fs.existsSync(file);
        if (cached && isFresh(entry.fetchedAt, this.deps.now(), cacheLimitDays)) {
            return { path: file };
        }
        if (!(await this.consents(url))) {
            // An earlier download is no new one: it is used, however old.
            return cached ? { path: file } : { unavailable: 'downloading inventories is not allowed' };
        }
        try {
            const bytes = await this.deps.download(url);
            writeAtomically(file, bytes);
            writeAtomically(`${file}.json`, JSON.stringify({ url, fetchedAt: this.deps.now() } satisfies CacheEntry));
            this.deps.log(`Downloaded the inventory ${url}`);
            return { path: file };
        } catch (error: unknown) {
            const reason = error instanceof Error ? error.message : String(error);
            this.deps.log(`Downloading the inventory ${url} failed: ${reason}`);
            return cached ? { path: file } : { unavailable: `the download failed: ${reason}` };
        }
    }

    /** Whether the user lets `url` be downloaded, asking when nothing decides it yet. */
    private async consents(url: string): Promise<boolean> {
        const decided = decideConsent(this.deps.setting(), this.deps.remembered());
        if (decided !== 'ask') return decided === 'allow';
        if (this.declinedThisSession) return false;
        if (this.pendingPrompt) {
            this.pendingPrompt.urls.push(url);
            return this.pendingPrompt.answer;
        }
        const urls = [url];
        const answer = new Promise<void>((resolve) => setTimeout(resolve, PROMPT_DELAY_MS)).then(async () => {
            const choice = await this.deps.prompt(consentMessage(hostsOf(urls)), ALLOW, NOT_NOW, NEVER);
            this.pendingPrompt = undefined;
            if (choice === ALLOW) {
                await this.deps.remember('allow');
                return true;
            }
            if (choice === NEVER) {
                await this.deps.remember('never');
            } else {
                this.declinedThisSession = true;
            }
            return false;
        });
        this.pendingPrompt = { urls, answer };
        return answer;
    }
}

/** Downloads `url` with the global `fetch`, failing on an error status or after a timeout. */
export async function downloadWithFetch(url: string): Promise<Uint8Array> {
    const response = await fetch(url, { signal: AbortSignal.timeout(DOWNLOAD_TIMEOUT_MS) });
    if (!response.ok) {
        throw new Error(`HTTP ${response.status} ${response.statusText}`.trim());
    }
    return new Uint8Array(await response.arrayBuffer());
}

/** The cache entry at `file`, if there is a readable one. */
function readEntry(file: string): CacheEntry | undefined {
    try {
        const entry: unknown = JSON.parse(fs.readFileSync(file, 'utf8'));
        if (typeof entry !== 'object' || entry === null || !('url' in entry) || !('fetchedAt' in entry)) {
            return undefined;
        }
        const { url, fetchedAt } = entry;
        return typeof url === 'string' && typeof fetchedAt === 'number' ? { url, fetchedAt } : undefined;
    } catch {
        return undefined;
    }
}

/** Writes `data` to `file` through a temporary file, so a reader never sees half of it. */
function writeAtomically(file: string, data: Uint8Array | string): void {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    const temporary = `${file}.${process.pid}.tmp`;
    fs.writeFileSync(temporary, data);
    fs.renameSync(temporary, file);
}
