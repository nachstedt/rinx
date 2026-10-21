import * as assert from 'assert';
import * as path from 'path';
import * as vscode from 'vscode';
import type { RinxApi } from '../../extension';
import { statusText } from '../../status';
import { waitFor } from '../e2e/wait';

/** `file` of the fixture's Bazel workspace. */
function workspaceFile(file: string): vscode.Uri {
    const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
    return vscode.Uri.file(path.join(root, file));
}

/** The extension's API, once it is active. */
async function rinx(): Promise<RinxApi> {
    const extension = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx');
    assert.ok(extension, 'the extension is installed');
    return extension.isActive ? extension.exports : await extension.activate();
}

/** The code `diagnostic` names. */
function codeOf(diagnostic: vscode.Diagnostic): string {
    const code = diagnostic.code;
    return typeof code === 'object' ? String(code.value) : String(code);
}

/** The codes of what the editor shows for `uri`. */
function codesFor(uri: vscode.Uri): string[] {
    return vscode.languages.getDiagnostics(uri).map(codeOf);
}

/** The line and code of each diagnostic the editor shows for `uri`. */
function linesAndCodes(uri: vscode.Uri): [number, string][] {
    return vscode.languages.getDiagnostics(uri).map((diagnostic) => [diagnostic.range.start.line, codeOf(diagnostic)]);
}

suite('Bazel project (e2e)', () => {
    teardown(async () => {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('the status bar names the site the manifest describes', async () => {
        // Given
        await vscode.window.showTextDocument(workspaceFile('reqs/index.rst'));
        const api = await rinx();

        // When — the server asks for the manifests, and the extension builds them
        const status = await waitFor('the site to be read from its manifest', () => {
            const reported = api.indexStatus();
            return (
                reported?.state === 'ready' &&
                reported.projects?.some((project) => project.kind === 'bazel') &&
                reported
            );
        });

        // Then
        assert.ok(statusText(status).startsWith('rinx: rinx project //:site · '), statusText(status));
    });

    test("each document is parsed with its own library's schema", async () => {
        // When
        await vscode.window.showTextDocument(workspaceFile('reqs/index.rst'));
        await vscode.window.showTextDocument(workspaceFile('other/index.rst'));

        // Then — `//reqs:docs` declares `req`, `//other:docs` does not
        await waitFor('the other library to report its entity as unknown', () =>
            codesFor(workspaceFile('other/index.rst')).includes('directive.unknown'),
        );
        await waitFor(
            'the entity library to report nothing',
            () => codesFor(workspaceFile('reqs/index.rst')).length === 0,
        );
    });

    test("a toctree entry outside the library's deps is underlined as the build fails", async () => {
        // When — `//other:docs` lists `reqs/index` in a toctree, but has no deps
        await vscode.window.showTextDocument(workspaceFile('other/index.rst'));

        // Then
        const undeclared = await waitFor('the undeclared toctree entry to be underlined', () =>
            vscode.languages
                .getDiagnostics(workspaceFile('other/index.rst'))
                .find((diagnostic) => codeOf(diagnostic) === 'toctree.undeclared-dependency'),
        );
        assert.strictEqual(undeclared.severity, vscode.DiagnosticSeverity.Error);
        assert.strictEqual(undeclared.range.start.line, 8);
    });

    test("a Jinja library's mistakes are underlined where they were written", async () => {
        // When — `//templated:docs` renders its document with `release`, and
        // a three-line template above the document's own mistake
        await vscode.window.showTextDocument(workspaceFile('templated/index.rst'));

        // Then — the heading rendered with its context fits its underline,
        // and each mistake is on its written line, in its own file
        // (waiting for exactly these, since the folder shows the unrendered
        // text's findings until the manifest arrives)
        for (const [file, expected] of [
            ['templated/index.rst', [[7, 'directive.unknown']]],
            ['templated/_header.rst', [[2, 'directive.unknown']]],
        ] as const) {
            await waitFor(
                `${file} to show exactly ${JSON.stringify(expected)}`,
                () => JSON.stringify(linesAndCodes(workspaceFile(file))) === JSON.stringify(expected),
            );
        }
    });

    test('the quick fix adds the missing library to deps', async () => {
        // Given — `//other:docs` lists `reqs/index` in a toctree, but has no deps
        const document = workspaceFile('other/index.rst');
        await vscode.window.showTextDocument(document);
        const undeclared = await waitFor('the undeclared toctree entry to be underlined', () =>
            vscode.languages.getDiagnostics(document).find((diagnostic) => {
                const code = diagnostic.code;
                const value = typeof code === 'object' ? String(code.value) : String(code);
                return value === 'toctree.undeclared-dependency';
            }),
        );

        // When
        const actions = await vscode.commands.executeCommand<vscode.CodeAction[]>(
            'vscode.executeCodeActionProvider',
            document,
            undeclared.range,
            vscode.CodeActionKind.QuickFix.value,
        );
        const fix = actions.find((action) => action.title === 'Add //reqs:docs to the deps of //other:docs');
        assert.ok(fix?.edit, JSON.stringify(actions.map((action) => action.title)));
        assert.ok(await vscode.workspace.applyEdit(fix.edit));

        // Then
        const build = await vscode.workspace.openTextDocument(workspaceFile('other/BUILD.bazel'));
        assert.strictEqual(
            build.getText(),
            [
                'load("@rinx//:defs.bzl", "rinx_library")',
                '',
                'rinx_library(',
                '    name = "docs",',
                '    srcs = ["index.rst"],',
                '    deps = ["//reqs:docs"],',
                ')',
                '',
            ].join('\n'),
        );
        // The workspace is this run's copy, so the edit may be kept.
        assert.ok(await build.save());
    });

    test("the preview draws a page with the site's own template", async () => {
        // Given
        const api = await rinx();
        await vscode.window.showTextDocument(workspaceFile('reqs/index.rst'));
        await waitFor('the site to be read from its manifest', () =>
            api.indexStatus()?.projects?.some((project) => project.kind === 'bazel'),
        );

        // When
        await vscode.commands.executeCommand('rinx.showPreview');

        // Then — `page.html`, which the manifest names, with `site.css`
        // linked where it lies, under the address the webview serves files at
        const page = await waitFor('the page in the site template', () => {
            const html = api.previewHtml();
            return html?.includes('· fixture site</title>') && html;
        });
        const href = /href="([^"]*)"/.exec(page)?.[1] ?? '';
        assert.ok(href.startsWith('https://'), page);
        assert.ok(href.endsWith(workspaceFile('site.css').path), page);
    });
});
