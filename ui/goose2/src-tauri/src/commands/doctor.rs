//! Tauri command wrappers for the doctor health-check system.

pub use doctor::{DoctorReport, FixType};

/// Run all health checks and return the report.
#[tauri::command]
pub async fn run_doctor() -> DoctorReport {
    doctor::run_checks().await
}

/// Run a fix command for a doctor check, identified by check ID and fix type.
#[tauri::command]
pub async fn run_doctor_fix(check_id: String, fix_type: FixType) -> Result<(), String> {
    doctor::execute_fix(check_id, fix_type).await
}
