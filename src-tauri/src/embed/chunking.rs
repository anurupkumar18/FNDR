//! Text chunking for embedding

/// Maximum tokens per chunk (MiniLM limit is 512, use 450 for safety)
const MAX_CHUNK_TOKENS: usize = 450;
/// Overlap between chunks in tokens
const CHUNK_OVERLAP: usize = 50;
/// Approximate chars per token for English
const CHARS_PER_TOKEN: usize = 4;

/// Text chunker for splitting long texts
pub struct TextChunker {
    max_chars: usize,
    overlap_chars: usize,
}

impl TextChunker {
    pub fn new() -> Self {
        Self {
            max_chars: MAX_CHUNK_TOKENS * CHARS_PER_TOKEN,
            overlap_chars: CHUNK_OVERLAP * CHARS_PER_TOKEN,
        }
    }

    /// Split text into chunks suitable for embedding
    pub fn chunk(&self, text: &str) -> Vec<String> {
        if text.len() <= self.max_chars {
            return vec![text.to_string()];
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < text.len() {
            let end = (start + self.max_chars).min(text.len());

            // Try to break at word boundary
            let chunk_end = if end < text.len() {
                text[start..end]
                    .rfind(|c: char| c.is_whitespace())
                    .map(|pos| start + pos)
                    .unwrap_or(end)
            } else {
                end
            };

            let chunk = text[start..chunk_end].trim().to_string();
            if !chunk.is_empty() {
                chunks.push(chunk);
            }

            // Move start with overlap
            start = if chunk_end >= self.overlap_chars {
                chunk_end - self.overlap_chars
            } else {
                chunk_end
            };

            // Safety: ensure we're making progress
            if start >= text.len() || chunk_end == text.len() {
                break;
            }
        }

        chunks
    }
}

impl Default for TextChunker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_text_no_chunking() {
        let chunker = TextChunker::new();
        let text = "Hello world";
        let chunks = chunker.chunk(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello world");
    }

    #[test]
    fn test_long_text_chunking() {
        let chunker = TextChunker::new();
        let text = "word ".repeat(500); // >2000 chars
        let chunks = chunker.chunk(&text);
        assert!(chunks.len() > 1);
        // Each chunk should be within limit
        for chunk in &chunks {
            assert!(chunk.len() <= chunker.max_chars + 50); // Allow some flexibility
        }
    }
}
