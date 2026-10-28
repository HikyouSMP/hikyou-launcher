// Run with: node --conditions=browser --test src/hooks/controllers.test.mjs
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import ts from 'typescript';
import { compileModule } from 'svelte/compiler';
import { flushSync } from 'svelte';
import { effect_root, push, pop, mutable_source, get, set } from 'svelte/internal/client';

const require = createRequire(import.meta.url);
const environment = { invoke: async () => {}, language: 'ja', listeners: new Map(), currentWindow: {} };
globalThis.__hikyouControllerTest = environment;
const cache = new Map();
const mockCore = 'export const invoke = (...args) => globalThis.__hikyouControllerTest.invoke(...args);';
const mockTranslation = 'export function createTranslation() { return { get language() { return globalThis.__hikyouControllerTest.language; } }; }';
const mockI18n = 'export default { get language() { return globalThis.__hikyouControllerTest.language; }, t: key => key };';
const mockWindow = 'export function getCurrentWindow() { return globalThis.__hikyouControllerTest.currentWindow; }';
const mockEvents = `export async function listen(name, callback) {
    const listeners = globalThis.__hikyouControllerTest.listeners;
    const callbacks = listeners.get(name) ?? new Set();
    callbacks.add(callback); listeners.set(name, callbacks);
    return () => callbacks.delete(callback);
}`;
const dataUrl = source => `data:text/javascript;base64,${Buffer.from(source).toString('base64')}`;

async function moduleUrl(filename) {
    filename = path.resolve(filename);
    if (cache.has(filename)) return cache.get(filename);
    const pending = (async () => {
        if (filename.endsWith('.json')) {
            return dataUrl(`export default ${JSON.stringify(JSON.parse(readFileSync(filename, 'utf8')))};`);
        }
        let source = ts.transpileModule(readFileSync(filename, 'utf8'), {
            compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext },
        }).outputText;
        if (filename.endsWith('.svelte.ts')) source = compileModule(source, { filename, generate: 'client' }).js.code;
        const parsed = ts.createSourceFile(filename + '.js', source, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
        const edits = [];
        for (const statement of parsed.statements) {
            if (!ts.isImportDeclaration(statement)) continue;
            const specifier = statement.moduleSpecifier.text;
            let url;
            if (specifier === '@tauri-apps/api/core') url = dataUrl(mockCore);
            else if (specifier === '@tauri-apps/api/event') url = dataUrl(mockEvents);
            else if (specifier === '@tauri-apps/api/window') url = dataUrl(mockWindow);
            else if (specifier.endsWith('/i18n.svelte')) url = dataUrl(mockTranslation);
            else if (specifier.endsWith('/i18n')) url = dataUrl(mockI18n);
            else if (specifier.startsWith('.')) {
                const base = path.resolve(path.dirname(filename), specifier);
                const resolved = [base, base + '.ts', base + '.js'].find(existsSync);
                assert.ok(resolved, `Cannot resolve ${specifier} from ${filename}`);
                url = await moduleUrl(resolved);
            } else url = pathToFileURL(require.resolve(specifier)).href;
            edits.push([statement.moduleSpecifier.getStart(parsed), statement.moduleSpecifier.end, JSON.stringify(url)]);
        }
        for (const [start, end, replacement] of edits.reverse()) source = source.slice(0, start) + replacement + source.slice(end);
        return dataUrl(source);
    })();
    cache.set(filename, pending);
    return pending;
}

