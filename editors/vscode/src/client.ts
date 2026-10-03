// The language client: starts `rinx lsp` and connects it to every
// reStructuredText document, so the server's diagnostics reach the editor.
//
// The server is the same binary the build runs (see
// docs/decisions/038-language-server.md), so its diagnostics are the build's.

import * as vscode from 'vscode';
import { LanguageClient, LanguageClientOptions, ServerOptions } from 'vscode-languageclient/node';
import { BazelScanner } from './bazel';
import { chooseBinaryPath, explicitValue } from './binary';

/** Finds the binary for the language server, the way the preview finds it. */
export async function resolveServerBinary(scanner: BazelScanner, log: (msg: string) => void): Promise<string> {
    const settings = vscode.workspace.getConfiguration('rinx');
    const configured = explicitValue(settings.inspect<string>('binaryPath'));
    const workspaceRoot = vscode.workspace.workspaceFolders?.[0].uri.fsPath;
    let discovered: string | undefined;
    if (!configured && workspaceRoot && (settings.get<boolean>('autoDiscover') ?? true)) {
        discovered = await scanner.deriveBinaryPath(workspaceRoot);
    }
    const binary = chooseBinaryPath(configured, discovered);
    log(`Language server binary: ${binary}`);
    return binary;
}

/** The options connecting the client to every reStructuredText document. */
export function clientOptions(outputChannel: vscode.LogOutputChannel): LanguageClientOptions {
    return {
        documentSelector: [
            { scheme: 'file', language: 'restructuredtext' },
            { scheme: 'untitled', language: 'restructuredtext' },
        ],
        outputChannel,
    };
}

/** Starts the language server, returning the running client. */
export async function startLanguageClient(
    binary: string,
    outputChannel: vscode.LogOutputChannel,
): Promise<LanguageClient> {
    const serverOptions: ServerOptions = { command: binary, args: ['lsp'] };
    const client = new LanguageClient('rinx', 'Rinx', serverOptions, clientOptions(outputChannel));
    await client.start();
    return client;
}
