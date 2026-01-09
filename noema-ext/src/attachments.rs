use crate::pdf::process_pdf;
use base64::Engine;
use llm::ContentBlock;

#[derive(Debug, Clone)]
pub struct Attachment {
    pub mime_type: String,
    pub data: String, // base64 encoded data
}

pub fn process_attachment(attachment: &Attachment) -> Result<Vec<ContentBlock>, String> {
    let mut blocks = Vec::new();
    let mime_lower = attachment.mime_type.to_lowercase();

    if mime_lower.starts_with("image/") {
        blocks.push(ContentBlock::Image {
            data: attachment.data.clone(),
            mime_type: attachment.mime_type.clone(),
        });
    } else if mime_lower.starts_with("audio/") {
        blocks.push(ContentBlock::Audio {
            data: attachment.data.clone(),
            mime_type: attachment.mime_type.clone(),
        });
    } else if mime_lower.starts_with("text/") {
        // Text/markdown files: decode and add as text content
        match base64::engine::general_purpose::STANDARD.decode(&attachment.data) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => {
                    blocks.push(ContentBlock::Text { text });
                }
                Err(e) => {
                    return Err(format!("Failed to decode text file as UTF-8: {}", e));
                }
            },
            Err(e) => {
                return Err(format!("Failed to decode base64: {}", e));
            }
        }
    } else if mime_lower == "application/pdf" {
        // PDF files: extract text and images
        match process_pdf_attachment(&attachment.data) {
            Ok(pdf_blocks) => {
                blocks.extend(pdf_blocks);
            }
            Err(e) => {
                return Err(format!("Failed to process PDF: {}", e));
            }
        }
    }

    if blocks.is_empty() {
        // Maybe warning or error? For now let's assume unsupported types return empty or error
        // If empty, caller might decide.
    }

    Ok(blocks)
}

fn process_pdf_attachment(base64_data: &str) -> Result<Vec<ContentBlock>, String> {
    // Decode base64 to bytes
    let pdf_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| format!("Failed to decode PDF base64: {}", e))?;

    let extracted = process_pdf(&pdf_bytes).map_err(|e| e.to_string())?;

    let mut blocks = Vec::new();

    // Add text content
    if let Some(text) = extracted.text {
        blocks.push(ContentBlock::Text {
            text: format!("[PDF Content]\n{}", text),
        });
    }

    // Add images
    for image in extracted.images {
        blocks.push(ContentBlock::Image {
            data: image.data,
            mime_type: image.mime_type,
        });
    }

    if blocks.is_empty() {
        return Err("Could not extract any content from PDF".to_string());
    }

    Ok(blocks)
}
