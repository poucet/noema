//! Chat-related Tauri commands

use llm::{create_model, list_all_models, ChatPayload, ContentBlock};
use noema_core::{ChatEngine, EngineEvent, McpRegistry, SessionStore};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;
use crate::types::{Attachment, ConversationInfo, DisplayMessage, ModelInfo};

/// Initialize the application - sets up database and default model
#[tauri::command]
pub async fn init_app(_app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    use noema_core::SqliteStore;

    // Use the same database path as TUI: dirs::data_dir()/noema/conversations.db
    // On mobile, fall back to Tauri's app_data_dir
    #[cfg(not(target_os = "android"))]
    let db_path = {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("noema");
        if !data_dir.exists() {
            std::fs::create_dir_all(&data_dir)
                .map_err(|e| format!("Failed to create data dir: {}", e))?;
        }
        data_dir.join("conversations.db")
    };

    #[cfg(target_os = "android")]
    let db_path = {
        let app_dir = _app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {}", e))?;
        if !app_dir.exists() {
            std::fs::create_dir_all(&app_dir)
                .map_err(|e| format!("Failed to create app dir: {}", e))?;
        }
        app_dir.join("conversations.db")
    };

    // Load environment
    config::load_env_file();

    // Open database
    let store =
        SqliteStore::open(&db_path).map_err(|e| format!("Failed to open database: {}", e))?;

    // Create initial session
    let session = store
        .create_conversation()
        .map_err(|e| format!("Failed to create session: {}", e))?;

    let conversation_id = session.conversation_id().to_string();
    *state.current_conversation_id.lock().await = conversation_id.clone();

    // Create default model (Gemini)
    let default_model_id = "gemini/models/gemini-2.5-flash";
    let model = create_model(default_model_id)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    let model_display_name = default_model_id.split('/').last().unwrap_or(default_model_id);
    *state.model_id.lock().await = default_model_id.to_string();
    *state.model_name.lock().await = model_display_name.to_string();

    // Create MCP registry
    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

    // Create engine
    let engine = ChatEngine::new(session, model, model_display_name.to_string(), mcp_registry);

    *state.store.lock().await = Some(store);
    *state.engine.lock().await = Some(engine);

    Ok(model_display_name.to_string())
}

/// Get current messages in the conversation
#[tauri::command]
pub async fn get_messages(state: State<'_, AppState>) -> Result<Vec<DisplayMessage>, String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;

    let session_arc = engine.get_session();
    let session = session_arc.lock().await;

    Ok(session
        .messages()
        .iter()
        .map(DisplayMessage::from_chat_message)
        .collect())
}

/// Send a message and get streaming responses via events
#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    message: String,
) -> Result<(), String> {
    let payload = ChatPayload::text(message);
    send_message_internal(app, state, payload).await
}

/// Send a message with attachments
#[tauri::command]
pub async fn send_message_with_attachments(
    app: AppHandle,
    state: State<'_, AppState>,
    message: String,
    attachments: Vec<Attachment>,
) -> Result<(), String> {
    // Build content blocks from message and attachments
    let mut content = Vec::new();

    // Add text if non-empty
    if !message.trim().is_empty() {
        content.push(ContentBlock::Text { text: message });
    }

    // Add attachments
    for attachment in attachments {
        let mime_lower = attachment.mime_type.to_lowercase();
        if mime_lower.starts_with("image/") {
            content.push(ContentBlock::Image {
                data: attachment.data,
                mime_type: attachment.mime_type,
            });
        } else if mime_lower.starts_with("audio/") {
            content.push(ContentBlock::Audio {
                data: attachment.data,
                mime_type: attachment.mime_type,
            });
        } else if mime_lower.starts_with("text/") {
            // Text/markdown files: decode and add as text content
            match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &attachment.data) {
                Ok(bytes) => {
                    match String::from_utf8(bytes) {
                        Ok(text) => {
                            content.push(ContentBlock::Text { text });
                        }
                        Err(e) => {
                            return Err(format!("Failed to decode text file as UTF-8: {}", e));
                        }
                    }
                }
                Err(e) => {
                    return Err(format!("Failed to decode base64: {}", e));
                }
            }
        } else if mime_lower == "application/pdf" {
            // PDF files: extract text and images
            match process_pdf_attachment(&attachment.data) {
                Ok(blocks) => {
                    content.extend(blocks);
                }
                Err(e) => {
                    return Err(format!("Failed to process PDF: {}", e));
                }
            }
        }
        // Ignore other unsupported types
    }

    if content.is_empty() {
        return Err("Message must have text or attachments".to_string());
    }

    let payload = ChatPayload { content };
    send_message_internal(app, state, payload).await
}

