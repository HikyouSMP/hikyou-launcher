use std::sync::{Arc, Mutex};
use windows::{
    Foundation::{IAsyncAction, TypedEventHandler},
    UI::ApplicationSettings::{
        AccountsSettingsPane, AccountsSettingsPaneCommandsRequestedEventArgs,
        WebAccountProviderCommand, WebAccountProviderCommandInvokedHandler,
    },
    Win32::System::WinRT::IAccountsSettingsPaneInterop,
    core::AgileReference,
};

use super::{operation::PendingAction, *};

// Keep the apartment-bound pane behind an OS-marshaled reference. Every exit,
// including a dropped future, unregisters its handler on the owning UI thread.
struct Registration {
    app: tauri::AppHandle,
    pane: AgileReference<AccountsSettingsPane>,
    cookie: windows::Foundation::EventRegistrationToken,
    active: CancellationToken,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.active.cancel();
        let pane = self.pane.clone();
        let cookie = self.cookie;
        let _ = self.app.run_on_main_thread(move || {
            if let Ok(pane) = pane.resolve() {
                let _ = pane.RemoveAccountCommandsRequested(cookie);
            }
        });
    }
}

pub(super) async fn select_provider(
    app: &tauri::AppHandle,
    provider: WebAccountProvider,
    cancel: &CancellationToken,
) -> Result<WebAccountProvider, String> {
    let selection = Arc::new(Mutex::new(None));
    let output = selection.clone();
    let owner = app.clone();
    let active = cancel.child_token();
    let (_registration, pending) = on_ui(app, cancel, move || {
        let window = owner
            .get_webview_window("main")
            .ok_or("The main window is unavailable.")?;
        let hwnd = window
            .hwnd()
            .map_err(|_| "The main window handle is unavailable.")?;
        let interop: IAccountsSettingsPaneInterop = win(
            "picker_interop",
            factory::<AccountsSettingsPane, IAccountsSettingsPaneInterop>(),
        )?;
        let pane: AccountsSettingsPane =
            win("picker_pane", unsafe { interop.GetForWindow(HWND(hwnd.0)) })?;
        let reference = win("picker_reference", AgileReference::new(&pane))?;
        let invoked = {
            let output = output.clone();
            let active = active.clone();
            WebAccountProviderCommandInvokedHandler::new(move |command| {
                if !active.is_cancelled() {
                    let result = command
                        .ok_or_else(|| "Windows account selection was unavailable.".to_string())
                        .and_then(|command| win("picker_provider", command.WebAccountProvider()));
                    *output.lock().unwrap_or_else(|error| error.into_inner()) = Some(result);
                }
                Ok(())
            })
        };
        let invoked = win("picker_callback_reference", AgileReference::new(&invoked))?;
        // The native picker owns account discovery; Hikyou neither enumerates
        // nor copies the OS account list. This is Microsoft's consumer WAM flow.
        let handler = TypedEventHandler::new({
            let active = active.clone();
            move |_: &Option<AccountsSettingsPane>,
                  args: &Option<AccountsSettingsPaneCommandsRequestedEventArgs>| {
                if active.is_cancelled() {
                    return Ok(());
                }
                let Some(args) = args else { return Ok(()) };
                let deferral = args.GetDeferral()?;
                let result = (|| {
                    let command = WebAccountProviderCommand::CreateWebAccountProviderCommand(
                        &provider,
                        &invoked.resolve()?,
                    )?;
                    args.WebAccountProviderCommands()?.Append(&command)
                })();
                let result = result.and(deferral.Complete());
                if let Err(error) = &result {
                    *output.lock().unwrap_or_else(|error| error.into_inner()) =
                        Some(win("picker_commands", Err(error.clone())));
                }
                result
            }
        });
        let cookie = win("picker_register", pane.AccountCommandsRequested(&handler))?;
        let registration = Registration {
            app: owner,
            pane: reference,
            cookie,
            active,
        };
        let action: IAsyncAction = win("picker_show", unsafe {
            interop.ShowAddAccountForWindowAsync(HWND(hwnd.0))
        })?;
        Ok((registration, PendingAction::new(action)))
    })
    .await?;
    pending.wait(cancel, Duration::from_secs(600)).await?;
    selection
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take()
        .unwrap_or_else(|| Err(session::CANCELLED.into()))
}
