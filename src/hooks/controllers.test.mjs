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
    if (globalThis.__hikyouControllerTest.listen) return globalThis.__hikyouControllerTest.listen(name, callback);
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

const modRow = (patch = {}) => ({ key: 'modrinth:sodium', projectId: 'sodium', objectId: null,
    name: 'Sodium', iconUrl: null, desired: 'excluded', source: 'recommended', active: null,
    reason: 'excluded', canEnable: true, canExclude: true, canInherit: true, ...patch });
const modSnapshot = (profileId = 'first', revision = 'r1', rows = [modRow()]) => ({
    profileId, revision, activeRevisionId: 'active', target: { mcVersion: '1.21', loader: 'fabric', loaderVersion: null }, rows,
    requested: { intent: { schema: 1, profileId, requirements: [], excludedProjects: ['sodium'] },
        activeRevisionId: 'active', pending: false, preparation: null },
});
const modDecision = (patch = {}) => ({ id: 'proposal', profileId: 'first',
    target: '1.22', phase: 'pending',
    omitted: [{ key: 'modrinth:sodium', name: 'Sodium' }], baselineTarget: 'baseline', expectedRevision: 'r1', ...patch });

test('Mod lifecycle choices retain absent exclusions and enforce backend capabilities', async () => {
    const { toggleChoice, choiceAllowed, visibleDecisions, decisionActions } = await import(await moduleUrl('src/modLifecycle.ts'));
    const absent = modRow();
    assert.equal(absent.active, null);
    assert.equal(toggleChoice(absent), 'include');
    assert.equal(toggleChoice(modRow({ reason: 'legacyExcluded', desired: 'notRequested' })), 'include');
    assert.equal(toggleChoice(modRow({ reason: 'defaultOff', desired: 'notRequested' })), 'include');
    assert.equal(toggleChoice(modRow({ desired: 'required', reason: 'pending' })), 'exclude');
    assert.equal(toggleChoice(modRow({ desired: 'notRequested', reason: 'dependency', active: { enabled: true }, canExclude: false })), null);
    for (const choice of ['include', 'exclude', 'inherit']) {
        assert.equal(choiceAllowed(absent, choice), true);
        assert.equal(choiceAllowed(modRow({ canEnable: false, canExclude: false, canInherit: false }), choice), false);
    }
    const decisions = ['pending', 'deferred', 'accepted', 'revoked'].map(phase => modDecision({ id: phase, phase }));
    assert.deepEqual(visibleDecisions(decisions, false).map(item => item.phase), ['pending']);
    assert.deepEqual(visibleDecisions(decisions, true).map(item => item.phase), ['pending', 'deferred', 'accepted']);
    assert.deepEqual(decisionActions('pending'), ['defer', 'accept']);
    assert.deepEqual(decisionActions('deferred'), ['accept']);
    assert.deepEqual(decisionActions('accepted'), ['revoke']);
    assert.deepEqual(decisionActions('revoked'), []);
});

test('Mod failure details retain bounded causes and operations without exposing arbitrary backend text', async () => {
    const { modLifecycleFailure } = await import(await moduleUrl('src/modLifecycleFailure.ts'));
    const operation = 'set_profile_mod_choice';
    for (const [message, code] of [
        ['Mod choices changed; refresh before applying this action', 'stale'],
        ['Mod decision inputs changed; refresh before answering', 'stale'],
        ['profile_mismatch', 'profile_mismatch'],
        ['Mod decision not found', 'missing'],
        ['Exact local Mod object is unavailable; import it again', 'local_object_missing'],
        ['Cannot read Mod decision: C:\\private\\state.json; token=secret', 'state_unreadable'],
        ['Invalid Mod decision; changes are blocked: private value', 'state_invalid'],
        ['Profile is running: private profile', 'busy'],
        ['Permission denied: /private/path', 'permission_denied'],
        ['Mod provider unavailable: https://private.invalid/?token=secret', 'provider_unavailable'],
        ['mod_resolution_limit: search states; private log', 'resolution_limit'],
        ['mod_resolution_unsatisfiable: private log', 'resolution_unsatisfiable'],
    ]) {
        const failure = modLifecycleFailure(new Error(message), operation);
        assert.deepEqual(failure, { operation, code, kind: 'error' });
        assert.doesNotMatch(JSON.stringify(failure), /private|secret|state\.json|https:/);
    }
    for (const reason of ['C:\\private\\name token=secret', '<script>private</script>', { message: 'private', code: 'secret' }, 'x'.repeat(100000)]) {
        const failure = modLifecycleFailure(reason, 'get_mod_decisions');
        assert.equal(failure.code, 'unknown');
        assert.ok(JSON.stringify(failure).length < 128);
        assert.doesNotMatch(JSON.stringify(failure), /private|secret|script/);
    }
});

test('deferred decisions do not send revoke because waiting is not active permission', async () => {
    const { useModDecisions } = await load('useModDecisions');
    const calls = [];
    environment.invoke = async (command, args) => { calls.push([command, args]); return [modDecision({ phase: 'deferred' })]; };
    const { value, destroy } = mount(useModDecisions);
    await settle();
    await value.answer(value.decisions[0], 'revoke');
    assert.deepEqual(calls.map(([command]) => command), ['get_mod_decisions']);
    destroy();
});

