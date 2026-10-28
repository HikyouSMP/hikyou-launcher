<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import { C } from "../theme";
  const { t } = createTranslation();
</script>

<div style="padding:44px 24px;text-align:center">
  <p style={css({ fontSize: 13, fontWeight: 500, color: C.t2, marginBottom: 8 })}>{t("profile.empty_title")}</p>
  <p style={css({ fontSize: 12, color: C.t3, lineHeight: 1.7 })}>{t("profile.empty_hint_pre")} <span style={css({ color: C.green, fontWeight: 600 })}>1.21</span> {t("profile.empty_hint_post")}</p>
</div>
