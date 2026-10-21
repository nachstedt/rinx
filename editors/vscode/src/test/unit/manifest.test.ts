import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';

/** The extension's root, from out/test/unit where this test runs. */
const root = path.resolve(__dirname, '../../..');

/** The value at `keys` in the JSON file `file`, if there is one. */
function readJson(file: string, ...keys: string[]): unknown {
    let value: unknown = JSON.parse(fs.readFileSync(path.join(root, file), 'utf8'));
    for (const key of keys) {
        value = typeof value === 'object' && value !== null ? Reflect.get(value, key) : undefined;
    }
    return value;
}

/** The string at `keys` in the JSON file `file`, if there is one. */
function readString(file: string, ...keys: string[]): string | undefined {
    const value = readJson(file, ...keys);
    return typeof value === 'string' ? value : undefined;
}

/** The objects of the array at `keys` in the JSON file `file`, if there is one. */
function readObjects(file: string, ...keys: string[]): Record<string, unknown>[] {
    const value = readJson(file, ...keys);
    return Array.isArray(value)
        ? value.filter((item): item is Record<string, unknown> => typeof item === 'object' && item !== null)
        : [];
}

/** The `[major, minor]` a version or a `^`/`>=` range starts at. */
function majorMinor(version: string): [number, number] {
    const match = /(\d+)\.(\d+)/.exec(version);
    assert.ok(match, `not a version: ${version}`);
    return [Number(match[1]), Number(match[2])];
}

suite('Manifest Test Suite', () => {
    test('majorMinor: reads a range and a plain version', () => {
        assert.deepStrictEqual(majorMinor('^1.138.0'), [1, 138]);
        assert.deepStrictEqual(majorMinor('1.140.2'), [1, 140]);
    });

    // vsce refuses to package an extension whose @types/vscode describes a
    // newer API than engines.vscode promises to run on; failing here says so
    // on the pull request that raised one without the other.
    test('@types/vscode describes no API newer than engines.vscode allows', () => {
        const engine = readString('package.json', 'engines', 'vscode');
        const types = readString('node_modules/@types/vscode/package.json', 'version');
        assert.ok(engine && types);
        const [engineMajor, engineMinor] = majorMinor(engine);
        const [typesMajor, typesMinor] = majorMinor(types);
        assert.ok(
            typesMajor < engineMajor || (typesMajor === engineMajor && typesMinor <= engineMinor),
            `@types/vscode ${types} is newer than engines.vscode ${engine}; raise engines.vscode to match`,
        );
    });

    // The icon in a reStructuredText editor's title bar, as Markdown has one:
    // VS Code shows a `navigation` entry with an icon in the toolbar rather
    // than in the overflow menu, and the API cannot observe either.
    test('rinx.showPreview is an icon in the title bar of a reStructuredText editor', () => {
        // Given the manifest's commands and title-bar menu
        const command = readObjects('package.json', 'contributes', 'commands').find(
            (entry) => entry.command === 'rinx.showPreview',
        );
        const menu = readObjects('package.json', 'contributes', 'menus', 'editor/title').find(
            (entry) => entry.command === 'rinx.showPreview',
        );

        // When / Then the command has an icon, shown in the toolbar for reStructuredText only
        assert.strictEqual(command?.icon, '$(open-preview)');
        assert.strictEqual(menu?.group, 'navigation');
        assert.match(String(menu?.when), /\beditorLangId == restructuredtext\b/);
    });
});
