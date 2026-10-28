<script lang="ts">
  import { untrack } from "svelte";
  import { createTranslation } from "../../lib/i18n.svelte";
  import type { Profile } from "../../types";
  import ModsPanel from "../ModsPanel.svelte";
  import ContentLibraryPanel from "./ContentLibraryPanel.svelte";
  import { LibraryShell } from "./LibraryShell";
  import ServerLibraryPanel from "./ServerLibraryPanel.svelte";
  import WorldLibraryPanel from "./WorldLibraryPanel.svelte";

  type Surface = "mods" | "shaders" | "resource_packs" | "worlds" | "servers";
  let { profile, onBack, initialSurface }: { profile: Profile; onBack: () => void; initialSurface?: Surface } = $props();
  const { t } = createTranslation();
  let surface = $state<Surface | null>(untrack(() => initialSurface ?? null));
  let selection = $state(0);
  let overview = $state<HTMLDivElement>();
  const surfaces = $derived<Array<{ id: Surface; label: string }>>([
    ...(profile.loader === "vanilla" ? [] : [{ id: "mods" as const, label: t("library.mods") }]),
    { id: "shaders", label: t("library.shaders") },
    { id: "resource_packs", label: t("library.resource_packs") },
    { id: "worlds", label: t("library.worlds") },
    { id: "servers", label: t("library.servers") },
  ]);
  $effect(() => { if (!surface) overview?.focus(); });
  function back() { if (surface && !initialSurface) surface = null; else onBack(); }
</script>

<LibraryShell title={profile.name} subtitle={`${profile.loader} · ${profile.mcVersion}`} onBack={back}>
  <div class="profile-workspace">
    {#if !surface}
      <div bind:this={overview} class="profile-scope-overview" role="listbox" tabindex="0" aria-label={t("library.manage")} onkeydown={(event) => {
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault(); const delta = event.key === "ArrowDown" ? 1 : -1;
          selection = (selection + delta + surfaces.length) % surfaces.length;
        } else if (event.key === "Enter") { event.preventDefault(); surface = surfaces[selection].id; }
      }}>
        {#each surfaces as item, index (item.id)}
          <button role="option" aria-selected={selection === index} onmouseenter={() => selection = index} onclick={() => surface = item.id}>{item.label}</button>
        {/each}
      </div>
    {:else}
      <section class="profile-workspace-surface">
        {#if surface === "mods"}
          <ModsPanel profileId={profile.id} profileName={profile.name} mcVersion={profile.mcVersion} loader={profile.loader} onClose={back} embedded />
        {:else if surface === "shaders" || surface === "resource_packs"}
          <ContentLibraryPanel profileId={profile.id} kind={surface === "shaders" ? "shader" : "resource_pack"} />
        {:else if surface === "worlds"}<WorldLibraryPanel profileId={profile.id} />
        {:else if surface === "servers"}<ServerLibraryPanel profileId={profile.id} />{/if}
      </section>
    {/if}
  </div>
</LibraryShell>
