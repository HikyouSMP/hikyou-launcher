import { Download } from "lucide-react";
import { useTranslation } from "react-i18next";

import { C } from "../theme";
import type { ModSearchResult, ModpackVersionInfo } from "../types";
import { ModalBackdrop } from "./ModalBackdrop";

export interface InstallingModpackVersion {
  projectId: string;
  versionId: string;
  title?: string;
}

export function ModpackVersionDialog({
  modpack,
  versions,
  isLoading,
  installing,
  focusedIndex,
  hoveredIndex,
  onHoveredIndexChange,
  onClose,
  onInstall,
}: {
  modpack: ModSearchResult;
  versions: ModpackVersionInfo[];
  isLoading: boolean;
  installing: InstallingModpackVersion | null;
  focusedIndex: number;
  hoveredIndex: number | null;
  onHoveredIndexChange: (index: number | null) => void;
  onClose: () => void;
  onInstall: (version: ModpackVersionInfo) => void;
}) {
  const { t } = useTranslation();
  const anyInstalling = installing !== null;

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 200,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
      onClick={anyInstalling ? undefined : onClose}
    >
      <ModalBackdrop fixed />
      <div
        onClick={(event) => event.stopPropagation()}
        style={{
          position: "relative",
          zIndex: 1,
          width: 268,
          maxHeight: 340,
          background: "rgba(34,33,31,.72)",
          backdropFilter: "blur(24px) saturate(155%)",
          WebkitBackdropFilter: "blur(24px) saturate(155%)",
          borderRadius: C.r + 2,
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          boxShadow: "0 12px 40px rgba(0,0,0,.5)",
        }}
      >
        <div
          style={{
            padding: "11px 13px 9px",
            borderBottom: `1px solid ${C.b2}`,
            flexShrink: 0,
          }}
        >
          <div
            style={{
              fontSize: 13,
              fontWeight: 500,
              color: C.t1,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              marginBottom: 1,
            }}
          >
            {modpack.title}
          </div>
          <div style={{ fontSize: 10, color: C.t3 }}>
            {t("modpack.select_version")}
          </div>
        </div>

        <div className="sb" style={{ overflowY: "auto", flex: 1, padding: "6px 0" }}>
          {isLoading && (
            <div
              style={{
                padding: "20px",
                textAlign: "center",
                fontSize: 11,
                color: C.t3,
              }}
            >
              {t("common.fetching")}
            </div>
          )}
          {!isLoading && versions.length === 0 && (
            <div
              style={{
                padding: "20px",
                textAlign: "center",
                fontSize: 11,
                color: C.t3,
              }}
            >
              {t("common.not_found")}
            </div>
          )}
          {!isLoading &&
            versions.map((version, index) => {
              const isInstalling =
                installing?.projectId === modpack.project_id &&
                installing?.versionId === version.id;
              const focused = focusedIndex === index;
              const hovered = hoveredIndex === index;
              return (
                <div
                  key={version.id}
                  data-modpack-version-idx={index}
                  onClick={() => onInstall(version)}
                  onMouseEnter={() => onHoveredIndexChange(index)}
                  onMouseLeave={() => onHoveredIndexChange(null)}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    padding: "6px 12px",
                    margin: "0 6px 1px",
                    borderRadius: C.r,
                    cursor: anyInstalling ? "default" : "pointer",
                    background: hovered || focused ? C.hover : "transparent",
                    opacity: anyInstalling && !isInstalling ? 0.4 : 1,
                    minHeight: 40,
                    scrollMarginTop: 8,
                    scrollMarginBottom: 8,
                  }}
                >
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div
                      style={{
                        fontSize: 13,
                        color: C.t1,
                        whiteSpace: "nowrap",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                      }}
                    >
                      {version.version_number}
                    </div>
                    <div
                      style={{
                        display: "flex",
                        gap: 3,
                        marginTop: 2,
                        flexWrap: "wrap",
                      }}
                    >
                      {version.game_versions.slice(0, 3).map((gameVersion) => (
                        <span
                          key={gameVersion}
                          style={{
                            fontSize: 9,
                            padding: "1px 5px",
                            borderRadius: 4,
                            background: "rgba(255,255,255,.06)",
                            color: C.t3,
                            lineHeight: 1.6,
                          }}
                        >
                          {gameVersion}
                        </span>
                      ))}
                    </div>
                  </div>
                  {isInstalling ? (
                    <div
                      style={{
                        width: 14,
                        height: 14,
                        borderRadius: "50%",
                        border: `1.5px solid ${C.t3}`,
                        borderTopColor: C.green,
                        animation: "spin .7s linear infinite",
                        flexShrink: 0,
                      }}
                    />
                  ) : (
                    (hovered || focused) &&
                    !anyInstalling && (
                      <Download size={13} style={{ color: C.green, flexShrink: 0 }} />
                    )
                  )}
                </div>
              );
            })}
        </div>
      </div>
    </div>
  );
}
