import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

type Args = {
  keepLauncherVisible?: boolean;
  loginModalOpenRef: React.MutableRefObject<boolean>;
  crashToastOpenRef: React.MutableRefObject<boolean>;
  isDraggingRef: React.MutableRefObject<boolean>;
  inputRef: React.RefObject<HTMLInputElement | null>;
};

export function useWindowFocusBehavior({
  keepLauncherVisible,
  loginModalOpenRef,
  crashToastOpenRef,
  isDraggingRef,
  inputRef,
}: Args) {
  useEffect(() => {
    const onMouseUp = () => {
      setTimeout(() => {
        isDraggingRef.current = false;
      }, 300);
    };
    document.addEventListener("mouseup", onMouseUp);

    const win = getCurrentWindow();
    let focusLostTimer: ReturnType<typeof setTimeout> | null = null;
    let dragGraceTimer: ReturnType<typeof setTimeout> | null = null;

    const shouldStayVisible = () =>
      loginModalOpenRef.current ||
      crashToastOpenRef.current ||
      Boolean(keepLauncherVisible);

    const focusMainInput = () => {
      if (document.querySelector('[data-focus-scope="modal"]')) return;
      inputRef.current?.focus();
    };

    const hideAfterFocusLoss = () => {
      if (focusLostTimer) clearTimeout(focusLostTimer);
      focusLostTimer = setTimeout(() => {
        if (shouldStayVisible()) return;
        if (isDraggingRef.current) {
          if (dragGraceTimer) clearTimeout(dragGraceTimer);
          dragGraceTimer = setTimeout(() => {
            isDraggingRef.current = false;
            if (!shouldStayVisible()) {
              invoke("hide_main_window", {
                reason: "focus_lost_after_drag_grace",
              }).catch(console.error);
            }
          }, 420);
          return;
        }
        invoke("hide_main_window", { reason: "focus_lost" }).catch(
          console.error,
        );
      }, 90);
    };

    const unlisten = win.onFocusChanged(({ payload: focused }) => {
      if (focused) {
        if (focusLostTimer) clearTimeout(focusLostTimer);
        if (dragGraceTimer) clearTimeout(dragGraceTimer);
        isDraggingRef.current = false;
        setTimeout(focusMainInput, 10);
      } else if (!shouldStayVisible()) {
        hideAfterFocusLoss();
      }
    });

    setTimeout(focusMainInput, 10);
    return () => {
      document.removeEventListener("mouseup", onMouseUp);
      if (focusLostTimer) clearTimeout(focusLostTimer);
      if (dragGraceTimer) clearTimeout(dragGraceTimer);
      unlisten.then((dispose) => dispose());
    };
  }, [
    keepLauncherVisible,
    loginModalOpenRef,
    crashToastOpenRef,
    isDraggingRef,
    inputRef,
  ]);
}
