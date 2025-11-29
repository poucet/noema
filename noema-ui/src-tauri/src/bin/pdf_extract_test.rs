//! Standalone PDF image extraction test utility
//!
//! Usage: cargo run --bin pdf_extract_test -- <path-to-pdf>

use std::env;
use std::fs;
use lopdf::{Object, ObjectId};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <path-to-pdf>", args[0]);
        std::process::exit(1);
    }

    let pdf_path = &args[1];
    println!("Loading PDF: {}", pdf_path);

    let pdf_bytes = match fs::read(pdf_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Failed to read PDF file: {}", e);
            std::process::exit(1);
        }
    };

    println!("PDF size: {} bytes", pdf_bytes.len());

    // Try text extraction first
    println!("\n=== Text Extraction ===");
    match pdf_extract::extract_text_from_mem(&pdf_bytes) {
        Ok(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                println!("No text extracted");
            } else {
                println!("Extracted {} chars of text", trimmed.len());
                println!("First 500 chars:\n{}", &trimmed[..trimmed.len().min(500)]);
            }
        }
        Err(e) => {
            println!("Text extraction failed: {}", e);
        }
    }

    // Try image extraction
    println!("\n=== Image Extraction ===");
    match lopdf::Document::load_mem(&pdf_bytes) {
        Ok(doc) => {
            println!("PDF loaded successfully");
            println!("Number of pages: {}", doc.get_pages().len());

            // First, scan ALL objects for images
            println!("\n=== Scanning all objects for images ===");
            let mut found_images: Vec<ObjectId> = Vec::new();
            for (obj_id, obj) in doc.objects.iter() {
                if let Object::Stream(stream) = obj {
                    let dict = &stream.dict;
                    // Check if it's an image XObject
                    if let Ok(subtype) = dict.get(b"Subtype") {
                        if let Object::Name(name) = subtype {
                            if name == b"Image" {
                                found_images.push(*obj_id);
                                println!("\nFound Image object: {:?}", obj_id);

                                // Print all dictionary keys
                                println!("  Dictionary keys:");
                                for (key, value) in dict.iter() {
                                    let key_str = String::from_utf8_lossy(key);
                                    println!("    {}: {:?}", key_str, summarize_object(value));
                                }
                                println!("  Stream content length: {} bytes", stream.content.len());
                            }
                        }
                    }
                }
            }
            println!("\nTotal image objects found in document: {}", found_images.len());

            let mut image_count = 0;
            let mut extracted_count = 0;

            for (page_num, page_id) in doc.get_pages() {
                println!("\n--- Page {} (id: {:?}) ---", page_num, page_id);

                match doc.get_page_images(page_id) {
                    Ok(images) => {
                        println!("Found {} images on this page", images.len());

                        for (idx, pdf_image) in images.iter().enumerate() {
                            image_count += 1;
                            println!("\n  Image {}.{}:", page_num, idx + 1);
                            println!("    ID: {:?}", pdf_image.id);
                            println!("    Dimensions: {}x{}", pdf_image.width, pdf_image.height);
                            println!("    Bits per component: {:?}", pdf_image.bits_per_component);
                            println!("    Color space: {:?}", pdf_image.color_space);
                            println!("    Filters: {:?}", pdf_image.filters);
                            println!("    Content size: {} bytes", pdf_image.content.len());

                            // Try to extract
                            if let Some(result) = extract_pdf_image(pdf_image) {
                                extracted_count += 1;
                                println!("    ✓ Extracted as {} ({} bytes base64)",
                                    result.mime_type, result.data.len());

                                // Save to file for inspection
                                let ext = if result.mime_type == "image/jpeg" { "jpg" } else { "png" };
                                let filename = format!("extracted_p{}_{}.{}", page_num, idx + 1, ext);
                                if let Ok(decoded) = base64::Engine::decode(
                                    &base64::engine::general_purpose::STANDARD,
                                    &result.data
                                ) {
                                    if fs::write(&filename, &decoded).is_ok() {
                                        println!("    Saved to: {}", filename);
                                    }
                                }
                            } else {
                                println!("    ✗ Could not extract");
                            }
                        }
                    }
                    Err(e) => {
                        println!("Failed to get images: {}", e);
                    }
                }
            }

            println!("\n=== Summary ===");
            println!("Total images found: {}", image_count);
            println!("Successfully extracted: {}", extracted_count);
        }
        Err(e) => {
            eprintln!("Failed to load PDF: {}", e);
        }
    }
}

struct ExtractedImage {
    data: String,
    mime_type: String,
}

/// Extract an image from a PdfImage
fn extract_pdf_image(pdf_image: &lopdf::xobject::PdfImage) -> Option<ExtractedImage> {
    use base64::Engine;

    let filters = pdf_image.filters.as_ref();
    let image_data = &pdf_image.content;

    println!("    Attempting extraction...");

    // Check for DCTDecode (JPEG)
    if filters.map_or(false, |f| f.iter().any(|s| s == "DCTDecode")) {
        println!("    -> DCTDecode (JPEG) detected");
        if image_data.starts_with(&[0xFF, 0xD8]) {
            let base64_data = base64::engine::general_purpose::STANDARD.encode(image_data);
            return Some(ExtractedImage {
                data: base64_data,
                mime_type: "image/jpeg".to_string(),
            });
        } else {
            println!("    -> Data doesn't start with JPEG magic bytes: {:02X?}",
                &image_data[..image_data.len().min(4)]);
        }
    }

    // Check for FlateDecode (compressed raw bitmap) - convert to PNG
    if filters.map_or(false, |f| f.iter().any(|s| s == "FlateDecode")) {
        println!("    -> FlateDecode detected, attempting decompression...");
        return extract_flate_image(pdf_image);
    }

    // Try raw uncompressed image data
    if filters.map_or(true, |f| f.is_empty()) {
        println!("    -> No filter or empty filter, trying raw extraction...");
        return extract_raw_image(pdf_image);
    }

    println!("    -> Unsupported filter combination");
    None
}