/// Process a PDF attachment and extract text and images
fn process_pdf_attachment(base64_data: &str) -> Result<Vec<ContentBlock>, String> {
    use base64::Engine;

    // Decode base64 to bytes
    let pdf_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| format!("Failed to decode PDF base64: {}", e))?;

    let mut blocks = Vec::new();

    // Extract text from PDF
    match pdf_extract::extract_text_from_mem(&pdf_bytes) {
        Ok(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                blocks.push(ContentBlock::Text {
                    text: format!("[PDF Content]\n{}", trimmed),
                });
            }
        }
        Err(e) => {
            // Log error but continue
            crate::log_message(&format!("PDF text extraction failed: {}", e));
        }
    }

    // Extract embedded images by scanning all document objects
    // (not just page XObjects, which may not always be properly linked)
    if let Ok(doc) = lopdf::Document::load_mem(&pdf_bytes) {
        for (_obj_id, obj) in doc.objects.iter() {
            if let lopdf::Object::Stream(stream) = obj {
                if let Some(image_block) = extract_image_from_stream(stream) {
                    blocks.push(image_block);
                }
            }
        }
    }

    if blocks.is_empty() {
        return Err("Could not extract any content from PDF".to_string());
    }

    Ok(blocks)
}

/// Extract an image from a lopdf Stream object
fn extract_image_from_stream(stream: &lopdf::Stream) -> Option<ContentBlock> {
    use base64::Engine;
    use lopdf::Object;

    let dict = &stream.dict;

    // Check if this is an image XObject
    let subtype = dict.get(b"Subtype").ok()?;
    if let Object::Name(name) = subtype {
        if name != b"Image" {
            return None;
        }
    } else {
        return None;
    }

    // Get image properties
    let width = match dict.get(b"Width").ok()? {
        Object::Integer(w) => *w as u32,
        _ => return None,
    };
    let height = match dict.get(b"Height").ok()? {
        Object::Integer(h) => *h as u32,
        _ => return None,
    };

    if width == 0 || height == 0 {
        return None;
    }

    let bits = dict
        .get(b"BitsPerComponent")
        .ok()
        .and_then(|o| match o {
            Object::Integer(b) => Some(*b as u8),
            _ => None,
        })
        .unwrap_or(8);

    // Get color space
    let color_space = dict.get(b"ColorSpace").ok().and_then(|o| match o {
        Object::Name(name) => Some(String::from_utf8_lossy(name).to_string()),
        _ => None,
    });

    // Get filter(s)
    let filters: Vec<String> = match dict.get(b"Filter").ok() {
        Some(Object::Name(name)) => vec![String::from_utf8_lossy(name).to_string()],
        Some(Object::Array(arr)) => arr
            .iter()
            .filter_map(|o| match o {
                Object::Name(name) => Some(String::from_utf8_lossy(name).to_string()),
                _ => None,
            })
            .collect(),
        _ => vec![],
    };

    let image_data = &stream.content;

    // Check for DCTDecode (JPEG) - data can be used directly
    if filters.iter().any(|f| f == "DCTDecode") {
        if image_data.starts_with(&[0xFF, 0xD8]) {
            let base64_data = base64::engine::general_purpose::STANDARD.encode(image_data);
            return Some(ContentBlock::Image {
                data: base64_data,
                mime_type: "image/jpeg".to_string(),
            });
        }
    }

    // Check for FlateDecode (compressed raw bitmap) - decompress and convert to PNG
    if filters.iter().any(|f| f == "FlateDecode") {
        return extract_flate_from_stream(image_data, width, height, bits, color_space.as_deref());
    }

    // Try raw uncompressed image data
    if filters.is_empty() {
        return convert_raw_to_png(image_data, width, height, bits, color_space.as_deref());
    }

    None
}

/// Extract a FlateDecode compressed image from raw stream data
fn extract_flate_from_stream(
    data: &[u8],
    width: u32,
    height: u32,
    bits: u8,
    color_space: Option<&str>,
) -> Option<ContentBlock> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    if decoder.read_to_end(&mut decompressed).is_err() {
        return None;
    }

    convert_raw_to_png(&decompressed, width, height, bits, color_space)
}