const load = async name => import(await moduleUrl(`src/hooks/${name}.svelte.ts`));
const reactive = initial => {
    const source = mutable_source(initial);
    return { get current() { return get(source); }, set current(next) { set(source, next); } };
};
const apply = (previous, update) => typeof update === 'function' ? update(previous) : update;
const deferred = () => {
    let resolve;
    let reject;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    return { promise, resolve, reject };
};
const settle = async () => { for (let i = 0; i < 5; i++) await Promise.resolve(); flushSync(); };
function keyboard(context) {
    const previous = new Map();
    class Element extends EventTarget { closest() { return null; } }
    class Input extends Element { focus() {} select() {} }
    for (const [name, value] of Object.entries({ window: new EventTarget(), HTMLElement: Element, HTMLInputElement: Input, HTMLTextAreaElement: Input })) {
        previous.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
        Object.defineProperty(globalThis, name, { configurable: true, value });
    }
    context.after(() => {
        for (const [name, descriptor] of previous) {
            if (descriptor) Object.defineProperty(globalThis, name, descriptor);
            else delete globalThis[name];
        }
    });
    return (key, extra = {}) => {
        const event = new Event('keydown', { cancelable: true });
        Object.assign(event, { key, code: key, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, repeat: false }, extra);
        window.dispatchEvent(event);
        return event;
    };
}
function mount(create) {
    let value;
    const destroy = effect_root(() => {
        push({}, true);
        value = create();
        pop();
    });
    flushSync();
    return { value, destroy };
}

function launchFixture() {
    const calls = [];
    const profile = { id: 'first', name: 'First', mcVersion: '1.21', loader: 'vanilla' };
    const state = {
        savedAuth: { username: 'Player', uuid: 'uuid' }, profiles: [profile], activeProfileId: profile.id,
        selectedVersion: '1.21', manifest: null, loaderType: 'vanilla', selectedFabricVersion: '',
        settings: { game: { memoryMb: 2048 }, advanced: { keepLauncherVisible: true } },
        profileRunStates: {}, profileLogs: {}, gameError: undefined, crashToastProfileId: null, crashAnalyses: {},
        launchingProfileIdRef: { current: null }, launchStartedAtRef: { current: new Map() },
        handleWebviewLogin: async () => { calls.push(['login']); },
    };
    for (const key of ['profiles', 'activeProfileId', 'profileRunStates', 'profileLogs', 'gameError', 'crashToastProfileId', 'crashAnalyses']) {
        state['set' + key[0].toUpperCase() + key.slice(1)] = update => { state[key] = apply(state[key], update); };
    }
    environment.invoke = async (command, args) => { calls.push([command, args]); };
    return { state, profile, calls };
}

test('state controllers expose current values and support updater setters', async () => {
    const { useLaunchState } = await load('useLaunchState');
    const { value, destroy } = mount(useLaunchState);
    value.setProfileRunStates({ first: { phase: 'running', dlProgress: null } });
    value.setProfileRunStates(previous => ({ ...previous, second: { phase: 'launching', dlProgress: null } }));
    assert.equal(value.profileRunStates.first.phase, 'running');
    assert.equal(value.profileRunStates.second.phase, 'launching');
    value.setGameError('failure');
    assert.equal(value.gameError, 'failure');
    destroy();
});

test('launch actions read current parameters instead of creation-time snapshots', async () => {
    const { useProfileActions } = await load('useProfileActions');
    const { state, calls } = launchFixture();
    const actions = useProfileActions(state);
    state.profiles = [{ id: 'second', name: 'Second', mcVersion: '1.22', loader: 'vanilla' }];
    state.activeProfileId = 'second';
    state.settings = { ...state.settings, game: { memoryMb: 6144 } };
    state.settings.game.javaOverrides = { '21': '/custom/java-21/bin/java' };
    await actions.handleLaunchGame();
    const launch = calls.find(([command]) => command === 'launch_game')[1];
    assert.equal(launch.profileId, 'second');
    assert.equal(launch.memoryMb, 6144);
    assert.equal(launch.version, '1.22');
    assert.deepEqual(launch.javaOverrides, { '21': '/custom/java-21/bin/java' });
});