test('known decisions can be answered during a background read without replaying its stale response', async () => {
    const { useModDecisions } = await load('useModDecisions');
    const background = deferred();
    const calls = [];
    let reads = 0;
    let current = modDecision();
    environment.invoke = async (command, args) => {
        calls.push([command, args]);
        if (command === 'get_mod_decisions') {
            if (++reads === 2) return background.promise;
            return [{ ...current }];
        }
        current = modDecision({ phase: 'deferred', expectedRevision: 'r2' });
    };
    const { value, destroy } = mount(useModDecisions);
    await settle();
    const known = value.decisions[0];
    const refreshing = value.refresh();
    assert.equal(value.loading, true);
    await value.answer(known, 'defer');
    assert.deepEqual(calls.find(([command]) => command === 'answer_mod_decision'), ['answer_mod_decision', {
        profileId: 'first', proposalId: 'proposal', expectedRevision: 'r1', action: 'defer',
    }]);
    assert.equal(value.decisions[0].phase, 'deferred');
    background.resolve([known]);
    await refreshing;
    assert.equal(value.decisions[0].phase, 'deferred');
    assert.equal(value.decisions[0].expectedRevision, 'r2');
    assert.equal(calls.filter(([command]) => command === 'answer_mod_decision').length, 1);
    destroy();
});

test('Mod snapshot uses one authoritative read and choice keys, with no optimistic change or duplicate mutation', async () => {
    const { useModSnapshot } = await load('useModSnapshot');
    const pending = deferred();
    const calls = [];
    environment.invoke = async (command, args) => {
        calls.push([command, args]);
        if (command === 'get_profile_mod_snapshot') return modSnapshot();
        if (command === 'set_profile_mod_choice') return pending.promise;
        assert.fail(`Unexpected legacy command: ${command}`);
    };
    const { value, destroy } = mount(() => useModSnapshot(() => 'first'));
    await settle();
    assert.deepEqual(calls.map(([command]) => command), ['get_profile_mod_snapshot']);
    assert.equal(value.snapshot.rows[0].key, 'modrinth:sodium');
    const change = value.choose('modrinth:sodium', 'include');
    await value.choose('modrinth:sodium', 'include');
    assert.equal(value.busy, true);
    value.retry();
    await settle();
    assert.equal(value.busy, true, 'retry cannot unlock an in-flight write');
    assert.equal(value.snapshot.rows[0].desired, 'excluded');
    assert.deepEqual(calls[1], ['set_profile_mod_choice', { key: 'modrinth:sodium', choice: 'include', profileId: 'first', expectedRevision: 'r1' }]);
    pending.resolve(modSnapshot('first', 'r2', [modRow({ desired: 'required', reason: 'pending' })]));
    await change;
    assert.equal(calls.length, 2);
    assert.equal(value.snapshot.revision, 'r2');
    assert.equal(value.snapshot.rows[0].desired, 'required');
    assert.equal(value.busy, false);
    destroy();
    await settle();
    for (const name of ['mods://prepared', 'mods://requests-changed', 'mods://decisions-changed']) assert.equal(environment.listeners.get(name)?.size, 0);
});

test('stale Mod choices reload without replay and use the new revision only on a fresh user action', async () => {
    const { useModSnapshot } = await load('useModSnapshot');
    const calls = [];
    let reads = 0;
    environment.invoke = async (command, args) => {
        calls.push([command, args]);
        if (command === 'get_profile_mod_snapshot') return modSnapshot('first', ++reads === 1 ? 'r1' : 'r2');
        throw new Error('stale revision with private local path');
    };
    const { value, destroy } = mount(() => useModSnapshot(() => 'first'));
    await settle();
    await value.choose('modrinth:sodium', 'include');
    assert.equal(value.changeFailed, true);
    assert.equal(value.failures[0].operation, 'set_profile_mod_choice');
    assert.doesNotMatch(JSON.stringify(value.failures), /private local path/);
    assert.equal(value.snapshot.revision, 'r2');
    assert.deepEqual(calls.map(([command]) => command), ['get_profile_mod_snapshot', 'set_profile_mod_choice', 'get_profile_mod_snapshot']);
    await value.choose('modrinth:sodium', 'inherit');
    assert.equal(calls[3][1].expectedRevision, 'r2');
    assert.equal(calls[3][1].choice, 'inherit');
    destroy();
});

