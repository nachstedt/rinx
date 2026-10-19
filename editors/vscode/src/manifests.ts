// Building the Bazel manifests the language server reads a rinx project from.
//
// A `rinx_site`'s `lsp_manifest` output group describes every library's
// sources and parse settings (docs/decisions/038-language-server.md §2). The
// server never runs Bazel: for a workspace folder holding `MODULE.bazel` or a
// `WORKSPACE`, it asks with a `rinx/bazelManifests` request, and this builds
// the group of every site in the folder and answers with where the manifests
// are. Whenever that answer may have changed — a `BUILD` file or a `.bzl`
// edited, the preferred site changed — the extension sends
// `rinx/bazelManifestsChanged`, and the server asks again.

import { fileURLToPath } from 'url';
import { errorMessage } from './errors';

/** The request the server asks for a folder's manifests with. */
export const BAZEL_MANIFESTS_METHOD = 'rinx/bazelManifests';

/** The notification telling the server the answers may have changed. */
export const BAZEL_MANIFESTS_CHANGED_METHOD = 'rinx/bazelManifestsChanged';

/** The parameters of `rinx/bazelManifests`. */
export interface BazelManifestsParams {
    /** The URI of the workspace folder. */
    folder: string;
}

/** The answer: the manifests built, with where their files are, or why there are none. */
export interface BazelManifestsResult {
    manifests?: string[];
    workspaceRoot?: string;
    executionRoot?: string;
    /** The site a document several sites list belongs to. */
    preferredSite?: string;
    unavailable?: string;
}

/** What answering needs, so tests can stand in for VS Code and Bazel. */
export interface ManifestDependencies {
    /** `rinx.autoDiscover`: whether Bazel may be run at all. */
    autoDiscover: () => boolean;
    /** `rinx.site` when set, else the site the preview was pointed at. */
    preferredSite: () => string | undefined;
    /** Every `rinx_site` in the workspace at `root`. */
    findSiteTargets: (root: string) => Promise<string[]>;
    /** Builds the manifests of `sites`, returning their paths; throws when Bazel fails. */
    buildManifests: (root: string, sites: string[]) => Promise<string[]>;
    /** `bazel info execution_root`. */
    executionRoot: (root: string) => Promise<string>;
}

/** Why there are no manifests, when the user turned Bazel off. */
export const NOT_DISCOVERING = 'rinx.autoDiscover is off';

/** Why there are no manifests, when the folder has no site. */
export const NO_SITE = 'no rinx_site target in the workspace';

/** The answer to one `rinx/bazelManifests` request. */
export async function answerManifests(
    params: BazelManifestsParams,
    deps: ManifestDependencies,
): Promise<BazelManifestsResult> {
    if (!deps.autoDiscover()) return { unavailable: NOT_DISCOVERING };
    const root = fileURLToPath(params.folder);
    try {
        const sites = await deps.findSiteTargets(root);
        if (sites.length === 0) return { unavailable: NO_SITE };
        const manifests = await deps.buildManifests(root, sites);
        const executionRoot = await deps.executionRoot(root);
        const preferredSite = deps.preferredSite();
        return {
            manifests,
            workspaceRoot: root,
            executionRoot,
            ...(preferredSite ? { preferredSite } : {}),
        };
    } catch (error: unknown) {
        return { unavailable: `building the sites' manifests failed: ${errorMessage(error)}` };
    }
}

/**
 * Calls `notify` once, `delayMs` after the last of a burst of calls to the
 * returned function — so saving several `BUILD` files rebuilds once.
 */
export function debounced(notify: () => void, delayMs: number): { (): void; cancel(): void } {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const trigger = () => {
        if (timer !== undefined) clearTimeout(timer);
        timer = setTimeout(() => {
            timer = undefined;
            notify();
        }, delayMs);
    };
    trigger.cancel = () => {
        if (timer !== undefined) clearTimeout(timer);
        timer = undefined;
    };
    return trigger;
}