/// Convert raw pixel data to PNG
fn convert_raw_to_png(
    data: &[u8],
    width: u32,
    height: u32,
    bits: u8,
    color_space: Option<&str>,
) -> Option<ContentBlock> {
    use base64::Engine;
    use image::{ImageBuffer, Rgb, Rgba, Luma};

    // Determine color type from color space
    let is_rgb = color_space.map_or(false, |cs| cs.contains("RGB"));
    let is_gray = color_space.map_or(false, |cs| cs.contains("Gray"));

    let png_data = if is_rgb && bits == 8 {
        // RGB image
        let expected_size = (width * height * 3) as usize;
        if data.len() < expected_size {
            return None;
        }
        let img: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_raw(width, height, data[..expected_size].to_vec())?;
        let mut png_bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png_bytes, image::ImageFormat::Png).ok()?;
        png_bytes.into_inner()
    } else if is_gray && bits == 8 {
        // Grayscale image
        let expected_size = (width * height) as usize;
        if data.len() < expected_size {
            return None;
        }
        let img: ImageBuffer<Luma<u8>, _> = ImageBuffer::from_raw(width, height, data[..expected_size].to_vec())?;
        let mut png_bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png_bytes, image::ImageFormat::Png).ok()?;
        png_bytes.into_inner()
    } else {
        // Default: assume RGB or try RGBA
        let rgb_size = (width * height * 3) as usize;
        let rgba_size = (width * height * 4) as usize;

        if data.len() >= rgba_size {
            let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(width, height, data[..rgba_size].to_vec())?;
            let mut png_bytes = std::io::Cursor::new(Vec::new());
            img.write_to(&mut png_bytes, image::ImageFormat::Png).ok()?;
            png_bytes.into_inner()
        } else if data.len() >= rgb_size {
            let img: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_raw(width, height, data[..rgb_size].to_vec())?;
            let mut png_bytes = std::io::Cursor::new(Vec::new());
            img.write_to(&mut png_bytes, image::ImageFormat::Png).ok()?;
            png_bytes.into_inner()
        } else {
            return None;
        }
    };

    let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_data);
    Some(ContentBlock::Image {
        data: base64_data,
        mime_type: "image/png".to_string(),
    })
}

/// Internal helper for sending messages
async fn send_message_internal(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: ChatPayload,
) -> Result<(), String> {
    // Check if already processing - if so, queue this message
    {
        let is_processing = *state.is_processing.lock().await;
        if is_processing {
            // Already processing, the message will be queued in the engine
            // but we shouldn't spawn another polling task
            let engine_guard = state.engine.lock().await;
            let engine = engine_guard.as_ref().ok_or("App not initialized")?;
            engine.send_message(payload);
            return Ok(());
        }
    }

    // Mark as processing
    *state.is_processing.lock().await = true;

    // Emit user message immediately
    let user_msg = DisplayMessage::from_payload(&payload);
    app.emit("user_message", &user_msg)
        .map_err(|e| e.to_string())?;

    // Send to engine
    {
        let engine_guard = state.engine.lock().await;
        let engine = engine_guard.as_ref().ok_or("App not initialized")?;
        engine.send_message(payload);
    }

    // Spawn a task to poll for events
    let app_handle = app.clone();
    tokio::spawn(async move {
        let state = app_handle.state::<AppState>();

        // Use a scope guard pattern to ensure is_processing is always reset
        struct ProcessingGuard<'a> {
            state: &'a AppState,
            completed: bool,
        }

        impl<'a> ProcessingGuard<'a> {
            fn new(state: &'a AppState) -> Self {
                Self { state, completed: false }
            }

            fn mark_completed(&mut self) {
                self.completed = true;
            }
        }

        impl Drop for ProcessingGuard<'_> {
            fn drop(&mut self) {
                if !self.completed {
                    // If we're dropping without completing, reset is_processing
                    // Use blocking lock since we're in drop
                    if let Ok(mut guard) = self.state.is_processing.try_lock() {
                        *guard = false;
                    }
                }
            }
        }

        let mut guard = ProcessingGuard::new(&state);

        loop {
            let event = {
                let mut engine_guard = state.engine.lock().await;
                let engine = match engine_guard.as_mut() {
                    Some(e) => e,
                    None => break,
                };
                engine.try_recv()
            };

            match event {
                Some(EngineEvent::Message(msg)) => {
                    let display_msg = DisplayMessage::from_chat_message(&msg);
                    let _ = app_handle.emit("streaming_message", &display_msg);
                }
                Some(EngineEvent::MessageComplete) => {
                    // Get all messages after completion
                    let messages = {
                        let engine_guard = state.engine.lock().await;
                        if let Some(engine) = engine_guard.as_ref() {
                            let session_arc = engine.get_session();
                            let session = session_arc.lock().await;
                            session
                                .messages()
                                .iter()
                                .map(DisplayMessage::from_chat_message)
                                .collect::<Vec<_>>()
                        } else {
                            vec![]
                        }
                    };
                    let _ = app_handle.emit("message_complete", &messages);
                    // Mark as no longer processing
                    *state.is_processing.lock().await = false;
                    guard.mark_completed();
                    break;
                }
                Some(EngineEvent::Error(err)) => {
                    let _ = app_handle.emit("error", &err);
                    // Mark as no longer processing
                    *state.is_processing.lock().await = false;
                    guard.mark_completed();
                    break;
                }
                Some(EngineEvent::ModelChanged(name)) => {
                    let _ = app_handle.emit("model_changed", &name);
                }
                Some(EngineEvent::HistoryCleared) => {
                    let _ = app_handle.emit("history_cleared", ());
                }
                None => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
            }
        }
    });

    Ok(())
}