test('a delayed choice response cannot replace a newer event snapshot', async () => {
    const { useModSnapshot } = await load('useModSnapshot');
    const pending = deferred();
    const calls = [];
    let revision = 'r1';
    environment.invoke = async (command, args) => {
        calls.push([command, args]);
        if (command === 'get_profile_mod_snapshot') return modSnapshot('first', revision);
        return pending.promise;
    };
    const { value, destroy } = mount(() => useModSnapshot(() => 'first'));
    await settle();
    const change = value.choose('modrinth:sodium', 'include');
    revision = 'r3';
    for (const listener of environment.listeners.get('mods://prepared')) listener({ payload: { profileId: 'first' } });
    await settle();
    assert.equal(value.snapshot.revision, 'r3');
    pending.resolve(modSnapshot('first', 'r2'));
    await change;
    assert.equal(value.snapshot.revision, 'r3');
    assert.equal(calls.filter(([command]) => command === 'set_profile_mod_choice').length, 1);
    destroy();
});

test('unknown Mod snapshot blocks writes, exposes retry and rejects mismatched profile data', async () => {
    const { useModSnapshot } = await load('useModSnapshot');
    const calls = [];
    let outcome = 'error';
    environment.invoke = async (command, args) => {
        calls.push([command, args]);
        if (command !== 'get_profile_mod_snapshot') assert.fail('Unknown state cannot mutate');
        if (outcome === 'error') throw new Error('unreadable private state');
        return modSnapshot(outcome === 'mismatch' ? 'other' : 'first');
    };
    const { value, destroy } = mount(() => useModSnapshot(() => 'first'));
    await settle();
    assert.equal(value.loadFailed, true);
    await value.choose('modrinth:sodium', 'include');
    await value.install({ projectId: 'other' });
    assert.equal(calls.length, 1);
    outcome = 'mismatch';
    await value.refresh();
    assert.equal(value.loadFailed, true);
    assert.equal(value.snapshot, null);
    outcome = 'good';
    value.retry();
    await settle();
    assert.equal(value.loadFailed, false);
    assert.equal(value.snapshot.profileId, 'first');
    destroy();
});

test('Mod snapshot refreshes all lifecycle events, ignores other profiles and late profile results', async () => {
    const { useModSnapshot } = await load('useModSnapshot');
    const late = deferred();
    const selected = mutable_source('first');
    let reads = 0;
    environment.invoke = async (_command, args) => {
        reads++;
        return reads === 1 ? late.promise : modSnapshot(args.profileId, `r${reads}`);
    };
    const { value, destroy } = mount(() => useModSnapshot(() => get(selected)));
    await settle();
    set(selected, 'second');
    await settle();
    late.resolve(modSnapshot('first'));
    await settle();
    assert.equal(value.snapshot.profileId, 'second');
    for (const name of ['mods://prepared', 'mods://requests-changed', 'mods://decisions-changed']) {
        const before = reads;
        for (const listener of environment.listeners.get(name)) listener({ payload: { profileId: 'first' } });
        await settle();
        assert.equal(reads, before);
        for (const listener of environment.listeners.get(name)) listener({ payload: { profileId: 'second' } });
        await settle();
        assert.equal(reads, before + 1);
    }
    destroy();
});

test('Mod controllers reject actions between a profile switch and effect cleanup', async () => {
    const { useModSnapshot } = await load('useModSnapshot');
    const { useModDecisions } = await load('useModDecisions');
    const calls = [];
    const selected = mutable_source('first');
    environment.invoke = async (command, args) => {
        calls.push([command, args]);
        if (command === 'get_profile_mod_snapshot') return modSnapshot(args.profileId);
        if (command === 'get_mod_decisions') return [modDecision({ profileId: args.profileId })];
        assert.fail('stale-profile UI cannot mutate');
    };
    const snapshot = mount(() => useModSnapshot(() => get(selected)));
    const decisions = mount(() => useModDecisions(() => get(selected)));
    await settle();
    const proposal = decisions.value.decisions[0];
    set(selected, 'second');
    const include = snapshot.value.choose('modrinth:sodium', 'include');
    const answer = decisions.value.answer(proposal, 'accept');
    await Promise.all([include, answer]);
    await settle();
    assert.equal(calls.every(([command]) => command.startsWith('get_')), true);
    assert.equal(snapshot.value.snapshot.profileId, 'second');
    assert.equal(decisions.value.decisions[0].profileId, 'second');
    snapshot.destroy(); decisions.destroy();
});

test('Mod decisions answer using backend revisions and retain durable defer, accept and revoke state', async () => {
    const { useModDecisions } = await load('useModDecisions');
    let persisted = modDecision();
    const calls = [];
    let reply = deferred();
    environment.invoke = async (command, args) => {
        calls.push([command, args]);
        if (command === 'get_mod_decisions') return [{ ...persisted }];
        await reply.promise;
        persisted = { ...persisted, phase: { defer: 'deferred', accept: 'accepted', revoke: 'revoked' }[args.action], expectedRevision: persisted.expectedRevision + 'x' };
    };
    let mounted = mount(() => useModDecisions(() => 'first'));
    await settle();
    for (const [action, phase] of [['defer', 'deferred'], ['accept', 'accepted'], ['revoke', 'revoked']]) {
        const before = mounted.value.decisions[0];
        reply = deferred();
        const answer = mounted.value.answer(before, action);
        assert.equal(mounted.value.decisions[0].phase, before.phase);
        await mounted.value.answer(before, action);
        const last = calls.at(-1);
        assert.deepEqual(last, ['answer_mod_decision', { profileId: 'first', proposalId: 'proposal', expectedRevision: before.expectedRevision, action }]);
        reply.resolve();
        await answer;
        assert.equal(mounted.value.decisions[0].phase, phase);
        mounted.destroy();
        mounted = mount(() => useModDecisions(() => 'first'));
        await settle();
        assert.equal(mounted.value.decisions[0].phase, phase, 'phase survives remount');
    }
    assert.equal(calls.filter(([command]) => command === 'answer_mod_decision').length, 3);
    mounted.destroy();
});