test('native file picker prevents auto-hide until it closes', async context => {
    context.mock.timers.enable({ apis: ['setTimeout'] });
    const previous = Object.getOwnPropertyDescriptor(globalThis, 'document');
    let pickerOpen = true;
    const document = new EventTarget();
    document.querySelector = selector => selector === '[data-native-dialog-open]' && pickerOpen ? {} : null;
    Object.defineProperty(globalThis, 'document', { configurable: true, value: document });
    context.after(() => previous ? Object.defineProperty(globalThis, 'document', previous) : delete globalThis.document);
    let focus;
    environment.currentWindow = { onFocusChanged: callback => { focus = callback; return Promise.resolve(() => {}); } };
    const calls = [];
    environment.invoke = async (command, args) => calls.push([command, args]);
    const { useWindowFocusBehavior } = await load('useWindowFocusBehavior');
    const { destroy } = mount(() => useWindowFocusBehavior({
        loginModalOpenRef: { current: false }, crashToastOpenRef: { current: false },
        isDraggingRef: { current: false }, inputRef: { current: null },
    }));
    focus({ payload: false });
    context.mock.timers.tick(100);
    assert.equal(calls.length, 0);
    pickerOpen = false;
    focus({ payload: false });
    context.mock.timers.tick(100);
    assert.equal(calls[0][0], 'hide_main_window');
    destroy();
});

test('smart launch delegates resolution to backend and cancellation prevents late UI success', async () => {
    const { useProfileActions } = await load('useProfileActions');
    const { state, calls } = launchFixture();
    state.profiles[0].id = 'smart:latest-plus';
    state.activeProfileId = 'smart:latest-plus';
    const loader = deferred();
    environment.invoke = async (command, args) => { calls.push([command, args]); if (command === 'launch_game') return loader.promise; };
    const actions = useProfileActions(state);
    const launch = actions.handleLaunchGame();
    await actions.handleStopGame(state.activeProfileId);
    loader.resolve({ mcVersion: '1.21', loader: 'fabric', loaderVersion: '0.17', revisionId: 'prepared' });
    await launch;
    assert.equal(calls.some(([command]) => command === 'get_fabric_versions'), false);
    assert.equal(calls.filter(([command]) => command === 'launch_game').length, 1);
    assert.deepEqual(state.profileRunStates, {});
    assert.equal(state.gameError, undefined);
    assert.equal(state.launchStartedAtRef.current.size, 0);
});

test('stopping an in-flight launch stops again after spawn and never resurrects running state', async () => {
    const { useProfileActions } = await load('useProfileActions');
    const { state, calls, profile } = launchFixture();
    const spawned = deferred();
    environment.invoke = async (command, args) => { calls.push([command, args]); if (command === 'launch_game') return spawned.promise; };
    const actions = useProfileActions(state);
    const launch = actions.handleLaunchGame(profile);
    await actions.handleLaunchGame(profile);
    assert.equal(calls.filter(([command]) => command === 'launch_game').length, 1);
    await actions.handleStopGame(profile.id);
    spawned.resolve();
    await launch;
    assert.equal(calls.filter(([command]) => command === 'stop_game').length, 2);
    assert.deepEqual(state.profileRunStates, {});
    assert.equal(state.gameError, undefined);
});

test('launch completion cannot overwrite a newer backend-prepared target', async () => {
    const { useProfileActions } = await load('useProfileActions');
    const { state } = launchFixture();
    const pending = deferred();
    state.profiles[0] = { ...state.profiles[0], id: 'smart:latest-plus', kind: 'smart', resolved: { mcVersion: '1.21', loader: 'fabric', loaderVersion: '0.16' } };
    state.activeProfileId = 'smart:latest-plus';
    environment.invoke = async command => command === 'launch_game' ? pending.promise : undefined;
    const action = useProfileActions(state).handleLaunchGame();
    state.profiles = state.profiles.map(profile => ({ ...profile, resolved: { mcVersion: '1.22', loader: 'fabric', loaderVersion: '0.17' } }));
    pending.resolve({ mcVersion: '1.21', loader: 'fabric', loaderVersion: '0.16' });
    await action;
    assert.equal(state.profiles[0].resolved.mcVersion, '1.22');
});

test('an early game exit is not overwritten by a late launch response', async () => {
    const { useProfileActions } = await load('useProfileActions');
    const { state, profile } = launchFixture();
    const spawned = deferred();
    environment.invoke = async command => command === 'launch_game' ? spawned.promise : undefined;
    const actions = useProfileActions(state);
    const launch = actions.handleLaunchGame(profile);
    state.profileRunStates = {};
    spawned.resolve();
    await launch;
    assert.deepEqual(state.profileRunStates, {});
});