/// Clear conversation history
#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    let engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_ref().ok_or("App not initialized")?;
    engine.clear_history();
    Ok(())
}

/// Set the current model
#[tauri::command]
pub async fn set_model(
    state: State<'_, AppState>,
    model_id: String,
    provider: String,
) -> Result<String, String> {
    // Construct full model ID as "provider/model"
    let full_model_id = format!("{}/{}", provider, model_id);

    let new_model = create_model(&full_model_id)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    let display_name = model_id
        .split('/')
        .last()
        .unwrap_or(&model_id)
        .to_string();

    {
        let mut engine_guard = state.engine.lock().await;
        let engine = engine_guard.as_mut().ok_or("App not initialized")?;
        engine.set_model(new_model, display_name.clone());
    }

    *state.model_id.lock().await = full_model_id;
    *state.model_name.lock().await = display_name.clone();

    Ok(display_name)
}

/// List available models from all providers
#[tauri::command]
pub async fn list_models(_state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    let mut all_models = Vec::new();

    for (provider_name, result) in list_all_models().await {
        if let Ok(models) = result {
            for m in models {
                all_models.push(ModelInfo {
                    id: m.definition.id.clone(),
                    display_name: m.definition.name().to_string(),
                    provider: provider_name.clone(),
                });
            }
        }
    }

    Ok(all_models)
}

/// List all conversations
#[tauri::command]
pub async fn list_conversations(state: State<'_, AppState>) -> Result<Vec<ConversationInfo>, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;

    store
        .list_conversations()
        .map(|convos| convos.into_iter().map(ConversationInfo::from).collect())
        .map_err(|e| format!("Failed to list conversations: {}", e))
}

/// Switch to a different conversation
#[tauri::command]
pub async fn switch_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Vec<DisplayMessage>, String> {
    use noema_core::SessionStore;

    let session = {
        let store_guard = state.store.lock().await;
        let store = store_guard.as_ref().ok_or("App not initialized")?;
        store
            .open_conversation(&conversation_id)
            .map_err(|e| format!("Failed to open conversation: {}", e))?
    };

    let model_id_str = state.model_id.lock().await.clone();
    let model_name = state.model_name.lock().await.clone();
    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

    // Create model
    let model = create_model(&model_id_str)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    // Get messages before creating new engine
    let messages: Vec<DisplayMessage> = session
        .messages()
        .iter()
        .map(DisplayMessage::from_chat_message)
        .collect();

    let engine = ChatEngine::new(session, model, model_name, mcp_registry);

    *state.engine.lock().await = Some(engine);
    *state.current_conversation_id.lock().await = conversation_id;

    Ok(messages)
}

/// Create a new conversation
#[tauri::command]
pub async fn new_conversation(state: State<'_, AppState>) -> Result<String, String> {
    let session = {
        let store_guard = state.store.lock().await;
        let store = store_guard.as_ref().ok_or("App not initialized")?;
        store
            .create_conversation()
            .map_err(|e| format!("Failed to create conversation: {}", e))?
    };

    let conversation_id = session.conversation_id().to_string();
    let model_id_str = state.model_id.lock().await.clone();
    let model_name = state.model_name.lock().await.clone();
    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

    let model = create_model(&model_id_str)
        .map_err(|e| format!("Failed to create model: {}", e))?;

    let engine = ChatEngine::new(session, model, model_name, mcp_registry);

    *state.engine.lock().await = Some(engine);
    *state.current_conversation_id.lock().await = conversation_id.clone();

    Ok(conversation_id)
}

/// Delete a conversation
#[tauri::command]
pub async fn delete_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<(), String> {
    let current_id = state.current_conversation_id.lock().await.clone();
    if conversation_id == current_id {
        return Err("Cannot delete current conversation".to_string());
    }

    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;

    store
        .delete_conversation(&conversation_id)
        .map_err(|e| format!("Failed to delete conversation: {}", e))
}

/// Rename a conversation
#[tauri::command]
pub async fn rename_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
    name: String,
) -> Result<(), String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("App not initialized")?;

    let name_opt = if name.trim().is_empty() {
        None
    } else {
        Some(name.as_str())
    };

    store
        .rename_conversation(&conversation_id, name_opt)
        .map_err(|e| format!("Failed to rename conversation: {}", e))
}

/// Get current model name
#[tauri::command]
pub async fn get_model_name(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.model_name.lock().await.clone())
}

/// Get current conversation ID
#[tauri::command]
pub async fn get_current_conversation_id(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.current_conversation_id.lock().await.clone())
}
