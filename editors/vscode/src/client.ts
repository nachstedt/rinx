// The language client: starts `rinx lsp` and connects it to every
// reStructuredText document, so the server's diagnostics reach the editor.
//
// The server is the same binary the build runs (see
// docs/decisions/038-language-server.md), so its diagnostics are the build's.

import * as vscode from 'vscode';
import { LanguageClient, LanguageClientOptions, ServerOptions } from 'vscode-languageclient/node';
import { BazelScanner } from './bazel';
import { chooseBinaryPath, explicitValue } from './binary';

/**
 * Finds the binary for the language server, the way the preview finds it;
 * `bundled` is the one the VSIX carries, if any.
 */
export async function resolveServerBinary(
    scanner: BazelScanner,
    bundled: string | undefined,
    log: (msg: string) => void,
): Promise<string> {
    const settings = vscode.workspace.getConfiguration('rinx');
    const configured = explicitValue(settings.inspect<string>('binaryPath'));
    const workspaceRoot = vscode.workspace.workspaceFolders?.[0].uri.fsPath;
    let discovered: string | undefined;
    if (!configured && workspaceRoot && (settings.get<boolean>('autoDiscover') ?? true)) {
        discovered = await scanner.deriveBinaryPath(workspaceRoot);
    }
    const binary = chooseBinaryPath(configured, discovered, bundled);
    log(`Language server binary: ${binary}`);
    return binary;
}

/**
 * The options connecting the client to every reStructuredText document, and
 * telling the server this client downloads inventories (see inventories.ts),
 * chooses the interpreter `conf.py` runs in (see conf.ts) and builds a Bazel
 * workspace's site manifests (see manifests.ts).
 */
export function clientOptions(outputChannel: vscode.LogOutputChannel): LanguageClientOptions {
    return {
        documentSelector: [
            { scheme: 'file', language: 'restructuredtext' },
            { scheme: 'untitled', language: 'restructuredtext' },
        ],
        initializationOptions: { fetchInventories: true, evaluateConf: true, bazelManifests: true },
        outputChannel,
    };
}

/**
 * The client of the language server at `binary`, not started yet: every
 * handler must be registered before `start()`, since the server sends its
 * first requests — `rinx/bazelManifests` among them — as soon as it has found
 * the workspace's projects, and one arriving before its handler is refused.
 */
export function createLanguageClient(binary: string, outputChannel: vscode.LogOutputChannel): LanguageClient {
    const serverOptions: ServerOptions = { command: binary, args: ['lsp'] };
    return new LanguageClient('rinx', 'Rinx', serverOptions, clientOptions(outputChannel));
}
