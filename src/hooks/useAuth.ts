import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { LauncherSettings, StoredAuth } from "../types";

export function useAuth(
  updateSettings: (patch: (p: LauncherSettings) => LauncherSettings) => void,
) {
  const [savedAuth, setSavedAuth] = useState<StoredAuth | null>(null);
  const [authLoaded, setAuthLoaded] = useState(false);

  const loadSavedAuth = useCallback(() => {
    setAuthLoaded(false);
    return invoke<StoredAuth>("get_saved_auth")
      .then((auth) => {
        setSavedAuth(auth);
        updateSettings((settings) => {
          const exists = settings.accounts.some((x) => x.uuid === auth.uuid);
          if (exists) return settings;
          return {
            ...settings,
            accounts: [auth, ...settings.accounts],
            activeAccountUuid:
              settings.activeAccountUuid ?? auth.uuid ?? null,
          };
        });
      })
      .catch(() => setSavedAuth(null))
      .finally(() => setAuthLoaded(true));
  }, [updateSettings]);

  return { savedAuth, setSavedAuth, authLoaded, loadSavedAuth };
}
