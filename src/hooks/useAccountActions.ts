import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type { LauncherSettings, LoginState, StoredAuth } from "../types";

interface UseAccountActionsParams {
  savedAuth: StoredAuth | null;
  setSavedAuth: Dispatch<SetStateAction<StoredAuth | null>>;
  settingsRef: MutableRefObject<LauncherSettings>;
  updateSettings: (patch: (settings: LauncherSettings) => LauncherSettings) => void;
  setSkinImgError: Dispatch<SetStateAction<boolean>>;
  setLoginModalOpen: Dispatch<SetStateAction<boolean>>;
  setLoginState: Dispatch<SetStateAction<LoginState>>;
  setErrorMessage: Dispatch<SetStateAction<string | undefined>>;
  loginWindowTitle: string;
}

export function useAccountActions({
  savedAuth,
  setSavedAuth,
  settingsRef,
  updateSettings,
  setSkinImgError,
  setLoginModalOpen,
  setLoginState,
  setErrorMessage,
  loginWindowTitle,
}: UseAccountActionsParams) {
  const handleLogoutAccount = async (account: StoredAuth | null) => {
    const target = account ?? savedAuth;
    const uuid = target?.uuid;

    if (uuid) {
      await invoke("delete_account_auth_cmd", { uuid }).catch(console.error);
    }

    const remaining = settingsRef.current.accounts.filter(
      (account) =>
        uuid
          ? account.uuid !== uuid
          : account.username !== target?.username,
    );
    const removedActive =
      uuid != null
        ? uuid === settingsRef.current.activeAccountUuid
        : target?.username === savedAuth?.username;
    const nextAccount = removedActive ? remaining[0] ?? null : savedAuth;
    updateSettings((settings) => ({
      ...settings,
      accounts: remaining,
      activeAccountUuid: removedActive
        ? nextAccount?.uuid ?? null
        : settings.activeAccountUuid,
    }));

    if (removedActive && nextAccount?.uuid) {
      await invoke("switch_account", { uuid: nextAccount.uuid }).catch(
        console.error,
      );
      setSavedAuth(nextAccount);
    } else if (removedActive) {
      await invoke("logout").catch(console.error);
      setSavedAuth(null);
    }
    setSkinImgError(false);
  };

  const handleLogout = async () => {
    await handleLogoutAccount(savedAuth);
  };

  const handleWebviewLogin = async () => {
    setErrorMessage(undefined);
    const window = getCurrentWindow();

    try {
      const auth = await invoke<StoredAuth>("start_webview_login", {
        windowTitle: loginWindowTitle,
      });
      await window.show();
      await window.setFocus();

      setSavedAuth(auth);
      setSkinImgError(false);
      updateSettings((settings) => {
        const accounts = settings.accounts.filter(
          (account) => account.uuid !== auth.uuid,
        );
        return {
          ...settings,
          accounts: [auth, ...accounts],
          activeAccountUuid: auth.uuid ?? null,
        };
      });
      setLoginState("success");
      setLoginModalOpen(true);
      setTimeout(() => {
        setLoginModalOpen(false);
        setLoginState("idle");
      }, 2000);
    } catch (error) {
      const message = String(error);
      if (message.includes("__user_cancelled__")) {
        setLoginState("idle");
        return;
      }

      await window.show();
      await window.setFocus();
      setLoginState("error");
      setErrorMessage(message);
      setLoginModalOpen(true);
    }
  };

  const handleSwitchAccount = async (account: StoredAuth) => {
    if (!account.uuid) return;

    try {
      await invoke("switch_account", { uuid: account.uuid });
      setSavedAuth(account);
      setSkinImgError(false);
      updateSettings((settings) => ({
        ...settings,
        activeAccountUuid: account.uuid ?? null,
      }));
    } catch (error) {
      console.error(error);
    }
  };

  return {
    handleLogout,
    handleLogoutAccount,
    handleWebviewLogin,
    handleSwitchAccount,
  };
}
