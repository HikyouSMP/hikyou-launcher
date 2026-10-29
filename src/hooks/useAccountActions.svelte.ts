import { onDestroy } from 'svelte';
import { type Setter, type Ref } from '../lib/state';
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { LauncherSettings, LoginState, StoredAuth } from "../types";
interface UseAccountActionsParams {
    savedAuth: StoredAuth | null;
    setSavedAuth: Setter<StoredAuth | null>;
    settingsRef: Ref<LauncherSettings>;
    updateSettings: (patch: (settings: LauncherSettings) => LauncherSettings) => void;
    setSkinImgError: Setter<boolean>;
    setLoginModalOpen: Setter<boolean>;
    setLoginState: Setter<LoginState>;
    setErrorMessage: Setter<string | undefined>;
    loginWindowTitle: string;
}
export function useAccountActions(params: UseAccountActionsParams) {
    let successTimer: ReturnType<typeof setTimeout> | null = null;
    let revision = 0;
    let pending = false;
    const cancelLogin = async () => {
        revision++;
        if (successTimer) clearTimeout(successTimer);
        await invoke("cancel_auth_login");
        pending = false;
        params.setLoginState("idle");
    };
    onDestroy(() => {
        revision++;
        if (successTimer) clearTimeout(successTimer);
        if (pending) void invoke("cancel_auth_login").catch(console.error);
    });
    const handleLogoutAccount = async (account: StoredAuth | null) => {
        await cancelLogin();
        const target = account ?? params.savedAuth;
        const uuid = target?.uuid;
        if (uuid) {
            await invoke("delete_account_auth_cmd", { uuid });
        }
        const remaining = params.settingsRef.current.accounts.filter((account) => uuid
            ? account.uuid !== uuid
            : account.username !== target?.username);
        const removedActive = uuid != null
            ? uuid === params.settingsRef.current.activeAccountUuid
            : target?.username === params.savedAuth?.username;
        const nextAccount = removedActive ? remaining[0] ?? null : params.savedAuth;
        params.updateSettings((settings) => ({
            ...settings,
            accounts: remaining,
            activeAccountUuid: removedActive
                ? nextAccount?.uuid ?? null
                : settings.activeAccountUuid,
        }));
        if (removedActive && nextAccount?.uuid) {
            await invoke("switch_account", { uuid: nextAccount.uuid });
            params.setSavedAuth(await invoke<StoredAuth>("get_saved_auth"));
        }
        else if (removedActive) {
            await invoke("logout");
            params.setSavedAuth(null);
        }
        params.setSkinImgError(false);
    };
    const handleLogout = async () => {
        await handleLogoutAccount(params.savedAuth);
    };
    const handleWebviewLogin = async () => {
        if (pending) return;
        pending = true;
        params.setLoginState("waiting");
        params.setLoginModalOpen(false);
        const request = ++revision;
        if (successTimer) clearTimeout(successTimer);
        params.setErrorMessage(undefined);
        const window = getCurrentWindow();
        try {
            await invoke("start_webview_login", {
                windowTitle: params.loginWindowTitle,
            });
            const auth = await invoke<StoredAuth>("get_saved_auth");
            if (request !== revision) return;
            await window.show();
            await window.setFocus();
            if (request !== revision) return;
            params.setSavedAuth(auth);
            params.setSkinImgError(false);
            params.updateSettings((settings) => {
                const accounts = settings.accounts.filter((account) => account.uuid !== auth.uuid);
                return {
                    ...settings,
                    accounts: [auth, ...accounts],
                    activeAccountUuid: auth.uuid ?? null,
                };
            });
            params.setLoginState("success");
            params.setLoginModalOpen(true);
            successTimer = setTimeout(() => {
                if (request !== revision) return;
                params.setLoginModalOpen(false);
                params.setLoginState("idle");
            }, 2000);
        }
        catch (error) {
            if (request !== revision) return;
            const message = String(error);
            await window.show();
            await window.setFocus();
            if (request !== revision) return;
            if (message.includes("__user_cancelled__")) {
                params.setLoginState("idle");
                return;
            }
            params.setLoginState("error");
            params.setErrorMessage(message);
            params.setLoginModalOpen(true);
        }
        finally {
            if (request === revision) {
                pending = false;
                params.setLoginState(current => current === "waiting" ? "idle" : current);
            }
        }
    };
    const handleSwitchAccount = async (account: StoredAuth) => {
        if (!account.uuid)
            return;
        try {
            await cancelLogin();
            await invoke("switch_account", { uuid: account.uuid });
            params.setSavedAuth(await invoke<StoredAuth>("get_saved_auth"));
            params.setSkinImgError(false);
            params.updateSettings((settings) => ({
                ...settings,
                activeAccountUuid: account.uuid ?? null,
            }));
        }
        catch (error) {
            console.error(error);
        }
    };
    return {
        cancelLogin,
        get handleLogout() {
            return handleLogout;
        },
        get handleLogoutAccount() {
            return handleLogoutAccount;
        },
        get handleWebviewLogin() {
            return handleWebviewLogin;
        },
        get handleSwitchAccount() {
            return handleSwitchAccount;
        }
    };
}
