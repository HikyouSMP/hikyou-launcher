<script lang="ts">
  import type { SearchSuggestion, SearchSuggestionAction } from "../search/suggestions";
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import { C } from "../theme";

  let { suggestion, onAction, compact = false }: {
    suggestion: SearchSuggestion | null;
    onAction: (action: SearchSuggestionAction) => void;
    compact?: boolean;
  } = $props();
  const { t } = createTranslation();
</script>

{#if suggestion}
  <button class="search-suggestion" class:compact data-command-action={suggestion.action} aria-keyshortcuts={suggestion.shortcut}
    aria-label={compact ? `${suggestion.shortcut}: ${t(suggestion.labelKey)}` : undefined}
    title={compact ? t(suggestion.labelKey) : undefined}
    onmousedown={(event) => { if (compact) event.preventDefault(); }}
    onclick={() => onAction(suggestion.action)}
    onkeydown={(event) => { if (event.key === "Enter" || event.key === " ") event.stopPropagation(); }}
    style={css({ "--suggestion-text": C.t2, "--suggestion-hover": C.t1, "--suggestion-border": C.b1 })}>
    <kbd>{suggestion.shortcut}</kbd>
    {#if !compact}<span>{t(suggestion.labelKey)}</span>{/if}
  </button>
{/if}

<style>
  .search-suggestion { display: inline-flex; align-items: center; justify-content: center; gap: 7px; height: 28px; max-width: 260px; padding: 0 5px; border: 0; border-radius: 4px; background: transparent; color: var(--suggestion-text); font: inherit; font-size: 12px; letter-spacing: 0; cursor: pointer; }
  .search-suggestion:hover, .search-suggestion:focus-visible { color: var(--suggestion-hover); }
  .search-suggestion:focus-visible { outline: 1px solid var(--suggestion-border); outline-offset: 0; }
  .search-suggestion kbd { flex-shrink: 0; padding: 1px 4px; border: 1px solid var(--suggestion-border); border-radius: 4px; font: inherit; font-size: 10px; line-height: 14px; }
  .search-suggestion span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .search-suggestion.compact { flex: none; padding: 0 2px; }
  .compact kbd { font-size: 11px; padding: 2px 5px; }
</style>
