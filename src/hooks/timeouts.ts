import { onDestroy } from 'svelte';

export function createTimeouts() {
    const pending = new Set<ReturnType<typeof setTimeout>>();
    const clear = (timer: ReturnType<typeof setTimeout>) => {
        clearTimeout(timer);
        pending.delete(timer);
    };
    onDestroy(() => {
        for (const timer of pending) clearTimeout(timer);
        pending.clear();
    });
    return {
        set(callback: () => void, delay: number) {
            const timer = setTimeout(() => {
                pending.delete(timer);
                callback();
            }, delay);
            pending.add(timer);
            return timer;
        },
        clear,
    };
}
