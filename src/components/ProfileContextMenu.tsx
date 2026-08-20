import { Boxes, Play, SlidersHorizontal, Square, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { Profile, StoredAuth } from "../types";

export interface ProfileContextMenuState {
  profileId: string;
  x: number;
  y: number;
}

export function ProfileContextMenu({
  menu,
  profiles,
  savedAuth,
  isProfileBusy,
  onClose,
  onLaunch,
  onStop,
  onLogin,
  onEditSettings,
  onManageMods,
  onDelete,
}: {
  menu: ProfileContextMenuState;
  profiles: Profile[];
  savedAuth: StoredAuth | null;
  isProfileBusy: (profileId: string) => boolean;
  onClose: () => void;
  onLaunch: (profile: Profile) => void;
  onStop: (profileId: string) => void;
  onLogin: () => void;
  onEditSettings: (profileId: string) => void;
  onManageMods: (profileId: string) => void;
  onDelete: (profileId: string) => void;
}) {
  const { t } = useTranslation();
  const profile = profiles.find((item) => item.id === menu.profileId);
  if (!profile) return null;

  const running = isProfileBusy(menu.profileId);
  const smart = profile.kind === "smart";
  const menuW = 168;
  const menuH = smart ? 76 : profile.loader !== "vanilla" ? 128 : 104;
  const x = Math.min(menu.x, 750 - menuW - 8);
  const y = Math.min(menu.y, 470 - menuH - 8);

  return (
    <>
      <div className="absolute inset-0 z-58" onMouseDown={onClose} />
      <div
        className="absolute z-60 menu-panel"
        style={{ left: x, top: y, width: menuW }}
        onMouseDown={(event) => event.stopPropagation()}
      >
        {running ? (
          <div
            className="menu-item danger"
            onClick={() => {
              onClose();
              onStop(profile.id);
            }}
          >
            <Square size={13} />
            {t("profile.stop")}
          </div>
        ) : (
          <div
            className="menu-item"
            onClick={() => {
              onClose();
              if (savedAuth) onLaunch(profile);
              else onLogin();
            }}
          >
            <Play size={13} />
            {t("profile.launch")}
          </div>
        )}
        <div
          className="menu-item"
          onClick={() => {
            onClose();
            onEditSettings(profile.id);
          }}
        >
          <SlidersHorizontal size={13} />
          {t("profile.edit_settings")}
        </div>
        {!smart && (
          <>
            {profile.loader !== "vanilla" && (
              <div
                className="menu-item"
                onClick={() => {
                  onClose();
                  onManageMods(profile.id);
                }}
              >
                <Boxes size={13} />
                {t("profile.manage_mods")}
              </div>
            )}
            <div className="h-px my-1 bg-b1" />
            <div
              className={`menu-item danger ${running ? "disabled" : ""}`}
              onClick={() => {
                if (running) return;
                onClose();
                onDelete(profile.id);
              }}
            >
              <Trash2 size={13} />
              {running ? t("profile.delete_busy") : t("common.delete")}
            </div>
          </>
        )}
      </div>
    </>
  );
}