test('decision failures remain visible, reload without replay, and block answers after a failed read', async () => {
    const { useModDecisions } = await load('useModDecisions');
    let reads = 0;
    let failRead = false;
    const calls = [];
    environment.invoke = async (command, args) => {
        calls.push([command, args]);
        if (command === 'get_mod_decisions') {
            reads++;
            if (failRead) throw new Error('private raw state');
            return [modDecision({ expectedRevision: `r${reads}` })];
        }
        throw new Error('stale decision');
    };
    const { value, destroy } = mount(useModDecisions);
    await settle();
    const old = value.decisions[0];
    await value.answer(old, 'accept');
    assert.equal(value.answerFailed, true);
    assert.equal(value.decisions[0].expectedRevision, 'r2');
    assert.deepEqual(calls.map(([command]) => command), ['get_mod_decisions', 'answer_mod_decision', 'get_mod_decisions']);
    await value.answer(old, 'accept');
    assert.equal(calls.length, 3, 'old proposal is not replayed');
    failRead = true;
    await value.refresh();
    assert.equal(value.loadFailed, true);
    await value.answer(value.decisions[0], 'accept');
    assert.equal(calls.length, 4);
    failRead = false;
    value.retry();
    await settle();
    assert.equal(value.loadFailed, false);
    destroy();
});

test('main Mod decisions refresh on all lifecycle events and clean up event listeners', async () => {
    const { useModDecisions } = await load('useModDecisions');
    const calls = [];
    environment.invoke = async (command, args) => { calls.push([command, args]); return []; };
    const { destroy } = mount(useModDecisions);
    await settle();
    assert.deepEqual(calls[0], ['get_mod_decisions', { profileId: null }]);
    for (const name of ['mods://prepared', 'mods://requests-changed', 'mods://decisions-changed']) {
        const before = calls.length;
        for (const listener of environment.listeners.get(name)) listener({ payload: { profileId: 'any-profile' } });
        await settle();
        assert.equal(calls.length, before + 1);
    }
    destroy();
    for (const name of ['mods://prepared', 'mods://requests-changed', 'mods://decisions-changed']) assert.equal(environment.listeners.get(name)?.size, 0);
});

test('Mod lifecycle event subscription failures are visible and retry reconnects both controllers', async context => {
    context.after(() => { delete environment.listen; });
    const { useModSnapshot } = await load('useModSnapshot');
    const { useModDecisions } = await load('useModDecisions');
    environment.invoke = async command => command === 'get_profile_mod_snapshot' ? modSnapshot() : [];
    for (const create of [() => useModSnapshot(() => 'first'), useModDecisions]) {
        environment.listen = async () => { throw new Error('event connection failed'); };
        const { value, destroy } = mount(create);
        await settle();
        assert.equal(value.eventsFailed, true);
        assert.equal(value.loadFailed, false, 'usable read is not mistaken for a working subscription');
        delete environment.listen;
        value.retry();
        await settle();
        assert.equal(value.eventsFailed, false);
        for (const name of ['mods://prepared', 'mods://requests-changed', 'mods://decisions-changed']) assert.equal(environment.listeners.get(name)?.size, 1);
        destroy();
    }
});

test('Mod lifecycle subscriptions resolving after disposal are stopped and cannot trigger a read', async context => {
    context.after(() => { delete environment.listen; });
    const { useModSnapshot } = await load('useModSnapshot');
    const { useModDecisions } = await load('useModDecisions');
    let reads = 0;
    environment.invoke = async () => { reads++; assert.fail('disposed controllers cannot fetch'); };
    for (const create of [() => useModSnapshot(() => 'first'), useModDecisions]) {
        const pending = [];
        let stopped = 0;
        environment.listen = async () => {
            const subscription = deferred();
            pending.push(subscription);
            return subscription.promise;
        };
        const { destroy } = mount(create);
        destroy();
        for (const subscription of pending) subscription.resolve(() => stopped++);
        await settle();
        assert.equal(stopped, 3);
    }
    assert.equal(reads, 0);
});

