import { useEffect } from "react";
import type { ActiveView, Profile } from "../types";

type Args = {
  navIndex: number;
  navItemsRef: React.MutableRefObject<string[]>;
  profiles: Profile[];
  setModsProfileId: React.Dispatch<React.SetStateAction<string | null>>;
  setActiveView: React.Dispatch<React.SetStateAction<ActiveView>>;
  navDirRef: React.MutableRefObject<"forward" | "back" | "none">;
};

export function useAltProfileShortcuts({
  navIndex,
  navItemsRef,
  profiles,
  setModsProfileId,
  setActiveView,
  navDirRef,
}: Args) {
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (!event.altKey || event.ctrlKey || event.metaKey) return;
      if (event.code === "Digit1") {
        event.preventDefault();
        navDirRef.current = "forward";
        setModsProfileId(null);
        setActiveView("main");
        return;
      }
      if (event.code === "Digit2") {
        event.preventDefault();
        const navItem = navItemsRef.current[navIndex];
        if (!navItem?.startsWith("p:")) return;
        const profileId = navItem.slice(2);
        const target = profiles.find(
          (profile) =>
            profile.id === profileId &&
            profile.kind !== "smart" &&
            profile.loader !== "vanilla",
        );
        if (target) {
          navDirRef.current = "forward";
          setModsProfileId(target.id);
          setActiveView("mods");
        }
        return;
      }
      event.preventDefault();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [navDirRef, navIndex, navItemsRef, profiles, setActiveView, setModsProfileId]);
}
