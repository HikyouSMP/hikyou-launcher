import { useEffect } from "react";

type Args = {
  active: boolean;
  inputRef: React.RefObject<HTMLInputElement | null>;
};

export function useMainInputFocusRedirect({ active, inputRef }: Args) {
  useEffect(() => {
    if (!active) return;
    let redirecting = false;
    const handleFocus = (event: FocusEvent) => {
      if (redirecting) return;
      const target = event.target as HTMLElement | null;
      if (!target) return;
      if (
        target === inputRef.current ||
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement
      ) {
        return;
      }
      redirecting = true;
      inputRef.current?.focus({ preventScroll: true });
      redirecting = false;
    };
    document.addEventListener("focusin", handleFocus);
    return () => document.removeEventListener("focusin", handleFocus);
  }, [active, inputRef]);
}
