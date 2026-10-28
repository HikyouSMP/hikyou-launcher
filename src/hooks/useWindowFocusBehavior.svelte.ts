import { untrack } from 'svelte';
import { type Ref } from '../lib/state';
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
type Args = {
    keepLauncherVisible?: boolean;
    loginModalOpenRef: Ref<boolean>;
    crashToastOpenRef: Ref<boolean>;
    isDraggingRef: Ref<boolean>;
    inputRef: Ref<HTMLInputElement | null>;
};
export function useWindowFocusBehavior(params: Args) {
    $effect(() => {
        void params.keepLauncherVisible;
        void params.loginModalOpenRef;
        void params.crashToastOpenRef;
        void params.isDraggingRef;
        void params.inputRef;
        return untrack(() => {
            const timers = new Set<ReturnType<typeof setTimeout>>();
            const schedule = (callback: () => void, delay: number) => {
                const timer = setTimeout(() => {
                    timers.delete(timer);
                    callback();
                }, delay);
                timers.add(timer);
                return timer;
            };
            const onMouseUp = () => {
                schedule(() => {
                    params.isDraggingRef.current = false;
                }, 300);
            };
            document.addEventListener("mouseup", onMouseUp);
            const win = getCurrentWindow();
            let focusLostTimer: ReturnType<typeof setTimeout> | null = null;
            let dragGraceTimer: ReturnType<typeof setTimeout> | null = null;
            const shouldStayVisible = () => params.loginModalOpenRef.current ||
                params.crashToastOpenRef.current ||
                Boolean(document.querySelector("[data-native-dialog-open]")) ||
                Boolean(params.keepLauncherVisible);
            const focusMainInput = () => {
                if (document.querySelector('[data-focus-scope="modal"]'))
                    return;
                params.inputRef.current?.focus();
            };
            const hideAfterFocusLoss = () => {
                if (focusLostTimer)
                    clearTimeout(focusLostTimer);
                focusLostTimer = schedule(() => {
                    if (shouldStayVisible())
                        return;
                    if (params.isDraggingRef.current) {
                        if (dragGraceTimer)
                            clearTimeout(dragGraceTimer);
                        dragGraceTimer = schedule(() => {
                            params.isDraggingRef.current = false;
                            if (!shouldStayVisible()) {
                                invoke("hide_main_window", {
                                    reason: "focus_lost_after_drag_grace",
                                }).catch(console.error);
                            }
                        }, 420);
                        return;
                    }
                    invoke("hide_main_window", { reason: "focus_lost" }).catch(console.error);
                }, 90);
            };
            const unlisten = win.onFocusChanged(({ payload: focused }) => {
                if (focused) {
                    if (focusLostTimer)
                        clearTimeout(focusLostTimer);
                    if (dragGraceTimer)
                        clearTimeout(dragGraceTimer);
                    params.isDraggingRef.current = false;
                    schedule(focusMainInput, 10);
                }
                else if (!shouldStayVisible()) {
                    hideAfterFocusLoss();
                }
            });
            schedule(focusMainInput, 10);
            return () => {
                document.removeEventListener("mouseup", onMouseUp);
                for (const timer of timers) clearTimeout(timer);
                if (focusLostTimer)
                    clearTimeout(focusLostTimer);
                if (dragGraceTimer)
                    clearTimeout(dragGraceTimer);
                unlisten.then((dispose) => dispose()).catch(console.error);
            };
        });
    });
}
