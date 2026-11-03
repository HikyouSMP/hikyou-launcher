import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { decisionActions, modLifecycleEvents, type DecisionView, type ModDecisionAction } from "../modLifecycle";
import { modLifecycleFailure, type ModLifecycleFailure } from "../modLifecycleFailure";

export function useModDecisions(profile: () => string | null = () => null) {
  let decisions = $state<DecisionView[]>([]);
  let loading = $state(true);
  let busy = $state(false);
  let loadError = $state<ModLifecycleFailure | null>(null);
  let answerError = $state<ModLifecycleFailure | null>(null);
  let eventError = $state<ModLifecycleFailure | null>(null);
  let retry = $state(0);
  let scope = 0;
  let request = 0;
  async function refresh(id = profile(), epoch = scope) {
    const current = ++request;
    loading = true;
    try {
      const value = await invoke<DecisionView[]>("get_mod_decisions", { profileId: id });
      if (epoch !== scope || current !== request) return;
      if (id && value.some((item) => item.profileId !== id)) throw new Error("profile_mismatch");
      decisions = value; loadError = null;
    } catch (reason) {
      if (epoch === scope && current === request) loadError = modLifecycleFailure(reason, "get_mod_decisions");
    } finally {
      if (epoch === scope && current === request) loading = false;
    }
  }
  $effect(() => {
    const id = profile();
    void retry;
    const epoch = ++scope;
    decisions = []; loading = true; busy = false;
    loadError = null; answerError = null; eventError = null;
    let disposed = false;
    const stops: UnlistenFn[] = [];
    void Promise.all(modLifecycleEvents.map(async (name) => {
      try {
        const stop = await listen<{ profileId?: string }>(name, ({ payload }) => {
          if (!disposed && (!id || !payload?.profileId || payload.profileId === id)) void refresh(id, epoch);
        });
        if (disposed) stop(); else stops.push(stop);
      } catch (reason) { if (!disposed) eventError = modLifecycleFailure(reason, name); }
    })).then(() => { if (!disposed) void refresh(id, epoch); });
    return () => { disposed = true; scope++; request++; stops.forEach((stop) => stop()); };
  });
  async function answer(decision: DecisionView, action: ModDecisionAction) {
    const id = profile();
    if (id && decision.profileId !== id) return;
    if (busy || loadError || !decisionActions(decision.phase).includes(action)) return;
    // Only the currently fetched proposal can supply a revision token.
    if (!decisions.some((item) => item.id === decision.id && item.profileId === decision.profileId && item.expectedRevision === decision.expectedRevision && item.phase === decision.phase)) return;
    const epoch = scope;
    busy = true; answerError = null;
    try {
      await invoke("answer_mod_decision", { profileId: decision.profileId, proposalId: decision.id, expectedRevision: decision.expectedRevision, action });
    } catch (reason) {
      if (epoch === scope) answerError = modLifecycleFailure(reason, "answer_mod_decision");
    } finally {
      if (epoch === scope) await refresh(id, epoch);
      if (epoch === scope) busy = false;
    }
  }
  return {
    get decisions() { return decisions; }, get loading() { return loading; }, get busy() { return busy; },
    get loadFailed() { return loadError !== null; }, get answerFailed() { return answerError !== null; }, get eventsFailed() { return eventError !== null; },
    get failures() { return [loadError, answerError, eventError].filter((failure): failure is ModLifecycleFailure => failure !== null); },
    answer, refresh, retry: () => { if (!busy) retry++; },
  };
}
