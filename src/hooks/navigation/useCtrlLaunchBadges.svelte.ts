import { untrack } from 'svelte';
import { type Setter, type Ref } from '../../lib/state';
type Args = {
    isMacOS: boolean;
    ctrlHeld: boolean;
    setCtrlHeld: Setter<boolean>;
    navItemsRef: Ref<string[]>;
};
export function useCtrlLaunchBadges(params: Args) {
    $effect(() => {
        void params.ctrlHeld;
        void params.isMacOS;
        void params.navItemsRef;
        void params.setCtrlHeld;
        return untrack(() => {
            const triggerKey = params.isMacOS ? "Meta" : "Control";
            let timer: ReturnType<typeof setTimeout> | null = null;
            const clearTimer = () => {
                if (timer) {
                    clearTimeout(timer);
                    timer = null;
                }
            };
            const reset = () => {
                clearTimer();
                params.setCtrlHeld(false);
            };
            const isPureTrigger = (event: KeyboardEvent) => {
                if (event.key !== triggerKey)
                    return false;
                if (params.isMacOS) {
                    return event.metaKey && !event.ctrlKey && !event.shiftKey && !event.altKey;
                }
                return event.ctrlKey && !event.metaKey && !event.shiftKey && !event.altKey;
            };
            const hasLaunchable = () => params.navItemsRef.current.some((item, index) => item.startsWith("p:") && index < 10);
            const onKeyDown = (event: KeyboardEvent) => {
                if (event.key !== triggerKey && (timer || params.ctrlHeld)) {
                    reset();
                    return;
                }
                if (isPureTrigger(event) && !params.ctrlHeld && hasLaunchable()) {
                    clearTimer();
                    timer = setTimeout(() => params.setCtrlHeld(true), 400);
                }
            };
            const onKeyUp = (event: KeyboardEvent) => {
                if (event.key !== triggerKey)
                    return;
                reset();
            };
            const onVisibilityChange = () => {
                if (document.hidden)
                    reset();
            };
            window.addEventListener("keydown", onKeyDown);
            window.addEventListener("keyup", onKeyUp);
            window.addEventListener("blur", reset);
            document.addEventListener("visibilitychange", onVisibilityChange);
            return () => {
                window.removeEventListener("keydown", onKeyDown);
                window.removeEventListener("keyup", onKeyUp);
                window.removeEventListener("blur", reset);
                document.removeEventListener("visibilitychange", onVisibilityChange);
                clearTimer();
            };
        });
    });
}