/// Extract a FlateDecode compressed image and convert to PNG
fn extract_flate_image(pdf_image: &lopdf::xobject::PdfImage) -> Option<ExtractedImage> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    let width = pdf_image.width as u32;
    let height = pdf_image.height as u32;
    let bits = pdf_image.bits_per_component.unwrap_or(8) as u8;

    if width == 0 || height == 0 {
        println!("    -> Invalid dimensions: {}x{}", width, height);
        return None;
    }

    // Decompress the data
    let mut decoder = ZlibDecoder::new(&pdf_image.content[..]);
    let mut decompressed = Vec::new();
    match decoder.read_to_end(&mut decompressed) {
        Ok(size) => {
            println!("    -> Decompressed {} bytes to {} bytes", pdf_image.content.len(), size);
        }
        Err(e) => {
            println!("    -> Decompression failed: {}", e);
            return None;
        }
    }

    convert_raw_to_png(&decompressed, width, height, bits, pdf_image.color_space.as_deref())
}

/// Extract raw uncompressed image data and convert to PNG
fn extract_raw_image(pdf_image: &lopdf::xobject::PdfImage) -> Option<ExtractedImage> {
    let width = pdf_image.width as u32;
    let height = pdf_image.height as u32;
    let bits = pdf_image.bits_per_component.unwrap_or(8) as u8;

    if width == 0 || height == 0 {
        println!("    -> Invalid dimensions: {}x{}", width, height);
        return None;
    }

    println!("    -> Trying raw conversion: {}x{}, {} bits", width, height, bits);
    convert_raw_to_png(&pdf_image.content, width, height, bits, pdf_image.color_space.as_deref())
}

/// Convert raw pixel data to PNG
fn convert_raw_to_png(
    data: &[u8],
    width: u32,
    height: u32,
    bits: u8,
    color_space: Option<&str>,
) -> Option<ExtractedImage> {
    use base64::Engine;
    use image::{ImageBuffer, Rgb, Rgba, Luma};

    // Determine color type from color space
    let is_rgb = color_space.map_or(false, |cs| cs.contains("RGB"));
    let is_gray = color_space.map_or(false, |cs| cs.contains("Gray"));

    println!("    -> Color space analysis: is_rgb={}, is_gray={}, bits={}", is_rgb, is_gray, bits);
    println!("    -> Data length: {}", data.len());

    let png_data = if is_rgb && bits == 8 {
        // RGB image
        let expected_size = (width * height * 3) as usize;
        println!("    -> Expecting RGB: {} bytes, have {} bytes", expected_size, data.len());
        if data.len() < expected_size {
            println!("    -> Not enough data for RGB");
            return None;
        }
        let img: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_raw(width, height, data[..expected_size].to_vec())?;
        let mut png_bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png_bytes, image::ImageFormat::Png).ok()?;
        png_bytes.into_inner()
    } else if is_gray && bits == 8 {
        // Grayscale image
        let expected_size = (width * height) as usize;
        println!("    -> Expecting Grayscale: {} bytes, have {} bytes", expected_size, data.len());
        if data.len() < expected_size {
            println!("    -> Not enough data for Grayscale");
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
        let gray_size = (width * height) as usize;

        println!("    -> Trying to guess format: gray={}, rgb={}, rgba={}, actual={}",
            gray_size, rgb_size, rgba_size, data.len());

        if data.len() >= rgba_size {
            println!("    -> Trying RGBA...");
            let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(width, height, data[..rgba_size].to_vec())?;
            let mut png_bytes = std::io::Cursor::new(Vec::new());
            img.write_to(&mut png_bytes, image::ImageFormat::Png).ok()?;
            png_bytes.into_inner()
        } else if data.len() >= rgb_size {
            println!("    -> Trying RGB...");
            let img: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_raw(width, height, data[..rgb_size].to_vec())?;
            let mut png_bytes = std::io::Cursor::new(Vec::new());
            img.write_to(&mut png_bytes, image::ImageFormat::Png).ok()?;
            png_bytes.into_inner()
        } else if data.len() >= gray_size {
            println!("    -> Trying Grayscale...");
            let img: ImageBuffer<Luma<u8>, _> = ImageBuffer::from_raw(width, height, data[..gray_size].to_vec())?;
            let mut png_bytes = std::io::Cursor::new(Vec::new());
            img.write_to(&mut png_bytes, image::ImageFormat::Png).ok()?;
            png_bytes.into_inner()
        } else {
            println!("    -> Data too small for any format");
            return None;
        }
    };

    let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_data);
    Some(ExtractedImage {
        data: base64_data,
        mime_type: "image/png".to_string(),
    })
}

/// Summarize an object for display
fn summarize_object(obj: &Object) -> String {
    match obj {
        Object::Null => "null".to_string(),
        Object::Boolean(b) => format!("bool({})", b),
        Object::Integer(i) => format!("int({})", i),
        Object::Real(r) => format!("real({})", r),
        Object::Name(n) => format!("name({})", String::from_utf8_lossy(n)),
        Object::String(s, _) => format!("string({} bytes)", s.len()),
        Object::Array(arr) => format!("array[{}]", arr.len()),
        Object::Dictionary(d) => format!("dict{{{} keys}}", d.len()),
        Object::Stream(s) => format!("stream({} bytes)", s.content.len()),
        Object::Reference(r) => format!("ref({}, {})", r.0, r.1),
    }
}
