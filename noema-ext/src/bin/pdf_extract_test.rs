//! Standalone PDF image extraction test utility
//!
//! Usage: cargo run --bin pdf_extract_test -- <path-to-pdf>

use noema_ext::pdf::{process_pdf, ExtractedImage};
use std::env;
use std::fs;

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

    match process_pdf(&pdf_bytes) {
        Ok(extracted) => {
            println!("\n=== Extraction Results ===");

            if let Some(text) = &extracted.text {
                println!("\nText extracted: {} chars", text.len());
                println!("First 500 chars:\n{}", &text[..text.len().min(500)]);
            } else {
                println!("\nNo text extracted");
            }

            println!("\nImages extracted: {}", extracted.images.len());
            for (i, image) in extracted.images.iter().enumerate() {
                println!(
                    "  Image {}: {} ({} bytes base64)",
                    i + 1,
                    image.mime_type,
                    image.data.len()
                );

                // Save to file for inspection
                save_image(image, i);
            }
        }
        Err(e) => {
            eprintln!("Failed to extract PDF content: {}", e);
            std::process::exit(1);
        }
    }
}

fn save_image(image: &ExtractedImage, index: usize) {
    use base64::Engine;

    let ext = if image.mime_type == "image/jpeg" {
        "jpg"
    } else {
        "png"
    };
    let filename = format!("extracted_{}.{}", index + 1, ext);

    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&image.data) {
        if fs::write(&filename, &decoded).is_ok() {
            println!("    Saved to: {}", filename);
        }
    }
}
