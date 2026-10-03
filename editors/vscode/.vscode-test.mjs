// The extension's tests, run inside a real VS Code by @vscode/test-cli:
//
//   npm test                                  # every label, on stable
//   VSCODE_VERSION=min npm test               # on the oldest VS Code engines.vscode allows
//   npm test -- --label unit --coverage       # one label, with coverage
//
// `unit` needs no workspace; PR 6's `e2e` adds a fixture workspace and the
// real rinx binary.
import { readFileSync } from 'node:fs';
import { defineConfig } from '@vscode/test-cli';

const manifest = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'));

/** The VS Code to test against: `min` names the oldest engines.vscode allows. */
function vscodeVersion() {
    const requested = process.env.VSCODE_VERSION ?? 'stable';
    return requested === 'min' ? manifest.engines.vscode.replace(/^\^/, '') : requested;
}

export default defineConfig({
    tests: [
        {
            label: 'unit',
            files: 'out/test/unit/**/*.test.js',
            version: vscodeVersion(),
            mocha: { ui: 'tdd', timeout: 20_000 },
        },
    ],
    coverage: {
        // Matched against absolute paths of the compiled files (test-cli turns
        // c8's relative matching off), hence the leading `**/`.
        include: ['**/out/**'],
        exclude: ['**/out/test/**'],
        // Files no test loads count as uncovered rather than disappearing.
        includeAll: true,
        // json-summary is what scripts/check-coverage.mjs reads.
        reporter: ['text', 'json-summary', 'lcov'],
    },
});
