//! Text chunking for embedding.

use anyhow::Result;
use async_trait::async_trait;

/// Stable replacement for str::floor_char_boundary (unstable).
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Stable replacement for str::ceil_char_boundary (unstable).
fn ceil_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// A chunk of text produced by a `Chunker`.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub text: String,
    pub index: u32,
}

/// Trait for splitting text into chunks for embedding.
///
/// Async to allow future implementations that use LLM-assisted
/// boundary detection or semantic chunking.
#[async_trait]
pub trait Chunker: Send + Sync {
    /// Split text into chunks for embedding.
    async fn chunk(&self, text: &str) -> Result<Vec<Chunk>>;
}

/// Default chunker: recursive character splitting.
///
/// Tries to split on paragraph boundaries first, then line breaks,
/// then spaces, then character boundaries. Preserves overlap between
/// chunks for context continuity.
pub struct RecursiveCharacterChunker {
    /// Maximum chunk size in characters.
    pub chunk_size: usize,
    /// Overlap between consecutive chunks in characters.
    pub chunk_overlap: usize,
}

impl RecursiveCharacterChunker {
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
        Self {
            chunk_size,
            chunk_overlap,
        }
    }

    /// Split text using the given separators, trying each in order.
    fn split_recursive(&self, text: &str, separators: &[&str]) -> Vec<String> {
        if text.len() <= self.chunk_size {
            return vec![text.to_string()];
        }

        let sep = separators
            .iter()
            .find(|s| text.contains(**s))
            .copied()
            .unwrap_or("");

        let parts: Vec<&str> = if sep.is_empty() {
            // Character-level split as last resort
            let mut result = Vec::new();
            let mut start = 0;
            while start < text.len() {
                let end = (start + self.chunk_size).min(text.len());
                // Don't split in the middle of a UTF-8 character
                let end = floor_char_boundary(text, end);
                result.push(&text[start..end]);
                start = end;
            }
            result
        } else {
            text.split(sep).collect()
        };

        let remaining_separators = if separators.len() > 1 {
            &separators[1..]
        } else {
            &[]
        };

        let mut chunks = Vec::new();
        let mut current = String::new();

        for part in parts {
            let candidate = if current.is_empty() {
                part.to_string()
            } else {
                format!("{}{}{}", current, sep, part)
            };

            if candidate.len() > self.chunk_size && !current.is_empty() {
                // Current chunk is full — recurse if it's still too big
                if current.len() > self.chunk_size && !remaining_separators.is_empty() {
                    chunks.extend(self.split_recursive(&current, remaining_separators));
                } else {
                    chunks.push(current);
                }
                current = part.to_string();
            } else {
                current = candidate;
            }
        }

        if !current.is_empty() {
            if current.len() > self.chunk_size && !remaining_separators.is_empty() {
                chunks.extend(self.split_recursive(&current, remaining_separators));
            } else {
                chunks.push(current);
            }
        }

        chunks
    }

    /// Apply overlap between chunks.
    fn apply_overlap(&self, chunks: Vec<String>) -> Vec<String> {
        if self.chunk_overlap == 0 || chunks.len() <= 1 {
            return chunks;
        }

        let mut result = Vec::with_capacity(chunks.len());
        for (i, chunk) in chunks.iter().enumerate() {
            if i == 0 {
                result.push(chunk.clone());
            } else {
                // Prepend the tail of the previous chunk as overlap
                let prev = &chunks[i - 1];
                let overlap_start = prev.len().saturating_sub(self.chunk_overlap);
                let overlap_start = ceil_char_boundary(prev, overlap_start);
                let overlap = &prev[overlap_start..];
                if overlap.is_empty() {
                    result.push(chunk.clone());
                } else {
                    result.push(format!("{}{}", overlap, chunk));
                }
            }
        }
        result
    }
}

impl Default for RecursiveCharacterChunker {
    fn default() -> Self {
        // ~4096 tokens ≈ 16k chars, 128 tokens ≈ 512 chars
        Self::new(16000, 512)
    }
}

#[async_trait]
impl Chunker for RecursiveCharacterChunker {
    async fn chunk(&self, text: &str) -> Result<Vec<Chunk>> {
        if text.is_empty() {
            return Ok(vec![]);
        }

        let separators = &["\n\n", "\n", " ", ""];
        let raw_chunks = self.split_recursive(text, separators);
        let chunks = self.apply_overlap(raw_chunks);

        Ok(chunks
            .into_iter()
            .enumerate()
            .map(|(i, text)| Chunk {
                text,
                index: i as u32,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn short_text_is_single_chunk() {
        let chunker = RecursiveCharacterChunker::new(1000, 0);
        let chunks = chunker.chunk("Hello, world!").await.unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "Hello, world!");
        assert_eq!(chunks[0].index, 0);
    }

    #[tokio::test]
    async fn empty_text_returns_empty() {
        let chunker = RecursiveCharacterChunker::new(1000, 0);
        let chunks = chunker.chunk("").await.unwrap();
        assert!(chunks.is_empty());
    }

    #[tokio::test]
    async fn splits_on_paragraph_boundary() {
        let chunker = RecursiveCharacterChunker::new(20, 0);
        let text = "First paragraph.\n\nSecond paragraph.";
        let chunks = chunker.chunk(text).await.unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].text, "First paragraph.");
        assert_eq!(chunks[1].text, "Second paragraph.");
    }

    #[tokio::test]
    async fn chunks_are_indexed() {
        let chunker = RecursiveCharacterChunker::new(10, 0);
        let text = "aaa\n\nbbb\n\nccc";
        let chunks = chunker.chunk(text).await.unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[1].index, 1);
    }
}
