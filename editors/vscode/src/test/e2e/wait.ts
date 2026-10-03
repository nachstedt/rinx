// Waiting for what the editor does asynchronously, without fixed sleeps.

/** How long a condition may take before the test fails. */
const TIMEOUT_MS = 15_000;

/**
 * Resolves with the first truthy result of `probe`, polled until `TIMEOUT_MS`
 * has passed; then rejects naming `what`, with the last result seen.
 */
export async function waitFor<T>(what: string, probe: () => T | undefined | false): Promise<T> {
    const deadline = Date.now() + TIMEOUT_MS;
    let last: T | undefined | false;
    while (Date.now() < deadline) {
        last = probe();
        if (last) {
            return last;
        }
        await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`timed out waiting for ${what}; last seen: ${JSON.stringify(last)}`);
}
