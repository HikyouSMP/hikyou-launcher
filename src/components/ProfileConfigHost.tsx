import { invoke } from "@tauri-apps/api/core";

import type { Profile } from "../types";
import { ProfileConfigPanel } from "./ProfileConfigPanel";

export function ProfileConfigHost({
  profile,
  globalDefaults,
  onClose,
  onSave,
  onDelete,
}: {
  profile: Profile;
  globalDefaults: { memoryMb: number; windowW: number; windowH: number };
  onClose: () => void;
  onSave: (
    id: string,
    changes: {
      name: string;
      memoryMb: number | null;
      windowW: number | null;
      windowH: number | null;
    },
  ) => void | Promise<void>;
  onDelete: (profileId: string) => void;
}) {
  return (
    <ProfileConfigPanel
      profile={profile}
      globalDefaults={globalDefaults}
      onClose={onClose}
      onSave={onSave}
      onDelete={onDelete}
      onOpenFolder={() => {
        invoke<Record<string, string>>("get_launcher_paths")
          .then((paths) => {
            const baseDir =
              profile.kind === "smart"
                ? String(paths.smart_profiles ?? "")
                : String(paths.profiles ?? "");
            const folderName =
              profile.kind === "smart" ? profile.smartKey : profile.id;
            if (!baseDir || !folderName) return;
            const separator = baseDir.includes("\\") ? "\\" : "/";
            const profileFolder = [
              baseDir,
              folderName,
              ".minecraft",
            ].join(separator);
            invoke("open_folder", { path: profileFolder }).catch(console.error);
          })
          .catch(console.error);
      }}
    />
  );
}
