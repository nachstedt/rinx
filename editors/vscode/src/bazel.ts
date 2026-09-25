import * as cp from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import { promisify } from 'util';

const exec = promisify(cp.exec);

// 30 second timeout for bazel queries (first query may load the workspace)
const BAZEL_TIMEOUT_MS = 30_000;
// 120 second timeout for bazel build (compilation can be slow)
const BAZEL_BUILD_TIMEOUT_MS = 120_000;

export interface DiscoveredConfig {
    siteTarget: string;      // e.g. "//Doc:site"
    binaryPath: string;      // absolute filesystem path to the rinx binary
    configPath: string;      // absolute filesystem path to config.toml
    templatePath: string;    // absolute filesystem path to template.html
    indexPath: string;       // absolute filesystem path to site.project.index
}

type LogFn = (msg: string) => void;
type Executor = (command: string, options: cp.ExecOptions) => Promise<{ stdout: string, stderr: string }>;

export class BazelScanner {
    private outputBase: string | undefined;
    private log: LogFn;
    private executor: Executor;

    constructor(log: LogFn = () => {}, executor: Executor = exec) {
        this.log = log;
        this.executor = executor;
    }

    private async bazelExec(command: string, workspaceRoot: string, timeoutMs: number = BAZEL_TIMEOUT_MS): Promise<string> {
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
            return result.split('\n').filter(line => line.length > 0);
        } catch (error: any) {
            this.log(`findSiteTargets failed: ${error.message || error}`);
            return [];
        }
    }

    async queryAttribute(target: string, attribute: string, workspaceRoot: string): Promise<string | undefined> {
        try {
            const query = `labels(${attribute}, ${target})`;
            const result = await this.bazelExec(`bazel query '${query}'`, workspaceRoot);
            const label = result.split('\n')[0];
            return label.length > 0 ? label : undefined;
        } catch (error: any) {
            this.log(`queryAttribute(${attribute}) failed: ${error.message || error}`);
            return undefined;
        }
    }

    async getOutputBase(workspaceRoot: string): Promise<string> {
        if (this.outputBase) return this.outputBase;
        try {
            this.outputBase = await this.bazelExec('bazel info output_base', workspaceRoot);
            return this.outputBase;
        } catch (error: any) {
            this.log(`getOutputBase failed: ${error.message || error}`);
            return '';
        }
    }

    async labelToFilesystemPath(label: string, workspaceRoot: string): Promise<string | undefined> {
        // Handle Bzlmod labels: @@repo//pkg:file or //pkg:file
        if (label.startsWith('@@')) {
            const match = label.match(/^@@([^/]+)\/\/([^:]+):(.+)$/);
            if (match) {
                const [, repo, pkg, file] = match;
                const outputBase = await this.getOutputBase(workspaceRoot);
                if (!outputBase) return undefined;
                return path.join(outputBase, 'external', repo, pkg, file);
            }
        }

        const match = label.match(/^\/\/([^:]*):(.+)$/);
        if (match) {
            const [, pkg, file] = match;
            return path.join(workspaceRoot, pkg, file);
        }

        return undefined;
    }

    deriveIndexPath(siteTarget: string, workspaceRoot: string): string {
        return this.deriveBazelBinPath(siteTarget, workspaceRoot, '.project.index');
    }

    async deriveBinaryPath(siteTarget: string, workspaceRoot: string): Promise<string | undefined> {
        const workerLabel = '@rinx//:rinx_worker';

        try {
            // 1. Try to find the output path via cquery
            const result = await this.bazelExec(
                `bazel cquery --output=files '${workerLabel}'`, workspaceRoot
            );
            
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
        } catch (error: any) {
            this.log(`Worker binary discovery failed: ${error.message || error}`);
        }

        return undefined;
    }

    private deriveBazelBinPath(target: string, workspaceRoot: string, suffix: string): string {
        const match = target.match(/^\/\/([^:]+):(.+)$/);
        if (match) {
            const [, pkg, name] = match;
            return path.join(workspaceRoot, 'bazel-bin', pkg, `${name}${suffix}`);
        }
        const rootMatch = target.match(/^\/\/:(.+)$/);
        if (rootMatch) {
            const [, name] = rootMatch;
            return path.join(workspaceRoot, 'bazel-bin', `${name}${suffix}`);
        }
        return '';
    }
}
