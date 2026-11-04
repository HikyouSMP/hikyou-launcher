export function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false;
  return Boolean(
    target.closest(
      "input, textarea, select, [contenteditable='true'], [contenteditable='']",
    ),
  );
}

export function isInteractiveTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false;
  return Boolean(target.closest(
    "button, a[href], input, textarea, select, summary, dialog, " +
    "[contenteditable='true'], [contenteditable=''], [contenteditable='plaintext-only'], " +
    "[role='button'], [role='checkbox'], [role='radio'], [role='switch'], [role='tab'], " +
    "[role='textbox'], [role='combobox'], [role='listbox'], [role='option'], " +
    "[role='slider'], [role='spinbutton'], [role='menu'], [role='menuitem'], [role='dialog'], " +
    "[data-command-action]",
  ));
}

export function hasDocumentSelection() {
  const selection = window.getSelection();
  return Boolean(selection && !selection.isCollapsed && selection.toString());
}