test('Mod lifecycle UI is localized, names omissions, confirms composition risk, and never opens worlds or focuses notices', () => {
    const read = file => readFileSync(`src/${file}`, 'utf8');
    const decisions = read('components/ModDecisions.svelte');
    assert.match(decisions, /composition_warning/);
    assert.match(decisions, /mod_decisions\.confirm_/);
    assert.match(decisions, /decision\.omitted\.map/);
    assert.doesNotMatch(decisions, /\.focus\(|autofocus|<dialog|aria-modal|open_world|upload/i);
    assert.doesNotMatch(decisions, /disabled=\{[^}]*model\.loading/, 'background reads cannot disable known decision buttons');
    const panel = read('components/ModsPanel.svelte');
    assert.doesNotMatch(panel, /get_profile_mods|get_profile_mod_requests|toggle_profile_mod|remove_profile_mod|remove_requested_mod/);
    assert.match(panel, /onEntryInherit/);
    const en = JSON.parse(read('locales/en.json'));
    const ja = JSON.parse(read('locales/ja.json'));
    for (const section of ['mod_lifecycle', 'mod_decisions', 'mod_failures']) assert.deepEqual(Object.keys(en[section]).sort(), Object.keys(ja[section]).sort());
    assert.match(en.mod_decisions.composition_warning, /worlds.*configuration.*Back up/);
    assert.equal(typeof en.mod_requests.decision_required, 'string');
    assert.equal(typeof ja.mod_requests.decision_required, 'string');
    for (const locale of [en, ja]) {
        for (const action of ['defer', 'accept', 'confirm_accept', 'revoke', 'confirm_revoke']) assert.match(locale.mod_decisions[action], /\{\{mods\}\}/);
    }
});

test('Mod removal shortcuts preserve text, caret, IME, controls, scope and held-key boundaries', async () => {
    const { isModRemovalKey, removalStep } = await import(await moduleUrl('src/components/modRemovalKeys.ts'));
    const scope = { blocked: false, query: '', queryInput: true, otherControl: false, selectionStart: 0, selectionEnd: 0 };
    const key = { key: 'Delete', altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, repeat: false, isComposing: false };
    for (const name of ['Delete', 'Backspace']) {
        const event = { ...key, key: name };
        assert.equal(isModRemovalKey(event, scope), true);
        for (const patch of [{ query: 'abc' }, { query: ' ' }, { selectionStart: 1, selectionEnd: 1 }, { selectionStart: 0, selectionEnd: 2 }, { selectionStart: null, selectionEnd: null }, { otherControl: true }, { blocked: true }]) {
            assert.equal(isModRemovalKey(event, { ...scope, ...patch }), false);
        }
        for (const flag of ['repeat', 'isComposing', 'ctrlKey', 'metaKey', 'shiftKey', 'altKey']) {
            assert.equal(isModRemovalKey({ ...event, [flag]: true }, scope), false);
        }
    }
    assert.equal(isModRemovalKey({ ...key, key: 'ArrowLeft', altKey: true }, { ...scope, query: 'installed search' }), true);
    assert.equal(isModRemovalKey({ ...key, key: 'ArrowLeft', altKey: true, repeat: true }, scope), false);
    const armed = removalStep(null, 'first');
    assert.deepEqual(armed, { pending: 'first', remove: false });
    assert.deepEqual(removalStep(armed.pending, 'first'), { pending: null, remove: true });
    assert.deepEqual(removalStep(armed.pending, 'second'), { pending: 'second', remove: false });
    assert.deepEqual(removalStep(null, 'first'), armed, 'cancelled confirmation must arm again');
});

test('Mod icons never use raw remote src and recover across failure and URL changes', async () => {
    const { useModIcon } = await load('useModIcon');
    const old = deferred();
    const current = deferred();
    const calls = [];
    const good = 'data:image/png;base64,AAAA';
    const { value, destroy } = mount(() => useModIcon(async (url, project) => { calls.push([url, project]); return calls.length === 1 ? old.promise : current.promise; }));
    const first = value.load('https://cdn.modrinth.com/data/old.png', 'first');
    assert.equal(value.src, null);
    assert.equal(value.status, 'loading');
    const second = value.load(null, 'second');
    current.resolve({ data_url: good, warning: 'icon_cache_write_failed' });
    await second;
    assert.equal(value.src, good);
    assert.equal(value.cacheWarning, true);
    old.reject('private path or network failure');
    await first;
    assert.equal(value.src, good, 'old rejection must not hide current icon');
    const request = value.request;
    await value.load(null, 'second');
    value.imageFailed(good, request);
    assert.equal(value.src, good, 'old image error must not hide identical newer data');
    value.imageFailed(good, value.request);
    assert.equal(value.src, null);
    assert.equal(value.status, 'decode_failed');
    await value.load(null, 'second');
    assert.equal(value.src, good, 'new load resets image failure');
    assert.deepEqual(calls[1], [null, 'second'], 'missing saved URL uses explicit project metadata only');
    destroy();
});

test('Mod icon failures are bounded diagnostics and late/disposed results are ignored', async () => {
    const { useModIcon } = await load('useModIcon');
    const { value, destroy } = mount(() => useModIcon(async () => { throw new Error('/private/path token'); }));
    await value.load(null, 'known-project');
    assert.equal(value.status, 'unavailable');
    await value.load(null, null);
    assert.equal(value.status, 'missing');
    const pending = deferred();
    const late = mount(() => useModIcon(async () => pending.promise));
    const loading = late.value.load(null, 'known-project');
    late.value.dispose();
    pending.resolve({ data_url: 'https://evil.invalid/icon', warning: null });
    await loading;
    assert.equal(late.value.src, null);
    late.destroy();
    const invalid = mount(() => useModIcon(async () => ({ data_url: 'https://evil.invalid/icon', warning: null })));
    await invalid.value.load(null, 'project');
    assert.equal(invalid.value.status, 'unsupported');
    invalid.destroy();
    destroy();
});

const reportFixture = () => ({
    context: { code: 'mod_resolution_limit', limit: 'search_states_projects', counters: { states: 20001, projects: 128, roots: 12 } },
    title: 'Launch error: mod_resolution_limit',
    body: 'Resolution limit: search states/projects\nNo partial solution accepted.',
});

test('launch reporting prepares locally and cannot open or copy before explicit review', async () => {
    const { useLaunchReport } = await load('useLaunchReport');
    const calls = [];
    const copies = [];
    const { value, destroy } = mount(() => useLaunchReport({
        invoke: async (command, args) => { calls.push([command, args]); return reportFixture(); },
        copy: async text => { copies.push(text); },
    }));
    assert.deepEqual(calls, []);
    await value.open();
    await value.copy();
    await value.prepare('mod_resolution_limit: search states/projects; token=private');
    assert.equal(calls.length, 1);
    assert.equal(calls[0][0], 'prepare_launch_error_report');
    await value.open();
    await value.copy();
    assert.equal(calls.length, 1);
    assert.deepEqual(copies, []);
    value.reviewed = true;
    await value.open();
    assert.deepEqual(calls[1], ['open_launch_error_report', { context: reportFixture().context, reviewed: true }]);
    assert.equal(JSON.stringify(calls[1]).includes('private'), false);
    await value.copy();
    assert.deepEqual(copies, [`${reportFixture().title}\n\n${reportFixture().body}`]);
    await value.prepare('new error');
    assert.equal(value.reviewed, false);
    assert.equal(value.result, null);
    destroy();
});

test('launch report failures stay visible, support fallback, and never expose raw exception text', async () => {
    const { useLaunchReport } = await load('useLaunchReport');
    let failPrepare = true;
    let failCopy = true;
    const { value, destroy } = mount(() => useLaunchReport({
        invoke: async command => {
            if (command === 'prepare_launch_error_report' && !failPrepare) return reportFixture();
            throw new Error('private token and path');
        },
        copy: async () => { if (failCopy) throw new Error('private clipboard error'); },
    }));
    await value.prepare('failure');
    assert.equal(value.failure, 'prepare_failed');
    assert.equal(value.preview, null);
    failPrepare = false;
    await value.prepare('failure');
    value.reviewed = true;
    await value.open();
    assert.equal(value.failure, 'open_failed');
    assert.notEqual(value.preview, null);
    await value.copy();
    assert.equal(value.failure, 'copy_failed');
    assert.equal(value.result, null);
    failCopy = false;
    await value.copy();
    assert.equal(value.failure, null);
    assert.equal(value.result, 'copied');
    destroy();
});

test('launch report ignores stale preparations and does not repeat pending external actions', async () => {
    const { useLaunchReport } = await load('useLaunchReport');
    const pending = deferred();
    let preparations = 0;
    let opens = 0;
    const { value, destroy } = mount(() => useLaunchReport({
        invoke: async command => {
            if (command === 'prepare_launch_error_report') return ++preparations === 1 ? pending.promise : reportFixture();
            opens++;
            return pending.promise;
        },
        copy: async () => {},
    }));
    const stale = value.prepare('old');
    await value.prepare('new');
    value.reviewed = true;
    const open = value.open();
    await value.open();
    assert.equal(opens, 1);
    value.dispose();
    pending.resolve({ ...reportFixture(), title: 'stale' });
    await stale;
    await open;
    assert.equal(value.preview.title, reportFixture().title);
    assert.equal(value.result, null);
    destroy();
});
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
    value.applyLoadedSettings(value.settings);
    value.updateSettings(previous => ({ ...previous, game: { ...previous.game, memoryMb: 4096 } }));
    assert.equal(value.settings.game.memoryMb, 4096);
    assert.equal(value.settingsRef.current.game.memoryMb, 4096);
    destroy();
    context.mock.timers.tick(500);
    assert.deepEqual(calls, []);
});

