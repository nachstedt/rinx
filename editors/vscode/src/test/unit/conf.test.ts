import * as assert from 'assert';
import * as path from 'path';
import {
    chooseInterpreter,
    InterpreterDependencies,
    NO_INTERPRETER,
    NOT_ENABLED,
    PythonExtensionApi,
    resolveConfigured,
    selectedByPythonExtension,
    UNTRUSTED,
} from '../../conf';

const CONF = 'file:///work/docs/conf.py';
const FOLDER = path.join(path.sep, 'work');

/** Dependencies for a trusted workspace allowing runs, with `overrides`. */
function deps(overrides: Partial<InterpreterDependencies> = {}): InterpreterDependencies {
    return {
        isTrusted: () => true,
        execute: () => true,
        pythonPath: () => undefined,
        folder: () => FOLDER,
        selectedInterpreter: () => Promise.resolve(undefined),
        ...overrides,
    };
}

suite('conf.py interpreter Test Suite', () => {
    test('chooseInterpreter: an untrusted workspace runs nothing, whatever the settings say', async () => {
        assert.deepStrictEqual(
            await chooseInterpreter({ conf: CONF }, deps({ isTrusted: () => false, pythonPath: () => 'python3' })),
            { unavailable: UNTRUSTED },
        );
    });

    test('chooseInterpreter: nothing runs while rinx.conf.execute is off', async () => {
        assert.deepStrictEqual(
            await chooseInterpreter({ conf: CONF }, deps({ execute: () => false, pythonPath: () => 'python3' })),
            { unavailable: NOT_ENABLED },
        );
    });

    test('chooseInterpreter: the setting comes before the Python extension', async () => {
        assert.deepStrictEqual(
            await chooseInterpreter(
                { conf: CONF },
                deps({ pythonPath: () => 'python3', selectedInterpreter: () => Promise.resolve('/venv/bin/python') }),
            ),
            { path: 'python3' },
        );
    });

    test('chooseInterpreter: the Python extension chooses when the setting is empty', async () => {
        const asked: string[] = [];
        assert.deepStrictEqual(
            await chooseInterpreter(
                { conf: CONF },
                deps({
                    selectedInterpreter: (conf) => {
                        asked.push(conf);
                        return Promise.resolve('/venv/bin/python');
                    },
                }),
            ),
            { path: '/venv/bin/python' },
        );
        assert.deepStrictEqual(asked, [CONF]);
    });

    test('chooseInterpreter: with neither, there is no interpreter', async () => {
        assert.deepStrictEqual(await chooseInterpreter({ conf: CONF }, deps()), { unavailable: NO_INTERPRETER });
    });

    test('resolveConfigured: a command stays one, a relative path joins the folder', () => {
        assert.strictEqual(resolveConfigured('python3', FOLDER), 'python3');
        assert.strictEqual(resolveConfigured('.venv/bin/python', FOLDER), path.join(FOLDER, '.venv/bin/python'));
        assert.strictEqual(resolveConfigured('/usr/bin/python3', FOLDER), '/usr/bin/python3');
        assert.strictEqual(resolveConfigured('.venv/bin/python', undefined), '.venv/bin/python');
    });

    test('selectedByPythonExtension: resolves the active environment of the resource', async () => {
        const resources: unknown[] = [];
        const api: PythonExtensionApi = {
            environments: {
                getActiveEnvironmentPath: (resource) => {
                    resources.push(resource);
                    return { id: 'venv', path: '/venv' };
                },
                resolveEnvironment: (environment) =>
                    Promise.resolve(
                        typeof environment !== 'string' && environment.id === 'venv'
                            ? { executable: { uri: { fsPath: '/venv/bin/python' } } }
                            : undefined,
                    ),
                onDidChangeActiveEnvironmentPath: () => ({ dispose: () => {} }),
            },
        };

        assert.strictEqual(await selectedByPythonExtension(api, 'the resource'), '/venv/bin/python');
        assert.deepStrictEqual(resources, ['the resource']);
    });

    test('selectedByPythonExtension: an environment without an executable gives none', async () => {
        const api: PythonExtensionApi = {
            environments: {
                getActiveEnvironmentPath: () => ({ id: 'conda', path: '/conda' }),
                resolveEnvironment: () => Promise.resolve({ executable: {} }),
                onDidChangeActiveEnvironmentPath: () => ({ dispose: () => {} }),
            },
        };

        assert.strictEqual(await selectedByPythonExtension(api, undefined), undefined);
    });
});
