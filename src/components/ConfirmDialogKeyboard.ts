export function moveDialogFocus(event: KeyboardEvent & { currentTarget: EventTarget & HTMLElement }) {
  if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
  const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("button"));
  const current = buttons.indexOf(document.activeElement as HTMLElement);
  if (event.key === "ArrowLeft") buttons[current <= 0 ? buttons.length - 1 : current - 1]?.focus();
  else buttons[current >= buttons.length - 1 ? 0 : current + 1]?.focus();
}

export function focusDialogButton(node: HTMLButtonElement) { node.focus(); }
