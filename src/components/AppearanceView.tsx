import { invoke } from "@tauri-apps/api/core";
import { Check, Upload, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { SkinViewer } from "skinview3d";

import type { MinecraftAppearance } from "../types";
import { readFileBytes } from "../utils/fileImport";
import { LibraryShell } from "./library/LibraryShell";

type PlayerModel = "classic" | "slim";
type PendingSkin = { filename: string; bytes: number[]; previewUrl: string };

function activeSkinOf(appearance: MinecraftAppearance | null) {
  return appearance?.skins.find((skin) => skin.state.toUpperCase() === "ACTIVE") ?? appearance?.skins[0];
}

function activeCapeOf(appearance: MinecraftAppearance | null) {
  return appearance?.capes.find((cape) => cape.state.toUpperCase() === "ACTIVE");
}

function modelOf(appearance: MinecraftAppearance | null): PlayerModel {
  return activeSkinOf(appearance)?.variant.toLowerCase() === "slim" ? "slim" : "classic";
}

function PlayerPreview({
  name,
  skinUrl,
  capeUrl,
  model,
}: {
  name?: string;
  skinUrl?: string;
  capeUrl?: string;
  model: PlayerModel;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const viewerRef = useRef<SkinViewer | null>(null);
  const modelRef = useRef(model);
  const skinRequestRef = useRef(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    const stage = stageRef.current;
    if (!canvas || !stage) return;

    const viewer = new SkinViewer({ canvas, width: stage.clientWidth, height: stage.clientHeight, zoom: 0.92 });
    viewer.pixelRatio = "match-device";
    viewer.background = null;
    viewer.autoRotate = false;
    viewer.animation = null;
    viewerRef.current = viewer;

    const resize = new ResizeObserver(([entry]) => {
      viewer.setSize(
        Math.max(1, Math.floor(entry.contentRect.width)),
        Math.max(1, Math.floor(entry.contentRect.height)),
      );
    });
    resize.observe(stage);
    return () => {
      resize.disconnect();
      viewerRef.current = null;
      viewer.dispose();
    };
  }, []);

  useEffect(() => {
    modelRef.current = model;
    const viewer = viewerRef.current;
    if (viewer) viewer.playerObject.skin.modelType = model === "slim" ? "slim" : "default";
  }, [model]);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer || !skinUrl) return;
    const request = ++skinRequestRef.current;
    void viewer.loadSkin(skinUrl, { model: modelRef.current === "slim" ? "slim" : "default" }).then(() => {
      if (request === skinRequestRef.current) {
        viewer.playerObject.skin.modelType = modelRef.current === "slim" ? "slim" : "default";
      }
    });
  }, [skinUrl]);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer) return;
    if (capeUrl) void viewer.loadCape(capeUrl);
    else viewer.resetCape();
  }, [capeUrl]);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (viewer) viewer.nameTag = name || null;
  }, [name]);

  return (
    <div ref={stageRef} className="appearance-stage">
      <canvas ref={canvasRef} aria-label={name ?? "Minecraft player"} />
    </div>
  );
}

function ModelFigure({ slim }: { slim: boolean }) {
  return (
    <span className={`model-figure ${slim ? "slim" : "classic"}`} aria-hidden="true">
      <i className="model-head" /><i className="model-body" /><i className="model-arm left" />
      <i className="model-arm right" /><i className="model-leg left" /><i className="model-leg right" />
    </span>
  );
}

function CapeTexture({ url }: { url: string }) {
  return <span className="cape-texture" aria-hidden="true"><img src={url} alt="" /></span>;
}

