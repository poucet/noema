//! Filesystem helpers for vault-backed Markdown.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::storage::helper::content_hash;
use crate::storage::vault::markdown::{
    serialize_markdown, split_markdown, Frontmatter, SystemFrontmatter,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrittenMarkdownFile {
    pub relative_path: String,
    pub mtime: Option<i64>,
    pub content_hash: String,
    pub frontmatter_hash: Option<String>,
}

pub fn sanitize_path_segment(input: &str) -> String {
    let mut out = String::new();
    let mut last_was_space = false;

    for ch in input.trim().chars() {
        let safe = match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => None,
            c if c.is_control() => None,
            c => Some(c),
        };

        match safe {
            Some(c) if c.is_whitespace() => {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            Some(c) => {
                out.push(c);
                last_was_space = false;
            }
            None => {}
        }
    }

    let out = out.trim().trim_matches('.').to_string();
    if out.is_empty() {
        "Untitled".to_string()
    } else {
        out
    }
}

pub fn numbered_path_segment(position: Option<i64>, title: &str) -> String {
    let title = sanitize_path_segment(title);
    match position {
        Some(position) if position >= 0 => format!("{:02} {title}", position + 1),
        _ => title,
    }
}

pub fn normalize_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn read_markdown_text(root: &Path, relative_path: &str) -> Result<Option<String>> {
    let path = root.join(relative_path);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read vault file {}", path.display()))?;
    Ok(Some(text))
}

pub fn read_markdown_body(root: &Path, relative_path: &str) -> Result<Option<String>> {
    let Some(text) = read_markdown_text(root, relative_path)? else {
        return Ok(None);
    };
    Ok(Some(split_markdown(&text).body.to_string()))
}

pub fn write_plain_markdown(
    root: &Path,
    relative_path: &str,
    body: &str,
) -> Result<WrittenMarkdownFile> {
    write_markdown_text(root, relative_path, body)
}

pub fn write_markdown_preserving_frontmatter(
    root: &Path,
    relative_path: &str,
    body: &str,
) -> Result<WrittenMarkdownFile> {
    let path = root.join(relative_path);
    let rendered = if path.exists() {
        let existing = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read vault file {}", path.display()))?;
        let split = split_markdown(&existing);
        match split.raw_frontmatter {
            Some(raw) => format!("---\n{raw}\n---\n{body}"),
            None => body.to_string(),
        }
    } else {
        body.to_string()
    };
    write_markdown_text(root, relative_path, &rendered)
}

pub fn write_frontmatter_markdown(
    root: &Path,
    relative_path: &str,
    frontmatter: &Frontmatter,
    system: &SystemFrontmatter,
    body: &str,
) -> Result<WrittenMarkdownFile> {
    let rendered = serialize_markdown(frontmatter, system, body)?;
    write_markdown_text(root, relative_path, &rendered)
}

pub fn write_markdown_text(
    root: &Path,
    relative_path: &str,
    text: &str,
) -> Result<WrittenMarkdownFile> {
    let path = root.join(relative_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create vault directory {}", parent.display()))?;
    }

    let temp_path = temp_path_for(&path);
    std::fs::write(&temp_path, text)
        .with_context(|| format!("Failed to write vault temp file {}", temp_path.display()))?;
    std::fs::rename(&temp_path, &path).with_context(|| {
        format!(
            "Failed to replace vault file {} with {}",
            path.display(),
            temp_path.display()
        )
    })?;

    let split = split_markdown(text);
    let metadata = std::fs::metadata(&path)
        .with_context(|| format!("Failed to stat vault file {}", path.display()))?;
    Ok(WrittenMarkdownFile {
        relative_path: relative_path.to_string(),
        mtime: metadata.modified().ok().and_then(modified_millis),
        content_hash: content_hash(split.body),
        frontmatter_hash: split.raw_frontmatter.map(content_hash),
    })
}

fn temp_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("vault-file");
    path.with_file_name(format!(".{file_name}.tmp"))
}

fn modified_millis(time: std::time::SystemTime) -> Option<i64> {
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_path_segments() {
        assert_eq!(sanitize_path_segment(" A/B:C  "), "ABC");
        assert_eq!(sanitize_path_segment("..."), "Untitled");
    }

    #[test]
    fn formats_ordered_segments() {
        assert_eq!(numbered_path_segment(Some(0), "Intro"), "01 Intro");
        assert_eq!(numbered_path_segment(None, "Intro"), "Intro");
    }
}