test('retirement blocks duplicate submissions, keeps failures visible and supports undo', async () => {
    const { useProfileDeletion } = await load('useProfileDeletion');
    let profiles = [{ id: 'first', name: 'First' }];
    const first = deferred();
    const calls = [];
    environment.invoke = async command => { calls.push(command); return first.promise; };
    const { value, destroy } = mount(() => useProfileDeletion({
        get profiles() { return profiles; }, setProfiles: update => { profiles = apply(profiles, update); },
        activeProfileId: null, setActiveProfileId: () => {}, isProfileBusy: () => false, profileRunStates: {},
    }));
    value.handleDeleteProfile('first');
    const pending = value.handleDeleteProfileConfirm();
    await value.handleDeleteProfileConfirm();
    assert.equal(calls.length, 1);
    first.reject('in use');
    await pending;
    assert.equal(value.error, 'in use');
    assert.equal(profiles.length, 1);
    environment.invoke = async command => { calls.push(command); return command === 'restore_retired_profile' ? { id: 'first', name: 'First' } : undefined; };
    await value.handleDeleteProfileConfirm();
    assert.equal(value.retired, true);
    assert.equal(profiles.length, 0);
    assert.equal(value.error, null);
    await value.handleRestore();
    assert.equal(value.deleteConfirmId, null);
    assert.equal(profiles[0].id, 'first');
    destroy();
});

