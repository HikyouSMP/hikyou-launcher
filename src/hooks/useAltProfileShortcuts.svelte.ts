import { untrack } from 'svelte';
import { type Setter, type Ref } from '../lib/state';
import type { ActiveView, Profile } from "../types";
type Args = {
    navIndex: number;
    navItemsRef: Ref<string[]>;
    profiles: Profile[];
    setModsProfileId: Setter<string | null>;
    setActiveView: Setter<ActiveView>;
    navDirRef: Ref<"forward" | "back" | "none">;
};
export function useAltProfileShortcuts(params: Args) {
    $effect(() => {
        void params.navDirRef;
        void params.navIndex;
        void params.navItemsRef;
        void params.profiles;
        void params.setActiveView;
        void params.setModsProfileId;
        return untrack(() => {
            const handler = (event: KeyboardEvent) => {
                if (!event.altKey || event.ctrlKey || event.metaKey)
                    return;
                if (event.code === "Digit1") {
                    event.preventDefault();
                    params.navDirRef.current = "forward";
                    params.setModsProfileId(null);
                    params.setActiveView("main");
                    return;
                }
                if (event.code === "Digit2") {
                    event.preventDefault();
                    const navItem = params.navItemsRef.current[params.navIndex];
                    if (!navItem?.startsWith("p:"))
                        return;
                    const profileId = navItem.slice(2);
                    const target = params.profiles.find((profile) => profile.id === profileId &&
                        profile.kind !== "smart" &&
                        profile.loader !== "vanilla");
                    if (target) {
                        params.navDirRef.current = "forward";
                        params.setModsProfileId(target.id);
                        params.setActiveView("mods");
                    }
                    return;
                }
                event.preventDefault();
            };
            window.addEventListener("keydown", handler);
            return () => window.removeEventListener("keydown", handler);
        });
    });
}
