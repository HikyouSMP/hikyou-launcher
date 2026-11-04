import { untrack } from 'svelte';
import { type Ref } from '../../lib/state';
import { isInteractiveTarget } from '../../utils/dom';
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
                const target = event.target;
                if (!(target instanceof HTMLElement))
                    return;
                if (target === params.inputRef.current ||
                    isInteractiveTarget(target)) {
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
