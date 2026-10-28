<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import { C } from "../theme";
  let { query }: { query: string } = $props();
  const { t } = createTranslation();
</script>

<div style="padding:30px 24px;text-align:center"><p style={css({ fontSize: 12, color: C.t3 })}>{t("profile.no_match", { query })}</p></div>
