// The status bar item showing the language server's workspace index, fed by
// the server's `rinx/status` notification (crates/lsp/src/progress.rs).

import * as vscode from 'vscode';

/** The notification the server reports its index on. */
export const STATUS_METHOD = 'rinx/status';

/** One project, as `rinx/status` names it. */
export type ProjectStatus =
    /**
     * A Sphinx project, by its `conf.py`'s path within its workspace folder,
     * with the interpreter `conf.py` was run in when it was run rather than read.
     */
    | { kind: 'sphinx'; conf: string; executedWith?: string }
    /** A workspace folder's documents under no `conf.py`, by the folder's name. */
    | { kind: 'folder'; root: string }
    /** A `rinx_site` read from its Bazel manifest, by its label. */
    | { kind: 'bazel'; site: string };

/** The parameters of `rinx/status`. */
export interface IndexStatus {
    state: 'indexing' | 'ready';
    /** The documents indexed so far — every one, once ready. */
    documents: number;
    /** How long the scan took, once ready. */
    elapsedMs?: number;
    /** The projects found so far. */
    projects?: ProjectStatus[];
}

/**
 * What the status bar shows for `status`: `rinx: 512 docs indexed (0.8 s)`,
 * or with Sphinx projects `rinx: Sphinx project (docs/conf.py) · 512 docs (0.8 s)`
 * — `(docs/conf.py, run)` when its `conf.py` was run — and with Bazel sites,
 * which come first, `rinx: rinx project //docs:site · 512 docs (0.8 s)`.
 */
export function statusText(status: IndexStatus): string {
    if (status.state === 'indexing') {
        return '$(sync~spin) rinx: indexing…';
    }
    const docs = status.documents === 1 ? 'doc' : 'docs';
    const seconds = ((status.elapsedMs ?? 0) / 1000).toFixed(1);
    const projects = projectsText(status);
    if (projects === undefined) {
        return `rinx: ${status.documents} ${docs} indexed (${seconds} s)`;
    }
    return `rinx: ${projects} · ${status.documents} ${docs} (${seconds} s)`;
}

/** The projects `status` names, as the status bar says them, if it names any but folders. */
function projectsText(status: IndexStatus): string | undefined {
    const sites = bazelSites(status);
    if (sites.length > 0) {
        return sites.length === 1 ? `rinx project ${sites[0]}` : `${sites.length} rinx projects`;
    }
    const sphinx = sphinxConfs(status);
    if (sphinx.length === 0) return undefined;
    return sphinx.length === 1
        ? `Sphinx project (${sphinx[0].conf}${sphinx[0].executedWith ? ', run' : ''})`
        : `${sphinx.length} Sphinx projects`;
}

/** The tooltip for `status`: what the item is, and every project, one per line. */
export function statusTooltip(status: IndexStatus): string {
    const lines = ['The rinx language server’s index of this workspace'];
    for (const project of status.projects ?? []) {
        if (project.kind === 'folder') {
            lines.push(`Folder: ${project.root}`);
        } else if (project.kind === 'bazel') {
            lines.push(`rinx project: ${project.site}`);
        } else if (project.executedWith) {
            lines.push(`Sphinx project: ${project.conf}, run with ${project.executedWith}`);
        } else {
            lines.push(`Sphinx project: ${project.conf}`);
        }
    }
    return lines.join('\n');
}

/** The label of every Bazel site `status` names. */
function bazelSites(status: IndexStatus): string[] {
    return (status.projects ?? []).flatMap((project) => (project.kind === 'bazel' ? [project.site] : []));
}

/** Every Sphinx project `status` names. */
function sphinxConfs(status: IndexStatus): { conf: string; executedWith?: string }[] {
    return (status.projects ?? []).flatMap((project) => (project.kind === 'sphinx' ? [project] : []));
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
        this.item.tooltip = statusTooltip(status);
        this.item.show();
    }

    dispose(): void {
        this.item.dispose();
    }
}
