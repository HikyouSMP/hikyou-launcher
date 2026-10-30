use crate::core::launch_report::{self, ReportContext, ReportPreview};

#[tauri::command]
pub fn prepare_launch_error_report(error: String) -> Result<ReportPreview, String> {
    launch_report::prepare(&error)
}

#[tauri::command]
pub fn open_launch_error_report(context: ReportContext, reviewed: bool) -> Result<(), String> {
    launch_report::open_issue(context, reviewed, |url| {
        open::that_detached(url).map_err(|_| "launch_report_open_failed".into())
    })
}