test('settings saves remain ordered and failure stays visible until a successful retry', async context => {
    context.mock.timers.enable({ apis: ['setTimeout'] });
    const { useSettings } = await load('useSettings');
    const first = deferred();
    const calls = [];
    environment.invoke = async (_command, args) => {
        calls.push(args.settings.game.memoryMb);
        if (calls.length === 1) return first.promise;
        if (calls.length === 2) throw new Error('disk full');
    };
    const { value, destroy } = mount(useSettings);
    value.updateSettings(previous => ({ ...previous, game: { ...previous.game, memoryMb: 1024 } }));
    context.mock.timers.tick(500);
    await settle();
    assert.deepEqual(calls, []); // No writes before a successful read.
    value.applyLoadedSettings(value.settings);
    value.updateSettings(previous => ({ ...previous, game: { ...previous.game, memoryMb: 4096 } }));
    context.mock.timers.tick(500);
    await settle();
    value.updateSettings(previous => ({ ...previous, game: { ...previous.game, memoryMb: 6144 } }));
    context.mock.timers.tick(500);
    await settle();
    assert.deepEqual(calls, [4096]);
    first.resolve();
    await settle();
    assert.deepEqual(calls, [4096, 6144]);
    assert.match(value.error, /disk full/);
    await value.retrySave();
    assert.deepEqual(calls, [4096, 6144, 6144]);
    assert.equal(value.error, null);
    value.markSettingsLoaded('corrupt settings');
    await value.retrySave();
    assert.equal(calls.length, 3);
    assert.equal(value.error, 'corrupt settings');
    destroy();
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
    const decisionButton = new HTMLElement();
    decisionButton.closest = selector => selector === '[data-mod-decisions]' ? decisionButton : null;
    const decisionEnter = new Event('keydown', { cancelable: true });
    Object.defineProperties(decisionEnter, { key: { value: 'Enter' }, target: { value: decisionButton } });
    window.dispatchEvent(decisionEnter);
    assert.equal(decisionEnter.defaultPrevented, false, 'decision button keeps its native Enter action');
    assert.deepEqual(launches, [], 'focused decision must not launch a profile');
    key('Enter');
    assert.deepEqual(launches, ['fresh']);
    args.showAccounts = true;
    key('Enter');
    assert.equal(launches.length, 1);
    destroy();
    key(',', { ctrlKey: true });
    assert.equal(args.activeView, 'main');
});

test('main Enter leaves native controls outside the Mod decision scope to their own actions', async context => {
    keyboard(context);
    const { useMainCommandKeydown } = await load('navigation/useMainCommandKeydown');
    const actions = [];
    const search = new HTMLInputElement();
    search.closest = selector => selector.split(',').map(value => value.trim()).includes('input') ? search : null;
    const args = {
        activeView: 'main', showAccounts: false, deleteConfirmId: null, logoutConfirm: false, showAdvConfirm: false,
        configProfileId: null, loginModalOpenRef: { current: false }, inputRef: { current: search },
        searchMode: 'profile', creating: null, versionDialogModpack: null,
        navItemsRef: { current: ['p:first'] }, navIndexRef: { current: 0 },
        profiles: [{ id: 'first' }], hoverProfileId: null, hoverVersionKey: null, savedAuth: null,
        isProfileBusy: () => false, setActiveProfileId: () => {},
        handleLaunchGame: () => actions.push('launch'), handleWebviewLogin: () => actions.push('login'),
        modpackResults: [{ project_id: 'pack' }], openModpackVersionDialog: () => actions.push('modpack'),
    };
    const { destroy } = mount(() => useMainCommandKeydown(args));
    const enter = target => {
        const event = new Event('keydown', { cancelable: true });
        Object.defineProperties(event, { key: { value: 'Enter' }, target: { value: target } });
        window.dispatchEvent(event);
        return event;
    };
    for (const mode of ['profile', 'modpack']) {
        args.searchMode = mode;
        for (const control of ['button', 'a[href]', 'input', 'textarea', 'select', 'summary', "[contenteditable='true']", "[role='button']"]) {
            const target = new HTMLElement();
            target.closest = selector => selector.split(',').map(value => value.trim()).includes(control) ? target : null;
            assert.equal(enter(target).defaultPrevented, false, `${control} keeps native Enter in ${mode} mode`);
            assert.deepEqual(actions, [], 'generic control Enter cannot start login, launch or open Modpack selection');
        }
    }
    args.searchMode = 'profile';
    assert.equal(enter(search).defaultPrevented, true, 'main search retains its command Enter');
    assert.deepEqual(actions, ['login']);
    args.searchMode = 'modpack';
    assert.equal(enter(search).defaultPrevented, true);
    assert.deepEqual(actions, ['login', 'modpack']);
    destroy();
});