test('profile creation lock rejects duplicate submissions and releases after completion', async () => {
    const { useProfileActions } = await load('useProfileActions');
    const { state } = launchFixture();
    const created = deferred();
    environment.invoke = async () => created.promise;
    const actions = useProfileActions(state);
    const first = actions.handleCreateProfile('1.21', 'vanilla', 'New');
    assert.equal(await actions.handleCreateProfile('1.21', 'vanilla', 'New'), null);
    created.resolve({ id: 'new', name: 'New', mcVersion: '1.21', loader: 'vanilla' });
    assert.equal((await first).id, 'new');
    assert.equal((await actions.handleCreateProfile('1.21', 'vanilla', 'New')).id, 'new');
});

test('search model getters recompute from fresh query and mode', async () => {
    const { useProfileSearchModel } = await load('useProfileSearchModel');
    const intent = reactive({ empty: true, nameTokens: [] });
    const mode = reactive('profile');
    const args = {
        creating: false, get intent() { return intent.current; },
        manifest: { latest: { release: '1.21' }, versions: [{ id: '1.21', type: 'release' }, { id: '1.20', type: 'release' }] },
        matchedProfiles: [], modpackResults: [{ project_id: 'pack' }], profiles: [], versionChannel: 'release',
        get searchMode() { return mode.current; },
    };
    const { value, destroy } = mount(() => useProfileSearchModel(args));
    assert.equal(value.candidateVersions.length, 2);
    intent.current = { empty: false, nameTokens: [], verHint: '1.20' };
    assert.equal(value.candidateVersions[0].id, '1.20');
    mode.current = 'modpack';
    assert.deepEqual(value.navItems, ['modpack:0']);
    destroy();
});

test('version channels are exclusive even when snapshot is typed as a query', async () => {
    const { useProfileSearchModel } = await load('useProfileSearchModel');
    const channel = reactive('release');
    const intent = reactive({ empty: true, nameTokens: [] });
    const { value, destroy } = mount(() => useProfileSearchModel({
        creating: false, get intent() { return intent.current; },
        get versionChannel() { return channel.current; },
        manifest: { versions: [
            { id: '1.21', type: 'release' }, { id: '24w33a', type: 'snapshot' },
            { id: 'old', type: 'old_alpha' },
        ] },
        matchedProfiles: [], modpackResults: [], profiles: [], searchMode: 'profile',
    }));
    assert.deepEqual(value.candidateVersions.map(version => version.id), ['1.21']);
    channel.current = 'snapshot';
    assert.deepEqual(value.candidateVersions.map(version => version.id), ['24w33a']);
    intent.current = { empty: false, isSnap: true, nameTokens: [] };
    channel.current = 'release';
    assert.deepEqual(value.candidateVersions, [], 'a snapshot query must not offer release versions');
    destroy();
});

test('legacy snapshot preferences migrate to one explicit version channel', async () => {
    const { mergeSettings } = await load('useSettings');
    assert.equal(mergeSettings({}).game.versionChannel, 'release');
    const migrated = mergeSettings({ game: { showSnapshots: true } });
    assert.equal(migrated.game.versionChannel, 'snapshot');
    assert.equal(Object.hasOwn(migrated.game, 'showSnapshots'), false);
    assert.equal(mergeSettings({ game: { showSnapshots: true, versionChannel: 'release' } }).game.versionChannel, 'release');
    assert.equal(mergeSettings({ game: { versionChannel: 'invalid' } }).game.versionChannel, 'release');
});

test('recency cannot turn an unrelated version into a search match', async () => {
    const { useProfileSearchModel } = await load('useProfileSearchModel');
    const intent = reactive({ empty: false, nameTokens: ['mp'] });
    const { value, destroy } = mount(() => useProfileSearchModel({
        creating: false, get intent() { return intent.current; }, versionChannel: 'release',
        manifest: { versions: [{ id: '1.21', type: 'release', releaseTime: '2026-01-01T00:00:00Z' }] },
        matchedProfiles: [], modpackResults: [], profiles: [], searchMode: 'profile',
    }));
    assert.deepEqual(value.candidateVersions, []);
    intent.current = { empty: false, nameTokens: ['1.21'] };
    assert.deepEqual(value.candidateVersions.map(version => version.id), ['1.21']);
    destroy();
});

