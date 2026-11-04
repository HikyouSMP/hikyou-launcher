import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { choiceAllowed, modLifecycleEvents, type ModChoice, type ModSnapshot } from "../modLifecycle";
import { modLifecycleFailure, type ModLifecycleFailure } from "../modLifecycleFailure";

export function useModSnapshot(profile: () => string) {
  let snapshot = $state<ModSnapshot | null>(null);
  let loading = $state(true);
  let busy = $state(false);
  let loadError = $state<ModLifecycleFailure | null>(null);
  let changeError = $state<ModLifecycleFailure | null>(null);
  let eventError = $state<ModLifecycleFailure | null>(null);
  let retry = $state(0);
  let scope = 0;
  let request = 0;

  function valid(value: ModSnapshot, id: string) {
    return value.profileId === id && value.requested.intent.profileId === id;
  }
  async function refresh(id = profile(), epoch = scope) {
    const current = ++request;
    loading = true;
    try {
      const value = await invoke<ModSnapshot>("get_profile_mod_snapshot", { profileId: id });
      if (epoch !== scope || current !== request) return;
      if (!valid(value, id)) throw new Error("profile_mismatch");
      snapshot = value;
      loadError = null;
    } catch (reason) {
      if (epoch === scope && current === request) loadError = modLifecycleFailure(reason, "get_profile_mod_snapshot");
    } finally {
      if (epoch === scope && current === request) loading = false;
    }
  }
  $effect(() => {
    const id = profile();
    void retry;
    const epoch = ++scope;
    snapshot = null; loading = true; busy = false;
    loadError = null; changeError = null; eventError = null;
    let disposed = false;
    const stops: UnlistenFn[] = [];
    void Promise.all(modLifecycleEvents.map(async (name) => {
      try {
        const stop = await listen<{ profileId?: string }>(name, ({ payload }) => {
          if (!disposed && (!payload?.profileId || payload.profileId === id)) void refresh(id, epoch);
        });
        if (disposed) stop(); else stops.push(stop);
      } catch (reason) { if (!disposed) eventError = modLifecycleFailure(reason, name); }
    })).then(() => { if (!disposed) void refresh(id, epoch); });
    return () => { disposed = true; scope++; request++; stops.forEach((stop) => stop()); };
  });

  async function mutate(command: "set_profile_mod_choice" | "install_modrinth_mod", args: Record<string, unknown>, returnsSnapshot: boolean) {
    const id = profile();
    if (busy || loading || loadError || !snapshot || snapshot.profileId !== id) return;
    const epoch = scope;
    const revision = snapshot.revision;
    const startedAt = request;
    busy = true; changeError = null;
    try {
      const value = await invoke<ModSnapshot>(command, { ...args, profileId: id, ...(returnsSnapshot ? { expectedRevision: revision } : {}) });
      if (epoch !== scope) return;
      if (returnsSnapshot) {
        if (!valid(value, id)) throw new Error("profile_mismatch");
        if (request !== startedAt) await refresh(id, epoch);
        else {
          request++;
          snapshot = value; loadError = null; loading = false;
        }
      } else await refresh(id, epoch);
    } catch (reason) {
      if (epoch === scope) {
        changeError = modLifecycleFailure(reason, command);
        // A stale revision or a saved-but-unprepared request must be re-read, never replayed.
        await refresh(id, epoch);
      }
    } finally {
      if (epoch === scope) busy = false;
    }
  }
  return {
    get snapshot() { return snapshot; }, get loading() { return loading; }, get busy() { return busy; },
    get loadFailed() { return loadError !== null; }, get changeFailed() { return changeError !== null; }, get eventsFailed() { return eventError !== null; },
    get failures() { return [loadError, changeError, eventError].filter((failure): failure is ModLifecycleFailure => failure !== null); },
    refresh, retry: () => { if (!busy) retry++; },
    choose(key: string, choice: ModChoice) {
      const row = snapshot?.rows.find((item) => item.key === key);
      if (!row || !choiceAllowed(row, choice)) return Promise.resolve();
      return mutate("set_profile_mod_choice", { key, choice }, true);
    },
    install(args: Record<string, unknown>) { return mutate("install_modrinth_mod", args, false); },
  };
}
