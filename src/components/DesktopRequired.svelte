<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  const { t } = createTranslation();
</script>

<main class="flex h-screen items-center justify-center bg-app-bg p-6 text-t1" role="status">
  <div class="max-w-sm">
    <h1 class="mb-2 text-lg font-medium">Hikyou Launcher</h1>
    <p class="text-sm text-t2">{t("common.desktop_required")}</p>
  </div>
</main>
