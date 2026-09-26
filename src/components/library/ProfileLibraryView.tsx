import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { Profile } from "../../types";
import { ModsPanel } from "../ModsPanel";
import { ContentLibraryPanel } from "./ContentLibraryPanel";
import { LibraryShell } from "./LibraryShell";
import { ServerLibraryPanel } from "./ServerLibraryPanel";
import { WorldLibraryPanel } from "./WorldLibraryPanel";

type Surface = "mods" | "shaders" | "resource_packs" | "worlds" | "servers";

export function ProfileLibraryView({
  profile,
  onBack,
  initialSurface,
}: {
  profile: Profile;
  onBack: () => void;
  initialSurface?: Surface;
}) {
  const { t } = useTranslation();
  const [surface, setSurface] = useState<Surface | null>(initialSurface ?? null);
  const [selection, setSelection] = useState(0);
  const overviewRef = useRef<HTMLDivElement>(null);
  const surfaces: Array<{ id: Surface; label: string }> = [
    ...(profile.loader === "vanilla" ? [] : [{ id: "mods" as const, label: t("library.mods") }]),
    { id: "shaders", label: t("library.shaders") },
    { id: "resource_packs", label: t("library.resource_packs") },
    { id: "worlds", label: t("library.worlds") },
    { id: "servers", label: t("library.servers") },
  ];

  useEffect(() => {
    if (!surface) overviewRef.current?.focus();
  }, [surface]);

  const back = () => {
    if (surface && !initialSurface) setSurface(null);
    else onBack();
  };

  return (
    <LibraryShell
      title={profile.name}
      subtitle={`${profile.loader} · ${profile.mcVersion}`}
      onBack={back}
    >
      <div className="profile-workspace">
        {!surface ? (
          <div
            ref={overviewRef}
            className="profile-scope-overview"
            role="listbox"
            tabIndex={0}
            aria-label={t("library.manage")}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                const delta = event.key === "ArrowDown" ? 1 : -1;
                setSelection((value) => (value + delta + surfaces.length) % surfaces.length);
              } else if (event.key === "Enter") {
                event.preventDefault();
                setSurface(surfaces[selection].id);
              }
            }}
          >
            {surfaces.map((item, index) => (
              <button
                key={item.id}
                role="option"
                aria-selected={selection === index}
                onMouseEnter={() => setSelection(index)}
                onClick={() => setSurface(item.id)}
              >
                {item.label}
              </button>
            ))}
          </div>
        ) : (
          <section className="profile-workspace-surface">
          {surface === "mods" && (
            <ModsPanel
              profileId={profile.id}
              profileName={profile.name}
              mcVersion={profile.mcVersion}
              loader={profile.loader}
              onClose={back}
              embedded
            />
          )}
          {(surface === "shaders" || surface === "resource_packs") && (
            <ContentLibraryPanel profileId={profile.id} kind={surface === "shaders" ? "shader" : "resource_pack"} />
          )}
          {surface === "worlds" && <WorldLibraryPanel profileId={profile.id} />}
          {surface === "servers" && <ServerLibraryPanel profileId={profile.id} />}
          </section>
        )}
      </div>
    </LibraryShell>
  );
}
