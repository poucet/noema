//! Extension utilities for Noema - PDF processing, etc.

pub mod attachments;
pub mod pdf;

pub use attachments::{process_attachment, Attachment};
pub use pdf::{process_pdf, ExtractedImage, ExtractedPdf};