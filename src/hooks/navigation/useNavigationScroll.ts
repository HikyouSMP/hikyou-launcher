import { useEffect } from "react";

type Args = {
  navIndex: number;
  navItemsRef: React.MutableRefObject<string[]>;
  navElemsRef: React.MutableRefObject<Map<string, HTMLElement>>;
  versionDialogOpen: boolean;
  modpackVersionIdx: number;
};

export function useNavigationScroll({
  navIndex,
  navItemsRef,
  navElemsRef,
  versionDialogOpen,
  modpackVersionIdx,
}: Args) {
  useEffect(() => {
    if (!versionDialogOpen) return;
    document
      .querySelector<HTMLElement>(
        `[data-modpack-version-idx="${modpackVersionIdx}"]`,
      )
      ?.scrollIntoView({ block: "nearest", behavior: "instant" });
  }, [modpackVersionIdx, versionDialogOpen]);

  useEffect(() => {
    const items = navItemsRef.current;
    if (navIndex < 0 || navIndex >= items.length) return;
    navElemsRef.current
      .get(items[navIndex])
      ?.scrollIntoView({ block: "nearest", behavior: "instant" });
  }, [navElemsRef, navIndex, navItemsRef]);
}