test('modpack search ignores stale responses and clears searching when inactive', async context => {
    context.mock.timers.enable({ apis: ['setTimeout'] });
    const { useModpackSearch } = await load('navigation/useModpackSearch');
    const active = reactive(true);
    const query = reactive('old');
    let results = [];
    let searching = false;
    const old = deferred();
    const fresh = deferred();
    environment.invoke = async (_command, args) => args.query === 'old' ? old.promise : fresh.promise;
    const { destroy } = mount(() => useModpackSearch({
        get active() { return active.current; }, get query() { return query.current; },
        setModpackResults: next => { results = apply(results, next); },
        setModpackSearching: next => { searching = apply(searching, next); },
    }));
    context.mock.timers.tick(350);
    query.current = 'new';
    flushSync();
    context.mock.timers.tick(350);
    fresh.resolve([{ project_id: 'new' }]);
    await settle();
    old.resolve([{ project_id: 'old' }]);
    await settle();
    assert.equal(results[0].project_id, 'new');
    active.current = false;
    flushSync();
    assert.deepEqual(results, []);
    assert.equal(searching, false);
    destroy();
});

test('crash parsing does not loop when its parsed result updates state', async () => {
    const { useCrashToastParser } = await load('useCrashToastParser');
    const lines = ['crash'];
    const analyses = reactive({ first: { profile_id: 'first', lines, parsed: null } });
    let calls = 0;
    environment.invoke = async () => { calls++; return { diagnosis: { summary: 'Parsed' } }; };
    const { destroy } = mount(() => useCrashToastParser({
        crashToastProfileId: 'first', get crashAnalyses() { return analyses.current; },
        setCrashAnalyses: update => { analyses.current = apply(analyses.current, update); },
    }));
    await settle();
    await settle();
    assert.equal(calls, 1);
    assert.equal(analyses.current.first.parsed.diagnosis.summary, 'Parsed');
    destroy();
});

test('deletion retains its original target when confirmation changes during the request', async () => {
    const { useProfileDeletion } = await load('useProfileDeletion');
    const pending = deferred();
    let profiles = [{ id: 'first' }, { id: 'second' }];
    environment.invoke = async () => pending.promise;
    const { value, destroy } = mount(() => useProfileDeletion({
        get profiles() { return profiles; }, setProfiles: update => { profiles = apply(profiles, update); },
        activeProfileId: null, setActiveProfileId: () => {}, isProfileBusy: () => false, profileRunStates: {},
    }));
    value.handleDeleteProfile('first');
    const deletion = value.handleDeleteProfileConfirm();
    value.handleDeleteProfile('second');
    pending.resolve();
    await deletion;
    assert.deepEqual(profiles.map(profile => profile.id), ['second']);
    assert.equal(value.deleteConfirmId, 'second');
    destroy();
});

test('settings getters and reference stay synchronized and pending save is cancelled on destroy', async context => {
    context.mock.timers.enable({ apis: ['setTimeout'] });
    const { useSettings } = await load('useSettings');
    const calls = [];
    environment.invoke = async command => { calls.push(command); };
    const { value, destroy } = mount(useSettings);
    value.updateSettings(previous => ({ ...previous, game: { ...previous.game, memoryMb: 4096 } }));
    assert.equal(value.settings.game.memoryMb, 4096);
    assert.equal(value.settingsRef.current.game.memoryMb, 4096);
    destroy();
    context.mock.timers.tick(500);
    assert.deepEqual(calls, []);
});

