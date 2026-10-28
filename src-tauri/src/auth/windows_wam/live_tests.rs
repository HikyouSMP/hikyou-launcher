//! Opt-in, real WAM tests. Never run on CI or write Hikyou's credential store.
use super::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[test]
#[ignore = "requires native Windows UI and two user-operated Microsoft accounts"]
fn two_accounts_and_cancellation() {
    run_native_test(|app| Box::pin(async move { exercise(&app).await }));
}

#[test]
#[ignore = "requires Windows WAM cache and displays an automatically canceled native prompt"]
fn cache_refresh_and_owner_close() {
    run_native_test(|app| Box::pin(async move { exercise_automatic(&app).await }));
}

#[test]
#[ignore = "reads the user-authorized saved login; silent verification only, never commits"]
fn saved_account_silent() {
    run_native_test(|app| {
        Box::pin(async move {
            let saved = crate::auth::load_auth().await?;
            let refreshed = refresh(&app, &saved, &CancellationToken::new()).await?;
            if refreshed.uuid != saved.uuid {
                return Err("saved_account_profile_mismatch".into());
            }
            println!(
                "WAM live test: independent process loaded encrypted account reference and completed account-bound Minecraft authentication; no Hikyou credential files changed"
            );
            Ok(())
        })
    });
}

type TestFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;

fn run_native_test(task: fn(tauri::AppHandle) -> TestFuture) {
    assert!(
        !super::super::CLIENT_ID.is_empty(),
        "Set HIKYOU_MSA_CLIENT_ID before compiling this test"
    );
    let result = Arc::new(Mutex::new(None));
    let output = result.clone();
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .any_thread()
        .on_window_event(super::super::on_window_event)
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::External("about:blank".parse().unwrap()),
            )
            .title("Hikyou - Windows authentication verification")
            .inner_size(660.0, 180.0)
            .build()?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let result = task(handle.clone()).await;
                let code = if let Err(error) = &result {
                    eprintln!("WAM live test failed: {error}");
                    1
                } else {
                    0
                };
                *output.lock().unwrap() = Some(result);
                handle.exit(code);
            });
            Ok(())
        })
        .build(context)
        .expect("create isolated native test application");
    app.run(|_, event| {
        if let tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            api.prevent_exit();
        }
    });
    let result = result
        .lock()
        .unwrap()
        .take()
        .expect("native test completed");
    assert!(result.is_ok(), "{}", result.unwrap_err());
}

async fn exercise(app: &tauri::AppHandle) -> Result<(), String> {
    let cancel = CancellationToken::new();
    let beats = Arc::new(AtomicUsize::new(0));
    let heartbeat = {
        let beats = beats.clone();
        let app = app.clone();
        tokio::spawn(async move {
            loop {
                let beats = beats.clone();
                if app
                    .run_on_main_thread(move || {
                        beats.fetch_add(1, Ordering::Relaxed);
                    })
                    .is_err()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    };
    let outcome = async {
        app.get_webview_window("main").ok_or("owner missing")?
            .set_title("Hikyou WAM test - sign in with your FIRST account").map_err(|_| "title")?;
        let first = login(app, &cancel).await?;
        let refreshed = refresh(app, &first, &cancel).await?;
        if first.uuid != refreshed.uuid { return Err("first_account_silent_mismatch".into()); }
        println!("WAM live test: first login and account-bound silent acquisition passed");
        app.get_webview_window("main").ok_or("owner missing")?
            .set_title("Hikyou WAM test - sign in with a DIFFERENT SECOND account").map_err(|_| "title")?;
        let second = acquire(app, None, &cancel).await?;
        if first.microsoft_account_key.as_deref() == Some(second.account_key.as_str()) { return Err("Select a different second Microsoft account to complete this test.".into()); }
        let first_again = refresh(app, &first, &cancel).await?;
        let second_again = acquire(app, Some(&second.account_ref), &cancel).await?;
        if first.uuid != first_again.uuid || second.account_key != second_again.account_key { return Err("cross_account_silent_mismatch".into()); }
        println!("WAM live test: two distinct accounts and bound silent acquisition passed");
        let canceled = CancellationToken::new();
        let trigger = canceled.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(700)).await;
            trigger.cancel();
        });
        let result = login(app, &canceled).await;
        if !matches!(result, Err(ref error) if error == session::CANCELLED) { return Err("cancellation_failed".into()); }
        if beats.load(Ordering::Relaxed) < 3 { return Err("UI did not process heartbeat callbacks".into()); }
        println!("WAM live test: cancellation and responsive native event loop passed; no credentials saved");
        Ok(())
    }.await;
    heartbeat.abort();
    outcome
}