export function AppearanceView({ onBack }: { onBack: () => void }) {
  const { t } = useTranslation();
  const [appearance, setAppearance] = useState<MinecraftAppearance | null>(null);
  const [draftModel, setDraftModel] = useState<PlayerModel>("classic");
  const [draftCapeId, setDraftCapeId] = useState<string | null>(null);
  const [pendingSkin, setPendingSkin] = useState<PendingSkin | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const adoptAppearance = useCallback((next: MinecraftAppearance) => {
    setAppearance(next);
    setDraftModel(modelOf(next));
    setDraftCapeId(activeCapeOf(next)?.id ?? null);
  }, []);

  const reload = useCallback(async () => {
    try {
      adoptAppearance(await invoke("get_account_appearance"));
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  }, [adoptAppearance]);

  useEffect(() => void reload(), [reload]);
  useEffect(() => () => {
    if (pendingSkin) URL.revokeObjectURL(pendingSkin.previewUrl);
  }, [pendingSkin]);

  useEffect(() => {
    appearance?.capes.forEach((cape) => {
      const image = new Image();
      image.src = cape.url;
    });
  }, [appearance?.capes]);

  const savedModel = modelOf(appearance);
  const savedCapeId = activeCapeOf(appearance)?.id ?? null;
  const dirty = Boolean(pendingSkin) || draftModel !== savedModel || draftCapeId !== savedCapeId;
  const draftCape = appearance?.capes.find((cape) => cape.id === draftCapeId);
  const skinUrl = pendingSkin?.previewUrl ?? activeSkinOf(appearance)?.url;

  const discardPendingSkin = () => {
    if (pendingSkin) URL.revokeObjectURL(pendingSkin.previewUrl);
    setPendingSkin(null);
  };

  const save = async () => {
    if (!appearance || !dirty || saving) return;
    setSaving(true);
    setError(null);
    try {
      let next = appearance;
      if (pendingSkin) {
        next = await invoke("upload_account_skin", {
          filename: pendingSkin.filename,
          variant: draftModel,
          bytes: pendingSkin.bytes,
        });
      } else if (draftModel !== savedModel) {
        next = await invoke("set_account_skin_variant", { variant: draftModel });
      }
      const serverCapeId = activeCapeOf(next)?.id ?? null;
      if (draftCapeId !== serverCapeId) {
        next = await invoke("set_account_cape", { capeId: draftCapeId });
      }
      discardPendingSkin();
      adoptAppearance(next);
      onBack();
    } catch (reason) {
      setError(String(reason));
      discardPendingSkin();
      await reload();
    } finally {
      setSaving(false);
    }
  };

  return (
    <LibraryShell title={t("appearance.title")} onBack={onBack}>
      <div className="appearance-editor">
        <PlayerPreview
          name={appearance?.name}
          skinUrl={skinUrl}
          capeUrl={draftCape?.url}
          model={draftModel}
        />

        <div className="appearance-controls">
          <section className="appearance-control-group">
            <div className="appearance-section-heading">
              <h2>{t("appearance.skin")}</h2>
              <button className="appearance-upload" onClick={() => inputRef.current?.click()} disabled={saving}>
                <Upload size={15} /> <span>{t("appearance.replace")}</span>
              </button>
            </div>
            <div className="model-picker" aria-label={t("appearance.model")}>
              {(["classic", "slim"] as const).map((value) => (
                <button
                  key={value}
                  aria-pressed={draftModel === value}
                  onClick={() => setDraftModel(value)}
                  disabled={saving}
                >
                  <ModelFigure slim={value === "slim"} />
                  <span>{value === "classic" ? t("appearance.classic") : t("appearance.slim")}</span>
                  {draftModel === value && <Check className="selection-check" size={14} />}
                </button>
              ))}
            </div>
            <input
              ref={inputRef}
              type="file"
              accept="image/png"
              className="hidden"
              onChange={async (event) => {
                const file = event.currentTarget.files?.[0];
                event.currentTarget.value = "";
                if (!file) return;
                try {
                  const bytes = await readFileBytes(file, 4 * 1024 * 1024);
                  discardPendingSkin();
                  setPendingSkin({ filename: file.name, bytes, previewUrl: URL.createObjectURL(file) });
                  setError(null);
                } catch (reason) {
                  setError(String(reason));
                }
              }}
            />
          </section>

          <section className="appearance-control-group">
            <div className="appearance-section-heading"><h2>{t("appearance.cape")}</h2></div>
            <div className="cape-picker">
              <button
                aria-pressed={draftCapeId === null}
                aria-label={t("appearance.no_cape")}
                title={t("appearance.no_cape")}
                onClick={() => setDraftCapeId(null)}
                disabled={saving}
              >
                <span className="cape-empty"><X size={17} /></span>
                {draftCapeId === null && <Check className="selection-check" size={14} />}
              </button>
              {appearance?.capes.map((cape) => {
                const selected = cape.id === draftCapeId;
                return (
                  <button
                    key={cape.id}
                    aria-pressed={selected}
                    aria-label={cape.alias}
                    title={cape.alias}
                    onClick={() => setDraftCapeId(cape.id)}
                    disabled={saving}
                  >
                    <CapeTexture url={cape.url} />
                    {selected && <Check className="selection-check" size={14} />}
                  </button>
                );
              })}
            </div>
          </section>
          {error && <div className="appearance-error">{error}</div>}
        </div>

        <footer className="appearance-actions">
          <button className="appearance-cancel" onClick={onBack} disabled={saving}>{t("common.cancel")}</button>
          <button className="appearance-save" onClick={() => void save()} disabled={!dirty || saving}>
            {saving ? t("common.saving") : t("common.save")}
          </button>
        </footer>
      </div>
    </LibraryShell>
  );
}
