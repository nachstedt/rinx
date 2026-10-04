// The status bar item showing the language server's workspace index, fed by
// the server's `rinx/status` notification (crates/lsp/src/progress.rs).

import * as vscode from 'vscode';

/** The notification the server reports its index on. */
export const STATUS_METHOD = 'rinx/status';

/** The parameters of `rinx/status`. */
export interface IndexStatus {
    state: 'indexing' | 'ready';
    /** The documents indexed so far — every one, once ready. */
    documents: number;
    /** How long the scan took, once ready. */
    elapsedMs?: number;
}

/** What the status bar shows for `status`, e.g. `rinx: 512 docs indexed (0.8 s)`. */
export function statusText(status: IndexStatus): string {
    if (status.state === 'indexing') {
        return '$(sync~spin) rinx: indexing…';
    }
    const docs = status.documents === 1 ? 'doc' : 'docs';
    const seconds = ((status.elapsedMs ?? 0) / 1000).toFixed(1);
    return `rinx: ${status.documents} ${docs} indexed (${seconds} s)`;
}

/** The status bar item, hidden until the server first reports. */
export class IndexStatusItem implements vscode.Disposable {
    private readonly item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left);
    /** The last status the server reported, for whoever asks the extension. */
    last: IndexStatus | undefined;

    constructor() {
        this.item.name = 'Rinx';
        this.item.tooltip = 'The rinx language server’s index of this workspace';
        this.item.command = 'rinx.showLogs';
    }

    update(status: IndexStatus): void {
        this.last = status;
        this.item.text = statusText(status);
        this.item.show();
    }

    dispose(): void {
        this.item.dispose();
    }
}