test('main focus redirect preserves legitimate controls and only redirects noninteractive surfaces', async context => {
    keyboard(context);
    const oldDocument = Object.getOwnPropertyDescriptor(globalThis, 'document');
    const document = new EventTarget();
    Object.defineProperty(globalThis, 'document', { configurable: true, value: document });
    context.after(() => {
        if (oldDocument) Object.defineProperty(globalThis, 'document', oldDocument);
        else delete globalThis.document;
    });
    const { useMainInputFocusRedirect } = await load('navigation/useMainInputFocusRedirect');
    const active = mutable_source(true);
    let activeElement = null;
    const redirects = [];
    const focus = target => {
        activeElement = target;
        const event = new Event('focusin');
        Object.defineProperty(event, 'target', { value: target });
        document.dispatchEvent(event);
    };
    const search = new HTMLInputElement();
    search.focus = options => { redirects.push(options); focus(search); };
    const inputRef = { current: search };
    const { destroy } = mount(() => useMainInputFocusRedirect({ get active() { return get(active); }, inputRef }));
    for (const control of [
        'button', 'a[href]', 'input', 'textarea', 'select', 'summary', 'dialog',
        "[contenteditable='true']", "[contenteditable='']", "[contenteditable='plaintext-only']",
        "[role='button']", "[role='checkbox']", "[role='radio']", "[role='switch']", "[role='tab']",
        "[role='textbox']", "[role='combobox']", "[role='listbox']", "[role='option']", "[role='slider']",
        "[role='spinbutton']", "[role='menu']", "[role='menuitem']", "[role='dialog']", '[data-command-action]',
    ]) {
        const target = new HTMLElement();
        target.closest = selector => selector.split(',').map(value => value.trim()).includes(control) ? target : null;
        focus(target);
        assert.equal(activeElement, target, `${control} retains focus without a Mod-specific marker`);
    }
    const child = new HTMLElement();
    const button = new HTMLElement();
    child.closest = selector => selector.split(',').map(value => value.trim()).includes('button') ? button : null;
    focus(child);
    assert.equal(activeElement, child, 'interactive descendants retain focus too');
    focus(search);
    assert.deepEqual(redirects, [], 'focusing controls or the search itself never forces another focus');
    const background = new HTMLElement();
    focus(background);
    assert.equal(activeElement, search, 'ordinary main surfaces still redirect to search');
    assert.deepEqual(redirects, [{ preventScroll: true }]);
    const replacement = new HTMLInputElement();
    replacement.focus = options => { redirects.push(options); focus(replacement); };
    inputRef.current = replacement;
    focus(background);
    assert.equal(activeElement, replacement, 'redirect reads the current search ref');
    assert.equal(redirects.length, 2, 'redirecting focus does not recurse');
    set(active, false);
    flushSync();
    focus(background);
    assert.equal(activeElement, background, 'inactive views never redirect focus');
    set(active, true);
    flushSync();
    focus(background);
    assert.equal(activeElement, replacement, 'returning to main restores ordinary redirect behavior');
    destroy();
    focus(background);
    assert.equal(activeElement, background, 'disposed focus owner removes its listener');
});

test('profile row activation leaves nested native button Enter to that button', () => {
    const source = readFileSync('src/components/ProfileRow.svelte', 'utf8');
    assert.match(source, /onkeydown=\{\(event\) => \{ if \(event\.key === "Enter" && event\.target === event\.currentTarget\)/);
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
    assert.equal(state.loginState, 'waiting');
    assert.equal(state.loginModalOpen, false);
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

test('sign-in waiting state resets after failure or user cancellation and permits retry', async () => {
    const { useAccountActions } = await load('useAccountActions');
    for (const failure of ['__user_cancelled__', 'provider unavailable']) {
        const login = deferred();
        let calls = 0;
        environment.invoke = async command => {
            if (command === 'start_webview_login') { calls++; return login.promise; }
        };
        const { state, params } = accountFixture();
        const visibility = [];
        environment.currentWindow.show = async () => visibility.push('show');
        environment.currentWindow.setFocus = async () => visibility.push('focus');
        const { value, destroy } = mount(() => useAccountActions(params));
        const pending = value.handleWebviewLogin();
        assert.equal(state.loginState, 'waiting');
        await value.handleWebviewLogin();
        assert.equal(calls, 1);
        login.reject(new Error(failure));
        await pending;
        assert.equal(state.loginState, failure === '__user_cancelled__' ? 'idle' : 'error');
        assert.deepEqual(visibility, ['show', 'focus']);
        const retry = value.handleWebviewLogin();
        assert.equal(calls, 2);
        await retry;
        destroy();
    }
});