test('keyboard navigation wraps, respects modals, reads fresh targets and removes listeners', async context => {
    const key = keyboard(context);
    const { useMainCommandKeydown } = await load('navigation/useMainCommandKeydown');
    const launches = [];
    const args = {
        activeView: 'main', showAccounts: false, deleteConfirmId: null, logoutConfirm: false, showAdvConfirm: false,
        configProfileId: null, loginModalOpenRef: { current: false }, inputRef: { current: null }, createInputRef: { current: null },
        navDirRef: { current: 'none' }, navIndexRef: { current: 0 }, navItemsRef: { current: ['p:first', 'p:second'] }, navElemsRef: { current: new Map() },
        searchMode: 'profile', searchValue: '', creating: null, isMacOS: false, profiles: [{ id: 'first' }, { id: 'second' }],
        savedAuth: {}, hoverProfileId: null, hoverVersionKey: null, versionDialogModpack: null,
        setNavIndex: value => { args.navIndexRef.current = apply(args.navIndexRef.current, value); },
        setHoverProfileId: () => {}, setHoverModpackIdx: () => {}, setHoverVersionKey: () => {},
        setActiveProfileId: value => { args.activeProfileId = value; },
        setActiveView: value => { args.activeView = value; },
        isProfileBusy: () => false, handleLaunchGame: profile => { launches.push(profile.id); },
    };
    const { destroy } = mount(() => useMainCommandKeydown(args));
    key('ArrowUp');
    assert.equal(args.navIndexRef.current, 1);
    key('ArrowDown', { repeat: true });
    assert.equal(args.navIndexRef.current, 1);
    key('ArrowDown');
    assert.equal(args.navIndexRef.current, 0);
    args.navItemsRef.current = ['p:fresh'];
    args.profiles = [{ id: 'fresh' }];
    key('Enter');
    assert.deepEqual(launches, ['fresh']);
    args.showAccounts = true;
    key('Enter');
    assert.equal(launches.length, 1);
    destroy();
    key(',', { ctrlKey: true });
    assert.equal(args.activeView, 'main');
});

test('Tab switches version channel without changing search mode and leaves other Tab shortcuts alone', async context => {
    const key = keyboard(context);
    const { useMainCommandKeydown } = await load('navigation/useMainCommandKeydown');
    let toggles = 0;
    const args = {
        activeView: 'main', searchMode: 'profile', creating: null, versionDialogModpack: null,
        showAccounts: false, deleteConfirmId: null, logoutConfirm: false, showAdvConfirm: false, configProfileId: null,
        loginModalOpenRef: { current: false }, inputRef: { current: null },
        onToggleVersionChannel: () => { toggles++; },
        setSearchMode: () => assert.fail('Tab must not enter Modpack search'),
    };
    const { destroy } = mount(() => useMainCommandKeydown(args));
    assert.equal(key('Tab').defaultPrevented, true);
    assert.equal(toggles, 1);
    assert.equal(key('Tab', { repeat: true }).defaultPrevented, true);
    assert.equal(toggles, 1, 'holding Tab must not repeatedly toggle');
    key('Tab');
    assert.equal(toggles, 2);
    for (const modifier of ['shiftKey', 'ctrlKey', 'metaKey', 'altKey']) {
        assert.equal(key('Tab', { [modifier]: true }).defaultPrevented, false);
    }
    for (const patch of [
        { activeView: 'settings' }, { showAccounts: true }, { deleteConfirmId: 'profile' },
        { logoutConfirm: true }, { showAdvConfirm: true }, { configProfileId: 'profile' },
        { creating: {} }, { versionDialogModpack: {} }, { searchMode: 'modpack' },
        { loginModalOpenRef: { current: true } },
    ]) {
        const previous = Object.fromEntries(Object.keys(patch).map(name => [name, args[name]]));
        Object.assign(args, patch);
        assert.equal(key('Tab').defaultPrevented, false);
        Object.assign(args, previous);
    }
    assert.equal(toggles, 2);
    destroy();
    key('Tab');
    assert.equal(toggles, 2, 'destroy removes the shortcut');
});

