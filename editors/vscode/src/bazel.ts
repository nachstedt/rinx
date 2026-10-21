import * as cp from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import { promisify } from 'util';
import { errorMessage } from './errors';

const exec = promisify(cp.exec);

// 30 second timeout for bazel queries (first query may load the workspace)
const BAZEL_TIMEOUT_MS = 30_000;
// 120 second timeout for bazel build (compilation can be slow)
const BAZEL_BUILD_TIMEOUT_MS = 120_000;

type LogFn = (msg: string) => void;
type Executor = (command: string, options: cp.ExecOptions) => Promise<{ stdout: string; stderr: string }>;

/** The suffix of the file a `rinx_site`'s `lsp_manifest` output group writes. */
export const MANIFEST_SUFFIX = '.lsp_manifest.json';

export class BazelScanner {
    private executionRoot: string | undefined;
    private log: LogFn;
    private executor: Executor;

    constructor(log: LogFn = () => {}, executor: Executor = exec) {
        this.log = log;
        this.executor = executor;
    }

    private async bazelExec(
        command: string,
        workspaceRoot: string,
        timeoutMs: number = BAZEL_TIMEOUT_MS,
    ): Promise<string> {
        this.log(`Running: ${command}`);
        const { stdout, stderr } = await this.executor(command, { cwd: workspaceRoot, timeout: timeoutMs });
        if (stderr.trim()) {
            this.log(`  stderr: ${stderr.trim()}`);
        }
        const result = stdout.trim();
        this.log(`  result: ${result || '(empty)'}`);
        return result;
    }

    async findSiteTargets(workspaceRoot: string): Promise<string[]> {
        try {
            const query = `kind("rinx_site", //...)`;
            const result = await this.bazelExec(`bazel query '${query}'`, workspaceRoot);
            return result.split('\n').filter((line) => line.length > 0);
        } catch (error: unknown) {
            this.log(`findSiteTargets failed: ${errorMessage(error)}`);
            return [];
        }
    }

    /** `bazel info execution_root`, under which a generated file is found. */
    async getExecutionRoot(workspaceRoot: string): Promise<string> {
        this.executionRoot ??= await this.bazelExec('bazel info execution_root', workspaceRoot);
        return this.executionRoot;
    }

    /**
     * Builds the `lsp_manifest` output group of every site in `sites` — the
     * language server's description of each — and returns where each
     * manifest is, as an absolute path. Throws when Bazel fails.
     */
    async buildManifests(workspaceRoot: string, sites: string[]): Promise<string[]> {
        if (sites.length === 0) return [];
        const quoted = sites.map((site) => `'${site}'`).join(' ');
        await this.bazelExec(
            `bazel build --output_groups=lsp_manifest ${quoted}`,
            workspaceRoot,
            BAZEL_BUILD_TIMEOUT_MS,
        );
        // One expression: `cquery` given several targets as several words
        // answers for none of them.
        const files = await this.bazelExec(
            `bazel cquery --output=files --output_groups=lsp_manifest 'set(${sites.join(' ')})'`,
            workspaceRoot,
        );
        return files
            .split('\n')
            .map((line) => line.trim())
            .filter((line) => line.endsWith(MANIFEST_SUFFIX))
            .map((line) => (path.isAbsolute(line) ? line : path.join(workspaceRoot, line)));
    }

    async deriveBinaryPath(workspaceRoot: string): Promise<string | undefined> {
        const workerLabel = '@rinx//:rinx';

        try {
            // 1. Try to find the output path via cquery
            const result = await this.bazelExec(`bazel cquery --output=files '${workerLabel}'`, workspaceRoot);

            if (result) {
                // cquery may return multiple files; take the first one
                const firstPath = result.split('\n')[0].trim();
                const absPath = path.isAbsolute(firstPath) ? firstPath : path.join(workspaceRoot, firstPath);

                if (fs.existsSync(absPath)) {
                    this.log(`Binary found at: ${absPath}`);
                    return absPath;
                }

                // 2. If path found but doesn't exist on disk, attempt a build
                this.log(`Building ${workerLabel}...`);
                await this.bazelExec(`bazel build ${workerLabel}`, workspaceRoot, BAZEL_BUILD_TIMEOUT_MS);

                // Refresh the path after build (it might have changed configurations)
                const freshResult = await this.bazelExec(`bazel cquery --output=files '${workerLabel}'`, workspaceRoot);
                if (freshResult) {
                    const freshPath = freshResult.split('\n')[0].trim();
                    const freshAbsPath = path.isAbsolute(freshPath) ? freshPath : path.join(workspaceRoot, freshPath);
                    if (fs.existsSync(freshAbsPath)) {
                        return freshAbsPath;
                    }
                }
            }
        } catch (error: unknown) {
            this.log(`Worker binary discovery failed: ${errorMessage(error)}`);
        }

        return undefined;
    }
}
