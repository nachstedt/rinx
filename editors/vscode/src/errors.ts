// Turning whatever a `catch` caught into text for the log.

/**
 * The message of `error` when it is an `Error`, else the value itself as text:
 * a rejected promise or a `throw` may carry anything.
 */
export function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
}
