type Key = Pick<KeyboardEvent, "key" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey" | "repeat" | "isComposing">;
export type RemovalScope = {
  blocked: boolean;
  query: string;
  queryInput: boolean;
  otherControl: boolean;
  selectionStart: number | null;
  selectionEnd: number | null;
};

export function isModRemovalKey(event: Key, scope: RemovalScope): boolean {
  if (scope.blocked || scope.otherControl || event.isComposing || event.repeat || event.ctrlKey || event.metaKey || event.shiftKey) return false;
  if (event.key === "ArrowLeft") return event.altKey;
  if (event.altKey || !["Delete", "Backspace"].includes(event.key) || scope.query !== "") return false;
  return !scope.queryInput || (scope.selectionStart === 0 && scope.selectionEnd === 0);
}

export function removalStep(pending: string | null, id: string): { pending: string | null; remove: boolean } {
  return pending === id ? { pending: null, remove: true } : { pending: id, remove: false };
}
