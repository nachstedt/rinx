// Choosing the interpreter the language server runs a Sphinx project's
// `conf.py` in.
//
// The server reads `conf.py` without running it
// (docs/decisions/038-language-server.md §4). Where that falls short — a
// computed setting, one changed after its literal — it asks with a
// `rinx/confInterpreter` request which interpreter to run the file in, and
// runs it only if this answers with one. That needs a trusted workspace and
// `rinx.conf.execute` on; the interpreter is `rinx.conf.pythonPath` when set,
// else the one the Python extension has selected for the `conf.py`. Whenever
// that answer may have changed, the extension sends
// `rinx/confInterpreterChanged`, and the server asks again.

import * as path from 'path';

/** The request the server asks for an interpreter with. */
export const CONF_INTERPRETER_METHOD = 'rinx/confInterpreter';

/** The notification telling the server the answers may have changed. */
export const CONF_INTERPRETER_CHANGED_METHOD = 'rinx/confInterpreterChanged';

/** The parameters of `rinx/confInterpreter`. */
export interface ConfInterpreterParams {
    /** The URI of the `conf.py` to run. */
    conf: string;
}

/** The answer: the interpreter to run `conf.py` in, or why it is not run. */
export interface ConfInterpreterResult {
    path?: string;
    unavailable?: string;
}

/** What choosing needs from VS Code, so tests can stand in for it. */
export interface InterpreterDependencies {
    /** Whether the user trusts the workspace. */
    isTrusted: () => boolean;
    /** `rinx.conf.execute`, for the folder holding `conf`. */
    execute: (conf: string) => boolean;
    /** `rinx.conf.pythonPath`, for the folder holding `conf`, if set. */
    pythonPath: (conf: string) => string | undefined;
    /** The folder `conf` belongs to, which a relative `rinx.conf.pythonPath` is relative to. */
    folder: (conf: string) => string | undefined;
    /** The interpreter the Python extension has selected for `conf`, if it is installed and has one. */
    selectedInterpreter: (conf: string) => Promise<string | undefined>;
}

/** Why `conf.py` is not run, when the workspace is not trusted. */
export const UNTRUSTED = 'the workspace is not trusted';

/** Why `conf.py` is not run, when the setting is off. */
export const NOT_ENABLED = 'rinx.conf.execute is off';

/** Why `conf.py` is not run, when there is no interpreter to run it in. */
export const NO_INTERPRETER =
    'no Python interpreter: set rinx.conf.pythonPath, or select one with the Python extension';

/** The answer to one `rinx/confInterpreter` request. */
export async function chooseInterpreter(
    params: ConfInterpreterParams,
    deps: InterpreterDependencies,
): Promise<ConfInterpreterResult> {
    if (!deps.isTrusted()) return { unavailable: UNTRUSTED };
    if (!deps.execute(params.conf)) return { unavailable: NOT_ENABLED };
    const configured = deps.pythonPath(params.conf);
    if (configured) return { path: resolveConfigured(configured, deps.folder(params.conf)) };
    const selected = await deps.selectedInterpreter(params.conf);
    return selected ? { path: selected } : { unavailable: NO_INTERPRETER };
}

/**
 * `configured`, a `rinx.conf.pythonPath`: a bare command (`python3`) stays
 * one, for the server to find on the PATH, and a relative path
 * (`.venv/bin/python`) is relative to `folder`.
 */
export function resolveConfigured(configured: string, folder: string | undefined): string {
    const isCommand = !configured.includes('/') && !configured.includes(path.sep);
    if (isCommand || path.isAbsolute(configured) || folder === undefined) return configured;
    return path.join(folder, configured);
}

/** The part of the Python extension's API choosing reads. */
export interface PythonExtensionApi {
    environments: {
        getActiveEnvironmentPath(resource?: unknown): { id: string; path: string };
        resolveEnvironment(
            environment: { id: string; path: string } | string,
        ): Promise<{ executable: { uri?: { fsPath: string } } } | undefined>;
        onDidChangeActiveEnvironmentPath: (listener: () => void) => { dispose(): void };
    };
}

/** The interpreter `api` has selected for `resource`, if it resolves to one. */
export async function selectedByPythonExtension(
    api: PythonExtensionApi,
    resource: unknown,
): Promise<string | undefined> {
    const environment = await api.environments.resolveEnvironment(api.environments.getActiveEnvironmentPath(resource));
    return environment?.executable.uri?.fsPath;
}
