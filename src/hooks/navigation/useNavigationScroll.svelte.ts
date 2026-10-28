import { untrack } from 'svelte';
import { type Ref } from '../../lib/state';
type Args = {
    navIndex: number;
    navItemsRef: Ref<string[]>;
    navElemsRef: Ref<Map<string, HTMLElement>>;
    versionDialogOpen: boolean;
    modpackVersionIdx: number;
};
export function useNavigationScroll(params: Args) {
    $effect(() => {
        void params.modpackVersionIdx;
        void params.versionDialogOpen;
        return untrack(() => {
            if (!params.versionDialogOpen)
                return;
            document
                .querySelector<HTMLElement>(`[data-modpack-version-idx="${params.modpackVersionIdx}"]`)
                ?.scrollIntoView({ block: "nearest", behavior: "instant" });
        });
    });
    $effect(() => {
        void params.navElemsRef;
        void params.navIndex;
        const items = params.navItemsRef.current;
        return untrack(() => {
            if (params.navIndex < 0 || params.navIndex >= items.length)
                return;
            params.navElemsRef.current
                .get(items[params.navIndex])
                ?.scrollIntoView({ block: "nearest", behavior: "instant" });
        });
    });
}
