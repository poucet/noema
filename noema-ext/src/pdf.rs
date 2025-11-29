//! PDF processing utilities for text and image extraction

use base64::Engine;
use flate2::read::ZlibDecoder;
use image::{ImageBuffer, Luma, Rgb, Rgba};
use lopdf::Object;
use std::io::Read;

/// Extracted content from a PDF
pub struct ExtractedPdf {
    pub text: Option<String>,
    pub images: Vec<ExtractedImage>,
}

/// An extracted image with its data and mime type
pub struct ExtractedImage {
    pub data: String, // base64 encoded
    pub mime_type: String,
}

/// Process PDF bytes and extract text and images
pub fn process_pdf(pdf_bytes: &[u8]) -> Result<ExtractedPdf, String> {
    let mut result = ExtractedPdf {
        text: None,
        images: Vec::new(),
    };

    // Extract text from PDF
    match pdf_extract::extract_text_from_mem(pdf_bytes) {
        Ok(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                result.text = Some(trimmed.to_string());
            }
        }
        Err(e) => {
            eprintln!("PDF text extraction failed: {}", e);
        }
    }

    // Extract embedded images by scanning all document objects
    if let Ok(doc) = lopdf::Document::load_mem(pdf_bytes) {
        for (_obj_id, obj) in doc.objects.iter() {
            if let lopdf::Object::Stream(stream) = obj {
                if let Some(image) = extract_image_from_stream(stream) {
                    result.images.push(image);
                }
            }
        }
    }

    // Extract diagrams (Form XObjects) using pdfium
    if let Ok(diagram_images) = extract_form_xobjects(pdf_bytes) {
        result.images.extend(diagram_images);
    }

    if result.text.is_none() && result.images.is_empty() {
        return Err("Could not extract any content from PDF".to_string());
    }

    Ok(result)
}

/// Extract an image from a lopdf Stream object
fn extract_image_from_stream(stream: &lopdf::Stream) -> Option<ExtractedImage> {
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
            return Some(ExtractedImage {
                data: base64_data,
                mime_type: "image/jpeg".to_string(),
            });
        }
    }

    // Check for FlateDecode (compressed raw bitmap) - decompress and convert to PNG
    if filters.iter().any(|f| f == "FlateDecode") {
        return extract_flate_image(image_data, width, height, bits, color_space.as_deref());
    }

    // Try raw uncompressed image data
    if filters.is_empty() {
        return convert_raw_to_png(image_data, width, height, bits, color_space.as_deref());
    }

    None
}

/// Extract a FlateDecode compressed image
fn extract_flate_image(
    data: &[u8],
    width: u32,
    height: u32,
    bits: u8,
    color_space: Option<&str>,
) -> Option<ExtractedImage> {
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
) -> Option<ExtractedImage> {
    // Determine color type from color space
    let is_rgb = color_space.map_or(false, |cs| cs.contains("RGB"));
    let is_gray = color_space.map_or(false, |cs| cs.contains("Gray"));

    let png_data = if is_rgb && bits == 8 {
        let expected_size = (width * height * 3) as usize;
        if data.len() < expected_size {
            return None;
        }
        let img: ImageBuffer<Rgb<u8>, _> =
            ImageBuffer::from_raw(width, height, data[..expected_size].to_vec())?;
        let mut png_bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png_bytes, image::ImageFormat::Png).ok()?;
        png_bytes.into_inner()
    } else if is_gray && bits == 8 {
        let expected_size = (width * height) as usize;
        if data.len() < expected_size {
            return None;
        }
        let img: ImageBuffer<Luma<u8>, _> =
            ImageBuffer::from_raw(width, height, data[..expected_size].to_vec())?;
        let mut png_bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png_bytes, image::ImageFormat::Png).ok()?;
        png_bytes.into_inner()
    } else {
        // Default: try different formats based on data size
        let rgb_size = (width * height * 3) as usize;
        let rgba_size = (width * height * 4) as usize;

        if data.len() >= rgba_size {
            let img: ImageBuffer<Rgba<u8>, _> =
                ImageBuffer::from_raw(width, height, data[..rgba_size].to_vec())?;
            let mut png_bytes = std::io::Cursor::new(Vec::new());
            img.write_to(&mut png_bytes, image::ImageFormat::Png).ok()?;
            png_bytes.into_inner()
        } else if data.len() >= rgb_size {
            let img: ImageBuffer<Rgb<u8>, _> =
                ImageBuffer::from_raw(width, height, data[..rgb_size].to_vec())?;
            let mut png_bytes = std::io::Cursor::new(Vec::new());
            img.write_to(&mut png_bytes, image::ImageFormat::Png).ok()?;
            png_bytes.into_inner()
        } else {
            return None;
        }
    };

    let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_data);
    Some(ExtractedImage {
        data: base64_data,
        mime_type: "image/png".to_string(),
    })
}

/// Extract Form XObjects (diagrams, charts) as rendered images using pdfium
fn extract_form_xobjects(pdf_bytes: &[u8]) -> Result<Vec<ExtractedImage>, String> {
    use pdfium_render::prelude::*;

    let mut images = Vec::new();

    // Try to bind to pdfium library
    let pdfium = Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
            .or_else(|_| Pdfium::bind_to_system_library())
            .map_err(|e| format!("Failed to load pdfium: {}", e))?,
    );

    let document = pdfium
        .load_pdf_from_byte_slice(pdf_bytes, None)
        .map_err(|e| format!("Failed to load PDF with pdfium: {}", e))?;

    // Iterate through pages and find Form XObjects
    for page in document.pages().iter() {
        for object in page.objects().iter() {
            // Check if this is a Form XObject (grouped vector graphics - likely a diagram)
            if object.object_type() == PdfPageObjectType::XObjectForm {
                // Get the bounds of this form object
                if let Ok(bounds) = object.bounds() {
                    let width = bounds.width().value as i32;
                    let height = bounds.height().value as i32;

                    // Skip very small objects (likely not diagrams)
                    if width < 50 || height < 50 {
                        continue;
                    }

                    // Render the page portion containing this object
                    let render_config = PdfRenderConfig::new()
                        .set_target_width(width * 2)
                        .set_target_height(height * 2);

                    if let Ok(bitmap) = page.render_with_config(&render_config) {
                        let image = bitmap.as_image();
                        let mut png_bytes = std::io::Cursor::new(Vec::new());
                        if image
                            .write_to(&mut png_bytes, image::ImageFormat::Png)
                            .is_ok()
                        {
                            let base64_data = base64::engine::general_purpose::STANDARD
                                .encode(png_bytes.into_inner());
                            images.push(ExtractedImage {
                                data: base64_data,
                                mime_type: "image/png".to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(images)
}
