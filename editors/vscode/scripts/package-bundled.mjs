// Packages the extension with a rinx binary inside, for the platform this
// runs on, so the installed VSIX needs neither rinx.binaryPath nor a rinx on
// the PATH:
//
//   npm run package:bundled                          # cargo build --release, then package
//   npm run package:bundled -- --binary path/to/rinx # package that binary instead
//
// The result is rinx-<target>.vsix, a platform-specific package (`vsce
// --target`): a binary runs on one platform only. `npm run package` stays the
// platform-neutral package without one.
import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const extension = fileURLToPath(new URL('..', import.meta.url));
const repository = fileURLToPath(new URL('../../..', import.meta.url));

/** The vsce target naming `platform` and `arch`, refusing what rinx is not built for. */
function vsceTarget(platform, arch) {
    const target = `${platform}-${arch}`;
    const supported = ['linux-x64', 'linux-arm64', 'darwin-x64', 'darwin-arm64', 'win32-x64'];
    if (!supported.includes(target)) {
        throw new Error(`no VSIX target for ${target}; supported: ${supported.join(', ')}`);
    }
    return target;
}

/** The binary to bundle: `--binary <path>`, else a fresh release build. */
function binaryToBundle(args) {
    const flag = args.indexOf('--binary');
    if (flag !== -1) {
        const path = args[flag + 1];
        if (!path) {
            throw new Error('--binary needs a path');
        }
        return path;
    }
    execFileSync('cargo', ['build', '--release', '--locked', '-p', 'rinx'], { cwd: repository, stdio: 'inherit' });
    return `${repository}target/release/rinx${process.platform === 'win32' ? '.exe' : ''}`;
}

/** The non-empty lines of `text`, sorted. */
function sortedLines(text) {
    return text
        .split('\n')
        .filter((line) => line !== '')
        .toSorted((a, b) => a.localeCompare(b));
}

/** Runs vsce from the extension's own dependencies, returning its output. */
function vsce(...args) {
    return execFileSync('npx', ['--no-install', 'vsce', ...args, '--no-dependencies'], {
        cwd: extension,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'inherit'],
    });
}

const target = vsceTarget(process.platform, process.arch);
const bundledName = process.platform === 'win32' ? 'bin/rinx.exe' : 'bin/rinx';
const binary = binaryToBundle(process.argv.slice(2));

rmSync(`${extension}bin`, { recursive: true, force: true });
mkdirSync(`${extension}bin`);
try {
    copyFileSync(binary, `${extension}${bundledName}`);
    chmodSync(`${extension}${bundledName}`, 0o755);
    execFileSync('node', ['scripts/write-license.mjs'], { cwd: extension, stdio: 'inherit' });

    const out = `rinx-${target}.vsix`;
    vsce('package', '--target', target, '--out', out);

    // The same files as the plain package, plus the binary: vsix-files.txt is
    // the plain package's list, which `npm run package:check` compares against.
    // Listed after packaging, which is what builds dist/extension.js.
    const expected = sortedLines(`${readFileSync(`${extension}vsix-files.txt`, 'utf8')}${bundledName}\n`);
    const actual = sortedLines(vsce('ls'));
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
        rmSync(`${extension}${out}`, { force: true });
        throw new Error(`unexpected packaged files:\n${actual.join('\n')}\nexpected:\n${expected.join('\n')}`);
    }
    console.log(`Packaged ${binary} into ${out}`);
} finally {
    // Left behind, the binary would ride along in the next plain package.
    rmSync(`${extension}bin`, { recursive: true, force: true });
}
