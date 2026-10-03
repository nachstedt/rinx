// Where the rinx binary is, decided once for every feature that runs it.
//
// The preview and the language server both start the binary, and must agree on
// which one: a server and a preview from two different builds would disagree
// about the very diagnostics the preview is there to explain.

/**
 * The binary to run, from the setting the user wrote and the one Bazel built.
 *
 * An explicitly configured `rinx.binaryPath` wins, since it is the only way to
 * point at a development build; the binary discovered through Bazel comes
 * next; a bare `rinx` from the PATH is the fallback.
 */
export function chooseBinaryPath(configured: string | undefined, discovered: string | undefined): string {
    return configured || discovered || 'rinx';
}

/** The parts of a `WorkspaceConfiguration.inspect` result that a user wrote. */
export interface InspectedSetting<T> {
    globalValue?: T;
    workspaceValue?: T;
    workspaceFolderValue?: T;
}

/**
 * The value the user set, or `undefined` when only the default applies.
 *
 * `get` cannot tell the two apart, and the default (`rinx`) must not outrank
 * a binary Bazel discovered.
 */
export function explicitValue<T>(inspected: InspectedSetting<T> | undefined): T | undefined {
    return inspected?.workspaceFolderValue ?? inspected?.workspaceValue ?? inspected?.globalValue;
}