async fn exercise_automatic(app: &tauri::AppHandle) -> Result<(), String> {
    let cancel = CancellationToken::new();
    let pending = on_ui(app, &cancel, || {
        Ok(Pending::new(win(
            "test_provider",
            Wam::FindAccountProviderWithAuthorityAsync(
                &HSTRING::from("https://login.microsoft.com"),
                &HSTRING::from("consumers"),
            ),
        )?))
    })
    .await?;
    let provider = pending.wait(&cancel, Duration::from_secs(20)).await?;
    let pending = on_ui(app, &cancel, move || {
        let request = create_request(&provider, super::super::CLIENT_ID)?;
        Ok(Pending::new(win(
            "test_silent",
            Wam::GetTokenSilentlyAsync(&request),
        )?))
    })
    .await?;
    let result = pending.wait(&cancel, Duration::from_secs(60)).await?;
    let before = on_ui(app, &cancel, move || inspect_result(&result)).await?;
    invalidate(app, before.cached_result.clone(), &cancel).await?;
    let after = acquire(app, Some(&before.account_ref), &cancel).await?;
    verify_account(&before.account_ref, &after.account_ref)?;
    println!(
        "WAM live test: cache invalidation and bound reacquisition passed; token_changed={}",
        before.access_token.as_str() != after.access_token.as_str()
    );

    let beats = Arc::new(AtomicUsize::new(0));
    let delay_ms = Arc::new(AtomicUsize::new(0));
    let heartbeat = {
        let app = app.clone();
        let beats = beats.clone();
        let delay_ms = delay_ms.clone();
        tokio::spawn(async move {
            loop {
                let queued = std::time::Instant::now();
                let beats = beats.clone();
                let delay_ms = delay_ms.clone();
                if app
                    .run_on_main_thread(move || {
                        beats.fetch_add(1, Ordering::Relaxed);
                        delay_ms
                            .fetch_max(queued.elapsed().as_millis() as usize, Ordering::Relaxed);
                    })
                    .is_err()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
    };
    let outcome = async {
        let deadline = tokio::time::timeout(Duration::from_millis(500), acquire(app, None, &cancel)).await;
        if deadline.is_ok() { return Err("test expected the interactive request still to be pending".into()); }
        println!("WAM live test: dropping a timed-out acquisition passed");
        let session = session::begin().await?;
        let owner = app.get_webview_window("main").ok_or("owner missing")?;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            let _ = owner.close();
        });
        let result = tokio::time::timeout(Duration::from_secs(10), acquire(app, None, &session.token)).await
            .map_err(|_| "owner-close cancellation timed out")?;
        if !matches!(result, Err(ref error) if error == session::CANCELLED) {
            return Err("owner-close did not cancel the active authentication operation".into());
        }
        let count = beats.load(Ordering::Relaxed);
        let delay = delay_ms.load(Ordering::Relaxed);
        if count < 3 || delay > 1000 { return Err(format!("UI heartbeat failed: count={count}, delay_ms={delay}")); }
        println!("WAM live test: owner close cancels production operation; UI callbacks={count}, max_dispatch_delay_ms={delay}; no credentials saved");
        Ok(())
    }.await;
    heartbeat.abort();
    outcome
}
