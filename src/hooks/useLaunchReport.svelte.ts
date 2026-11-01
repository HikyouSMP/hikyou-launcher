import { invoke } from "@tauri-apps/api/core";

type ReportContext = {
    code: "launch_failed" | "mod_resolution_limit" | "mod_resolution_unsatisfiable" | "mod_resolution_unsupported";
    limit: "search_states" | "search_states_projects" | "projects" | "wall_time" | "other" | null;
    counters?: { states: number | null; projects: number | null; roots: number | null } | null;
};
type ReportPreview = { context: ReportContext; title: string; body: string };
type Dependencies = {
    invoke: typeof invoke;
    copy: (text: string) => Promise<void>;
};

export function useLaunchReport(dependencies: Dependencies = {
    invoke,
    copy: text => navigator.clipboard.writeText(text),
}) {
    let preview = $state<ReportPreview | null>(null);
    let busy = $state(false);
    let reviewed = $state(false);
    let failure = $state<"prepare_failed" | "open_failed" | "copy_failed" | null>(null);
    let result = $state<"opened" | "copied" | null>(null);
    let generation = 0;

    async function prepare(error: string) {
        const current = ++generation;
        preview = null;
        reviewed = false;
        failure = null;
        result = null;
        busy = true;
        try {
            const report = await dependencies.invoke<ReportPreview>("prepare_launch_error_report", { error });
            if (current === generation) preview = report;
        } catch {
            if (current === generation) failure = "prepare_failed";
        } finally {
            if (current === generation) busy = false;
        }
    }

    async function act(kind: "open" | "copy") {
        if (!preview || !reviewed || busy) return;
        const current = generation;
        busy = true;
        failure = null;
        result = null;
        try {
            if (kind === "open") {
                await dependencies.invoke("open_launch_error_report", { context: preview.context, reviewed: true });
            } else {
                await dependencies.copy(`${preview.title}\n\n${preview.body}`);
            }
            if (current === generation) result = kind === "open" ? "opened" : "copied";
        } catch {
            if (current === generation) failure = kind === "open" ? "open_failed" : "copy_failed";
        } finally {
            if (current === generation) busy = false;
        }
    }

    return {
        get preview() { return preview; },
        get busy() { return busy; },
        get reviewed() { return reviewed; },
        set reviewed(value: boolean) { reviewed = value; },
        get failure() { return failure; },
        get result() { return result; },
        prepare,
        open: () => act("open"),
        copy: () => act("copy"),
        dispose() { generation++; },
    };
}
