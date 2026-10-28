import { untrack } from 'svelte';
export function useSettingsInputBlur(enabled: () => boolean) {
    $effect(() => {
        const active = enabled();
        return untrack(() => {
            if (!active)
                return;
            const blurSettingsInput = (event: MouseEvent) => {
                const active = document.activeElement;
                if (!(active instanceof HTMLInputElement ||
                    active instanceof HTMLTextAreaElement)) {
                    return;
                }
                const target = event.target;
                if (!(target instanceof HTMLElement))
                    return;
                if (target.closest("input, textarea, select, button, a[href], [tabindex]"))
                    return;
                active.blur();
            };
            document.addEventListener("mousedown", blurSettingsInput, true);
            return () => document.removeEventListener("mousedown", blurSettingsInput, true);
        });
    });
}
