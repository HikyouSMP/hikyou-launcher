<script lang="ts">
  import { getContext } from "svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { C } from "../theme";
  import { css } from "../lib/dom";
  let { on, onToggle, "aria-label": ariaLabel }: { on: boolean; onToggle: () => void; "aria-label"?: string } = $props();
  const settingLabel = getContext<(() => string) | undefined>("hikyou-settings-label");
  const { t } = createTranslation();
</script>

<button onclick={onToggle} role="switch" aria-checked={on} aria-label={ariaLabel ?? settingLabel?.() ?? t("common.enable")}
  class="toggle-root flex items-center leading-none shrink-0 cursor-pointer p-0 bg-transparent border-0">
  <span class={on ? "toggle-track on inline-flex w-9 h-5 rounded-[10px] relative shrink-0 transition-[background,box-shadow] duration-200" : "toggle-track inline-flex w-9 h-5 rounded-[10px] relative shrink-0 transition-[background,box-shadow] duration-200"}
    style={css({ background: on ? C.green : "rgba(255,255,255,.10)", boxShadow: "inset 0 0 0 1px rgba(255,255,255,.035)", transitionTimingFunction: "cubic-bezier(.2,0,0,1)" })}>
    <span class="absolute top-[3px] w-[14px] h-[14px] rounded-[7px] bg-white transition-[left,box-shadow] duration-200"
      style={css({ left: on ? 19 : 3, boxShadow: on ? "0 2px 6px rgba(0,0,0,.32), 0 0 10px rgba(255,255,255,.18)" : "0 1px 3px rgba(0,0,0,.35)", transitionTimingFunction: "cubic-bezier(.2,0,0,1)" })}></span>
  </span>
</button>
