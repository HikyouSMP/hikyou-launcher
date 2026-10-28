import { onDestroy } from "svelte";
import type { TOptions } from "i18next";
import i18n from "../i18n";

export function createTranslation() {
  let language = $state(i18n.language);
  const changed = (next: string) => { language = next; };
  i18n.on("languageChanged", changed);
  onDestroy(() => { i18n.off("languageChanged", changed); });
  return {
    get language() { return language; },
    t(key: string, options?: TOptions): string {
      return i18n.t(key, { ...options, lng: language });
    },
    i18n,
  };
}
