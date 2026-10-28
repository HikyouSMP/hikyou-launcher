<script lang="ts">
  import { CheckCircle2, XCircle } from "@lucide/svelte";
  import { scale } from "svelte/transition";
  import { createTranslation } from "../lib/i18n.svelte";
  import { C } from "../theme";
  import type { LoginState } from "../types";
  import ModalBackdrop from "./ModalBackdrop.svelte";

  let { isOpen, state, errorMessage, onRetry, onClose }: {
    isOpen: boolean;
    state: LoginState;
    errorMessage?: string;
    onRetry?: () => void;
    onClose?: () => void;
  } = $props();
  const { t } = createTranslation();
</script>

{#if isOpen}
  <div class="fixed inset-0 z-50" onclick={onClose} role="presentation"><ModalBackdrop fixed /></div>
  <div class="fixed inset-0 flex items-center justify-center z-51 pointer-events-none">
    <div class="w-100 p-8 pointer-events-auto rounded-md bg-surface border border-b1"
      style="box-shadow:0 24px 64px rgba(0,0,0,.6)" transition:scale={{ start: 0.9, duration: 180 }}>
      {#if state === "success"}
        <div class="flex flex-col items-center gap-4 text-center py-6">
          <CheckCircle2 size={64} color={C.green} />
          <div>
            <h2 class="text-t1 text-[22px] font-bold mb-2">{t("login_modal.success_title")}</h2>
            <p class="text-t2 text-[13px]">{t("login_modal.success_body")}</p>
          </div>
        </div>
      {/if}
      {#if state === "error"}
        <div class="flex flex-col items-center gap-4 text-center py-4">
          <XCircle size={64} color={C.danger} />
          <div>
            <h2 class="text-t1 text-xl font-bold mb-2">{t("login_modal.error_title")}</h2>
            <p class="text-xs mb-6 max-w-75 wrap-break-word leading-normal" style="color:rgba(160,40,65,.9)">
              {errorMessage || t("login_modal.error_unknown")}
            </p>
            <button onclick={onRetry} class="btn-phys rounded-md px-6 py-2 text-[13px] font-medium cursor-pointer text-t1 bg-hover border border-b1">
              {t("login_modal.try_again")}
            </button>
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}
