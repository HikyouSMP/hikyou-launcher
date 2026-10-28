import { untrack } from 'svelte';
import { type Ref } from '../../lib/state';
type Args = {
    active: boolean;
    inputRef: Ref<HTMLInputElement | null>;
};
export function useMainInputFocusRedirect(params: Args) {
    $effect(() => {
        void params.active;
        void params.inputRef;
        return untrack(() => {
            if (!params.active)
                return;
            let redirecting = false;
            const handleFocus = (event: FocusEvent) => {
                if (redirecting)
                    return;
                const target = event.target as HTMLElement | null;
                if (!target)
                    return;
                if (target === params.inputRef.current ||
                    target.closest("[data-command-action]") ||
                    target instanceof HTMLInputElement ||
                    target instanceof HTMLTextAreaElement) {
                    return;
                }
                redirecting = true;
                params.inputRef.current?.focus({ preventScroll: true });
                redirecting = false;
            };
            document.addEventListener("focusin", handleFocus);
            return () => document.removeEventListener("focusin", handleFocus);
        });
    });
}
