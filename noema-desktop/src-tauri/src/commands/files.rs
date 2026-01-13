//! File-related Tauri commands

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use std::sync::Arc;

use noema_core::storage::ids::AssetId;
use crate::logging::log_message;
use crate::state::AppState;

/// Save binary data to a file using the system save dialog
#[tauri::command]
pub async fn save_file(
    app: AppHandle,
    data: String,      // base64 encoded data
    filename: String,  // suggested filename
    mime_type: String, // mime type for file filter
) -> Result<bool, String> {
    use base64::Engine;

    log_message(&format!(
        "save_file called: filename={}, mime_type={}",
        filename, mime_type
    ));

    // Decode base64 data
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| {
            log_message(&format!("Failed to decode base64: {}", e));
            format!("Failed to decode data: {}", e)
        })?;

    log_message(&format!("Decoded {} bytes", bytes.len()));

    // Determine file extension from mime type
    let extension = mime_type.split('/').nth(1).unwrap_or("bin").to_string();

    // Use a channel to get the result from the dialog callback
    let (tx, rx) = tokio::sync::oneshot::channel();

    app.dialog()
        .file()
        .set_file_name(&filename)
        .add_filter(&mime_type, &[&extension])
        .save_file(move |file_path| {
            let _ = tx.send(file_path);
        });

    let file_path = rx.await.map_err(|e| format!("Dialog error: {}", e))?;

    log_message(&format!("Dialog returned: {:?}", file_path));

    if let Some(path) = file_path {
        let path_buf = path.as_path().ok_or("Invalid path")?;
        log_message(&format!("Writing to: {:?}", path_buf));
        std::fs::write(path_buf, &bytes).map_err(|e| {
            log_message(&format!("Failed to write: {}", e));
            format!("Failed to write file: {}", e)
        })?;
        log_message("File saved successfully");
        Ok(true)
    } else {
        log_message("User cancelled");
        Ok(false) // User cancelled
    }
}

// Note: Asset fetching is done via the noema-asset:// custom protocol
// which enables proper HTTP caching. See lib.rs for the protocol handler.
// Frontend fetches assets at: noema-asset://localhost/{asset_id}?mime_type={mime}

/// Store an asset in blob storage
///
/// Returns the asset ID (UUID) for referencing in messages.
#[tauri::command]
pub async fn store_asset(
    state: State<'_, Arc<AppState>>,
    data: String,      // base64 encoded
    mime_type: String,
) -> Result<AssetId, String> {
    let coordinator = state.get_coordinator()?;

    coordinator
        .store_asset(&data, &mime_type)
        .await
        .map_err(|e| format!("Failed to store asset: {}", e))
}
