import { useEffect } from "react";

type Args = {
  isMacOS: boolean;
  ctrlHeld: boolean;
  setCtrlHeld: React.Dispatch<React.SetStateAction<boolean>>;
  navItemsRef: React.MutableRefObject<string[]>;
};

export function useCtrlLaunchBadges({
  isMacOS,
  ctrlHeld,
  setCtrlHeld,
  navItemsRef,
}: Args) {
  useEffect(() => {
    const triggerKey = isMacOS ? "Meta" : "Control";
    let timer: ReturnType<typeof setTimeout> | null = null;
    const clearTimer = () => {
      if (timer) {
        clearTimeout(timer);
        timer = null;
      }
    };
    const reset = () => {
      clearTimer();
      setCtrlHeld(false);
    };
    const isPureTrigger = (event: KeyboardEvent) => {
      if (event.key !== triggerKey) return false;
      if (isMacOS) {
        return event.metaKey && !event.ctrlKey && !event.shiftKey && !event.altKey;
      }
      return event.ctrlKey && !event.metaKey && !event.shiftKey && !event.altKey;
    };
    const hasLaunchable = () =>
      navItemsRef.current.some(
        (item, index) => item.startsWith("p:") && index < 10,
      );
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== triggerKey && (timer || ctrlHeld)) {
        reset();
        return;
      }
      if (isPureTrigger(event) && !ctrlHeld && hasLaunchable()) {
        clearTimer();
        timer = setTimeout(() => setCtrlHeld(true), 400);
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key !== triggerKey) return;
      reset();
    };
    const onVisibilityChange = () => {
      if (document.hidden) reset();
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
  }, [ctrlHeld, isMacOS, navItemsRef, setCtrlHeld]);
}