test('Space command completion is limited to unmodified non-IME input at the end of the query', async context => {
    const key = keyboard(context);
    const { useMainCommandKeydown } = await load('navigation/useMainCommandKeydown');
    let completions = 0;
    let accepted = true;
    window.selectionStart = 2;
    window.selectionEnd = 2;
    const args = {
        activeView: 'main', searchMode: 'profile', searchValue: 'mp', creating: null, versionDialogModpack: null,
        showAccounts: false, deleteConfirmId: null, logoutConfirm: false, showAdvConfirm: false, configProfileId: null,
        loginModalOpenRef: { current: false }, inputRef: { current: window },
        onCompleteSearchCommand: () => { completions++; return accepted; },
    };
    const { destroy } = mount(() => useMainCommandKeydown(args));
    assert.equal(key(' ').defaultPrevented, true);
    assert.equal(completions, 1);
    for (const flag of ['isComposing', 'repeat', 'ctrlKey', 'metaKey', 'altKey', 'shiftKey']) {
        assert.equal(key(' ', { [flag]: true }).defaultPrevented, false);
    }
    for (const patch of [{ searchMode: 'modpack' }, { creating: {} }, { versionDialogModpack: {} }, { configProfileId: 'profile' }, { activeView: 'settings' }]) {
        const previous = Object.fromEntries(Object.keys(patch).map(name => [name, args[name]]));
        Object.assign(args, patch);
        assert.equal(key(' ').defaultPrevented, false);
        Object.assign(args, previous);
    }
    window.selectionStart = 0;
    assert.equal(key(' ').defaultPrevented, false, 'middle edits must insert ordinary spaces');
    window.selectionStart = 2;
    window.selectionEnd = 0;
    assert.equal(key(' ').defaultPrevented, false, 'selected text must not become a command');
    assert.equal(completions, 1);
    window.selectionEnd = 2;
    accepted = false;
    assert.equal(key(' ').defaultPrevented, false, 'ordinary queries retain spaces');
    assert.equal(completions, 2);
    destroy();
    key(' ');
    assert.equal(completions, 2);
});

test('Backspace exits only an empty Modpack input and shares Escape cleanup', async context => {
    const key = keyboard(context);
    const { useMainCommandKeydown } = await load('navigation/useMainCommandKeydown');
    const { useEscapeHandling } = await load('useEscapeHandling');
    let exits = 0;
    const args = {
        activeView: 'main', isMainView: true, searchMode: 'modpack', searchValue: '', creating: null,
        versionDialogModpack: null, versionDialogOpen: false, showAccounts: false, deleteConfirmId: null,
        logoutConfirm: false, showAdvConfirm: false, configProfileId: null,
        loginModalOpenRef: { current: false }, inputRef: { current: window },
        onExitSearchMode: () => { exits++; },
        setSearchValue: value => { args.searchValue = apply(args.searchValue, value); },
    };
    const { destroy } = mount(() => { useMainCommandKeydown(args); useEscapeHandling(args); });
    assert.equal(key('Backspace').defaultPrevented, true);
    assert.equal(exits, 1);
    for (const flag of ['isComposing', 'repeat', 'ctrlKey', 'metaKey', 'altKey', 'shiftKey']) {
        assert.equal(key('Backspace', { [flag]: true }).defaultPrevented, false);
    }
    for (const patch of [
        { searchValue: 'x' }, { searchValue: ' ' }, { searchMode: 'profile' }, { creating: {} },
        { versionDialogModpack: {} }, { configProfileId: 'profile' }, { activeView: 'settings' },
    ]) {
        const previous = Object.fromEntries(Object.keys(patch).map(name => [name, args[name]]));
        Object.assign(args, patch);
        assert.equal(key('Backspace').defaultPrevented, false);
        Object.assign(args, previous);
    }
    assert.equal(exits, 1);
    args.searchValue = 'query';
    key('Escape');
    assert.equal(args.searchValue, '');
    assert.equal(exits, 1);
    key('Escape');
    assert.equal(exits, 2, 'Escape calls the same mode exit action');
    destroy();
    key('Backspace');
    assert.equal(exits, 2);
});

