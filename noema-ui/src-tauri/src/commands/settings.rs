//! Settings commands

use config::Settings;

/// Get the current user email setting
#[tauri::command]
pub fn get_user_email() -> Option<String> {
    Settings::load().user_email
}

/// Set the user email setting
#[tauri::command]
pub fn set_user_email(email: String) -> Result<(), String> {
    let mut settings = Settings::load();
    settings.user_email = Some(email);
    settings.save()
}
