import * as assert from 'assert';
import * as path from 'path';
import * as vscode from 'vscode';
import type { RinxApi } from '../../extension';
import { waitFor } from './wait';

/** The file `name` at the root of the fixture workspace. */
function workspaceFile(name: string): vscode.Uri {
    const root = vscode.workspace.workspaceFolders?.[0].uri.fsPath ?? '';
    return vscode.Uri.file(path.join(root, name));
}

/** The extension's API, once it is active. */
async function rinx(): Promise<RinxApi> {
    const extension = vscode.extensions.getExtension<RinxApi>('nachstedt.rinx');
    assert.ok(extension, 'the extension is installed');
    return extension.isActive ? extension.exports : await extension.activate();
}

/** The open tab titled `label`, if any. */
function tabLabelled(label: string): vscode.Tab | undefined {
    return vscode.window.tabGroups.all.flatMap((group) => group.tabs).find((tab) => tab.label === label);
}

suite('Preview (e2e)', () => {
    teardown(async () => {
        const labelled = vscode.workspace.textDocuments.find((document) =>
            document.uri.fsPath.endsWith('labelled.rst'),
        );
        if (labelled?.isDirty) {
            await vscode.window.showTextDocument(labelled);
            await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
        }
        await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    });

    test('the preview command opens a preview panel', async () => {
        // Given
        await vscode.window.showTextDocument(workspaceFile('with_include.rst'));

        // When
        await vscode.commands.executeCommand('rinx.showPreview');

        // Then
        await waitFor('the preview tab', () => tabLabelled('Rinx Preview'));
    });

    test('the preview shows an image of a Sphinx project where it lies', async () => {
        // Given
        const api = await rinx();
        await vscode.window.showTextDocument(workspaceFile('sphinx/pictured.rst'));

        // When
        await vscode.commands.executeCommand('rinx.showPreview');

        // Then — linked under the address the webview serves files at, not
        // into the `_images/` a build would copy it into
        const page = await waitFor('the page with its image', () => {
            const html = api.previewHtml();
            return html?.includes('Pictured') && html;
        });
        const src = /<img [^>]*src="([^"]*)"/.exec(page)?.[1] ?? '';
        assert.ok(src.startsWith('https://'), page);
        assert.ok(src.endsWith(workspaceFile('sphinx/logo.svg').path), page);
    });

    test('the preview draws a diagram, with no JVM', async () => {
        // Given
        const api = await rinx();
        await vscode.window.showTextDocument(workspaceFile('drawn.rst'));

        // When
        await vscode.commands.executeCommand('rinx.showPreview');

        // Then — the server sent the text, the webview drew the picture with
        // PlantUML's JavaScript build; the broken template drew none
        const page = await waitFor('the page of drawn.rst', () => {
            const html = api.previewHtml();
            return html?.includes('data-rinx-plantuml') && html;
        });
        assert.ok(!page.includes('_images/'), page);
        assert.ok(page.includes('needuml: template error on line 2 of the diagram'), page);
        const drawn = await waitFor('the diagram drawn', () => api.previewDiagrams());
        assert.deepStrictEqual(drawn, { type: 'diagrams', drawn: 1, failed: [] });
    });

    test('the preview shows a title changed a moment ago in another document', async () => {
        // Given — `navigating.rst` links to `labelled.rst`'s section by
        // label, so the link shows that section's title
        const api = await rinx();
        await vscode.window.showTextDocument(workspaceFile('navigating.rst'));
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');
        await vscode.commands.executeCommand('rinx.showPreview');
        await waitFor('the page with the old title', () => api.previewHtml()?.includes('Installing'));
        // The focus into the preview and back to the editor, as when glancing
        // at the page and typing on, the panel visible throughout
        await vscode.commands.executeCommand('workbench.action.focusSecondEditorGroup');
        await vscode.commands.executeCommand('workbench.action.focusFirstEditorGroup');

        // When — the title changes in `labelled.rst`'s buffer, unsaved
        const labelled = await vscode.workspace.openTextDocument(workspaceFile('labelled.rst'));
        const edit = new vscode.WorkspaceEdit();
        edit.replace(labelled.uri, new vscode.Range(2, 0, 2, 10), 'Setting up');
        await vscode.workspace.applyEdit(edit);

        // Then
        const page = await waitFor('the page with the new title', () => {
            const html = api.previewHtml();
            return html?.includes('Setting up') && html;
        });
        assert.ok(page.includes('before you start.'), 'still the page of navigating.rst');
    });
    test('a link in the preview opens the document it leads to, and the preview follows', async () => {
        // Given — the page of `navigating.rst`, which links `labelled.rst`
        const api = await rinx();
        await vscode.window.showTextDocument(workspaceFile('navigating.rst'));
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');
        await vscode.commands.executeCommand('rinx.showPreview');
        await waitFor('the page of navigating.rst', () => api.previewHtml()?.includes('before you start.'));

        // When — the link to the label is clicked
        await api.followPreviewLink('labelled.html#install-guide');

        // Then — its source is open, and the preview shows its page
        await waitFor('labelled.rst open in an editor', () =>
            vscode.window.visibleTextEditors.some((editor) => editor.document.uri.fsPath.endsWith('labelled.rst')),
        );
        await waitFor('the page of labelled.rst', () => api.previewHtml()?.includes('How to install.'));
    });

    test('a link to a page the build writes alone leaves the preview as it was', async () => {
        // Given
        const api = await rinx();
        await vscode.window.showTextDocument(workspaceFile('navigating.rst'));
        await waitFor('the index to be ready', () => api.indexStatus()?.state === 'ready');
        await vscode.commands.executeCommand('rinx.showPreview');
        const page = await waitFor('the page of navigating.rst', () => {
            const html = api.previewHtml();
            return html?.includes('before you start.') && html;
        });

        // When
        await api.followPreviewLink('genindex.html');

        // Then
        assert.strictEqual(api.previewHtml(), page);
        assert.ok(
            !vscode.window.visibleTextEditors.some((editor) => editor.document.uri.fsPath.endsWith('genindex.rst')),
        );
    });

    suite('scrolling with the editor', () => {
        /** What `editor` shows and what the preview last reported, for a failure to name. */
        function syncState(api: RinxApi, editor: vscode.TextEditor): string {
            return JSON.stringify({
                visible: editor.visibleRanges.map((range) => [range.start.line, range.end.line]),
                editors: vscode.window.visibleTextEditors.map((shown) => path.basename(shown.document.uri.fsPath)),
                sync: api.previewSync(),
            });
        }

        /** Waits as {@link waitFor} does, a timeout naming the state of `editor` and the preview. */
        async function waitForSync<T>(
            what: string,
            api: RinxApi,
            editor: vscode.TextEditor,
            probe: () => T | undefined | false,
        ): Promise<T> {
            try {
                return await waitFor(what, probe);
            } catch (error) {
                throw new Error(
                    `${error instanceof Error ? error.message : String(error)}; ${syncState(api, editor)}`,
                    {
                        cause: error,
                    },
                );
            }
        }

        /** The editor of `scrolled.rst`, with its page shown in the preview and the page's script listening. */
        async function previewed(): Promise<{ api: RinxApi; editor: vscode.TextEditor }> {
            const api = await rinx();
            const editor = await vscode.window.showTextDocument(workspaceFile('scrolled.rst'));
            await vscode.commands.executeCommand('rinx.showPreview');
            await waitFor('the page of scrolled.rst', () => api.previewHtml()?.includes('Paragraph 80'));
            await waitFor('the cursor marked', () => api.previewSync()?.marked !== undefined);
            return { api, editor };
        }

        test('the page names the line each block was written on', async () => {
            // When
            const { api } = await previewed();

            // Then — paragraph 25 on line 51, counted from 0
            const page = api.previewHtml() ?? '';
            assert.ok(page.includes('<p data-source-line="51">Paragraph 25 '), page);
        });

        test('scrolling the editor scrolls the preview', async () => {
            // Given
            const { api, editor } = await previewed();

            // When
            editor.revealRange(new vscode.Range(101, 0, 101, 0), vscode.TextEditorRevealType.AtTop);

            // Then — the editor scrolled, and the page followed it to
            // paragraph 50, well down the page
            await waitForSync(
                'the editor scrolled',
                api,
                editor,
                () => (editor.visibleRanges[0]?.start.line ?? 0) >= 95,
            );
            const revealed = await waitForSync('the page scrolled', api, editor, () => {
                const report = api.previewSync()?.revealed;
                return report && report.line >= 95 && report;
            });
            assert.ok(revealed.y > 0, JSON.stringify(revealed));
        });

        test('the preview marks the block holding the cursor', async () => {
            // Given
            const { api, editor } = await previewed();

            // When
            editor.selection = new vscode.Selection(51, 4, 51, 4);

            // Then
            await waitFor('paragraph 25 marked', () => api.previewSync()?.marked === 51);
        });

        test('scrolling the preview scrolls the editor', async () => {
            // Given
            const { api, editor } = await previewed();

            // When — the reader scrolls the page to paragraph 40
            api.scrollPreviewTo(81);

            // Then
            await waitForSync('the editor scrolled', api, editor, () => {
                const top = editor.visibleRanges[0]?.start.line ?? 0;
                return top >= 80 && top <= 82;
            });
        });

        test('a double-click on the preview moves the cursor to the line', async () => {
            // Given
            const { api } = await previewed();

            // When
            await api.doubleClickPreview(41);

            // Then
            await waitFor(
                'the cursor on line 41',
                () =>
                    vscode.window.activeTextEditor?.document.uri.fsPath.endsWith('scrolled.rst') &&
                    vscode.window.activeTextEditor.selection.active.line === 41,
            );
        });
    });
});