test('Escape precedence remains current and delayed focus is cancelled on destroy', async context => {
    context.mock.timers.enable({ apis: ['setTimeout'] });
    const key = keyboard(context);
    const { useEscapeHandling } = await load('useEscapeHandling');
    let focuses = 0;
    const args = {
        activeView: 'mods', isMainView: false, modsProfileId: 'first', modsReturnView: 'library',
        searchValue: '', searchMode: 'profile', profileCtxMenuOpen: true, showAccounts: false, versionDialogOpen: false,
        logoutConfirm: false, deleteConfirmId: null, configProfileId: null, creating: null,
        navDirRef: { current: 'none' }, loginModalOpenRef: { current: false },
        inputRef: { current: { focus() { focuses++; }, select() {} } },
        closeProfileCtxMenu: () => { args.profileCtxMenuOpen = false; },
        setModsProfileId: value => { args.modsProfileId = value; }, setActiveView: value => { args.activeView = value; },
    };
    const { destroy } = mount(() => useEscapeHandling(args));
    key('Escape');
    assert.equal(args.profileCtxMenuOpen, false);
    assert.equal(args.modsProfileId, 'first');
    key('Escape');
    assert.equal(args.activeView, 'library');
    assert.equal(args.modsProfileId, null);
    destroy();
    context.mock.timers.tick(100);
    assert.equal(focuses, 0);
});

test('stopped game exits skip crash analysis and event listeners are disposed', async () => {
    const { useGameEvents } = await load('useGameEvents');
    environment.listeners = new Map();
    const calls = [];
    environment.invoke = async command => { calls.push(command); };
    const { state } = launchFixture();
    // launchFixture configures invoke too; use this test's command recorder.
    environment.invoke = async command => { calls.push(command); };
    state.profileRunStates = { first: { phase: 'running', dlProgress: null } };
    state.launchStartedAtRef.current.set('first', 1);
    const { destroy } = mount(() => useGameEvents({ ...state, setDebugInfo: () => {} }));
    await settle();
    const exits = environment.listeners.get('game://exit');
    assert.equal(exits.size, 1);
    await [...exits][0]({ payload: { profile_id: 'first', exit_code: 1, stopped: true } });
    assert.equal(calls.includes('get_latest_crash_analysis'), false);
    assert.equal(state.launchStartedAtRef.current.size, 0);
    destroy();
    await settle();
    assert.equal([...environment.listeners.values()].every(listeners => listeners.size === 0), true);
});

function accountFixture() {
    const state = { savedAuth: { username: 'Old', uuid: 'old' }, loginState: 'idle', loginModalOpen: false,
        errorMessage: undefined, skinImgError: false };
    const settingsRef = { current: { accounts: [state.savedAuth], activeAccountUuid: 'old' } };
    const params = { settingsRef, loginWindowTitle: 'Sign in', get savedAuth() { return state.savedAuth; },
        updateSettings: update => { settingsRef.current = update(settingsRef.current); } };
    for (const key of Object.keys(state)) params['set' + key[0].toUpperCase() + key.slice(1)] = update => {
        state[key] = apply(state[key], update);
    };
    environment.currentWindow = { show: async () => {}, setFocus: async () => {} };
    return { state, params };
}

test('late sign-in completion cannot restore UI after logout; duplicate clicks do not start another login', async () => {
    const { useAccountActions } = await load('useAccountActions');
    const login = deferred();
    const calls = [];
    environment.invoke = async command => {
        calls.push(command);
        if (command === 'start_webview_login') return login.promise;
        if (command === 'get_saved_auth') return { username: 'Late', uuid: 'late' };
    };
    const { state, params } = accountFixture();
    const { value, destroy } = mount(() => useAccountActions(params));
    const pending = value.handleWebviewLogin();
    await value.handleWebviewLogin();
    assert.equal(calls.filter(command => command === 'start_webview_login').length, 1);
    await value.handleLogout();
    login.resolve({ username: 'Late', uuid: 'late' });
    await pending;
    assert.equal(state.savedAuth, null);
    assert.equal(state.loginModalOpen, false);
    assert.equal(state.loginState, 'idle');
    destroy();
});

test('account selection displays the backend snapshot, not caller-supplied account details', async () => {
    const { useAccountActions } = await load('useAccountActions');
    const snapshot = { username: 'Backend', uuid: 'selected' };
    environment.invoke = async command => command === 'get_saved_auth' ? snapshot : undefined;
    const { state, params } = accountFixture();
    const { value, destroy } = mount(() => useAccountActions(params));
    await value.handleSwitchAccount({ username: 'Untrusted display name', uuid: 'selected' });
    assert.deepEqual(state.savedAuth, snapshot);
    destroy();
});
