use crate::{
    LauncherPaths,
    core::java_selection::{self, RuntimeChoice},
};
use std::{path::Path, sync::Arc};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn check_java_updates(
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<crate::core::java::ManualCheckResult, String> {
    crate::core::java::check_updates(&paths).await
}

#[tauri::command]
pub async fn validate_java_runtime(
    major: u32,
    path: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<RuntimeChoice, String> {
    java_selection::inspect_external(Path::new(&path), major, &paths)
}

#[tauri::command]
pub async fn pick_java_runtime(
    major: u32,
    window: tauri::WebviewWindow,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Option<RuntimeChoice>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let dialog = window
        .dialog()
        .file()
        .set_parent(&window)
        .set_title(format!("Java {major}"))
        .set_file_name(crate::core::java::JAVA_BIN);
    #[cfg(windows)]
    let dialog = dialog.add_filter("Java", &["exe"]);
    dialog.pick_file(move |file| {
        let _ = sender.send(file);
    });
    let Some(file) = receiver
        .await
        .map_err(|_| "Java file picker closed unexpectedly")?
    else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    java_selection::inspect_external(&path, major, &paths).map(Some)
}
