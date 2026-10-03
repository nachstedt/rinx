// The extension's tests, run inside a real VS Code by @vscode/test-cli:
//
//   npm test                                  # every label, on stable
//   VSCODE_VERSION=min npm test               # on the oldest VS Code engines.vscode allows
//   npm test -- --label unit --coverage       # one label, with coverage
//
// - `unit` needs no workspace.
// - `e2e` opens a fixture workspace and talks to the real `rinx lsp`: the
//   binary at RINX_BINARY, or else the repository's `cargo build` output.
// - `e2e-no-server` points the extension at a binary that does not exist.
// - `e2e-vsix`, only when RINX_VSIX names a packaged extension, runs the e2e
//   tests against that VSIX installed, rather than against this checkout.
//
// Each e2e label gets its own copy of test-fixtures/workspace in the temp
// directory, with the settings it needs written into the copy: test-cli shares
// one user-data directory between labels, so a setting written by a test would
// leak into the next label, and the fixtures in the repository stay untouched.
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from '@vscode/test-cli';

const manifest = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'));
const repository = (path) => fileURLToPath(new URL(`../../${path}`, import.meta.url));

/** The VS Code to test against: `min` names the oldest engines.vscode allows. */
function vscodeVersion() {
    const requested = process.env.VSCODE_VERSION ?? 'stable';
    return requested === 'min' ? manifest.engines.vscode.replace(/^\^/, '') : requested;
}

/** A fresh copy of the fixture workspace whose settings are `settings`. */
function fixtureWorkspace(name, settings) {
    const workspace = mkdtempSync(join(tmpdir(), `rinx-${name}-`));
    cpSync(fileURLToPath(new URL('./test-fixtures/workspace', import.meta.url)), workspace, { recursive: true });
    mkdirSync(join(workspace, '.vscode'));
    writeFileSync(join(workspace, '.vscode', 'settings.json'), JSON.stringify(settings, null, 2));
    return workspace;
}

const rinxBinary = process.env.RINX_BINARY ?? repository('target/debug/rinx');

/** The settings every e2e workspace shares: no Bazel, the repository's theme. */
const workspaceSettings = {
    'rinx.autoDiscover': false,
    'rinx.trace.server': 'verbose',
    'rinx.configPath': repository('templates/default_config.toml'),
    'rinx.templatePath': repository('templates/default.html'),
};

const common = { version: vscodeVersion(), mocha: { ui: 'tdd', timeout: 20_000 } };

/** The e2e tests against `vsix` installed, with an empty extension in development. */
function packagedExtensionTests(vsix) {
    return {
        ...common,
        label: 'e2e-vsix',
        files: ['out/test/e2e/**/*.test.js', 'out/test/e2e-vsix/**/*.test.js'],
        extensionDevelopmentPath: fileURLToPath(new URL('./test-fixtures/vsix-host', import.meta.url)),
        installExtensions: [resolve(vsix)],
        workspaceFolder: fixtureWorkspace('e2e-vsix', {
            ...workspaceSettings,
            'rinx.binaryPath': rinxBinary,
        }),
    };
}

export default defineConfig({
    tests: [
        { ...common, label: 'unit', files: 'out/test/unit/**/*.test.js' },
        {
            ...common,
            label: 'e2e',
            files: 'out/test/e2e/**/*.test.js',
            workspaceFolder: fixtureWorkspace('e2e', {
                ...workspaceSettings,
                'rinx.binaryPath': rinxBinary,
            }),
        },
        ...(process.env.RINX_VSIX ? [packagedExtensionTests(process.env.RINX_VSIX)] : []),
        {
            ...common,
            label: 'e2e-no-server',
            files: 'out/test/e2e-no-server/**/*.test.js',
            workspaceFolder: fixtureWorkspace('e2e-no-server', {
                ...workspaceSettings,
                'rinx.binaryPath': '/nonexistent/rinx',
            }),
        },
    ],
    coverage: {
        // Matched against absolute paths of the compiled files (test-cli turns
        // c8's relative matching off), hence the leading `**/`.
        // dist/ is the bundle the e2e tests load as the extension.
        include: ['**/out/**', '**/dist/**'],
        exclude: ['**/out/test/**'],
        // Files no test loads count as uncovered rather than disappearing.
        includeAll: true,
        // json-summary is what scripts/check-coverage.mjs reads and prints,
        // limited to src/: the bundle's source map also reaches into the
        // libraries it carries, and c8 applies `exclude` before source maps.
        reporter: ['json-summary', 'lcov'],
    },
});
