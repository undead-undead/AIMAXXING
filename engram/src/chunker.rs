//! Text chunking for document indexing
//!
//! Splits documents into overlapping chunks for vector embedding.

use serde::{Deserialize, Serialize};

/// Configuration for the chunker
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub min_chunk_size: usize,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            chunk_size: 512,
            chunk_overlap: 64,
            min_chunk_size: 50,
        }
    }
}

/// A text chunk with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub text: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub sequence: usize,
}

/// Chunking statistics
#[derive(Debug, Clone, Default)]
pub struct ChunkStats {
    pub total_chunks: usize,
    pub total_chars: usize,
    pub avg_chunk_size: usize,
}

/// Text chunker
pub struct Chunker {
    config: ChunkerConfig,
}

impl Chunker {
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }

    /// Split text into overlapping chunks
    pub fn chunk(&self, text: &str) -> Vec<Chunk> {
        if text.len() < self.config.min_chunk_size {
            return vec![Chunk {
                text: text.to_string(),
                start_offset: 0,
                end_offset: text.len(),
                sequence: 0,
            }];
        }

        let mut chunks = Vec::new();
        let mut start = 0;
        let mut seq = 0;
        let chars: Vec<char> = text.chars().collect();
        let total = chars.len();

        while start < total {
            let end = (start + self.config.chunk_size).min(total);

            // Try to break at sentence/paragraph boundary
            let actual_end = if end < total {
                self.find_break_point(&chars, start, end)
            } else {
                end
            };

            let chunk_text: String = chars[start..actual_end].iter().collect();
            if chunk_text.len() >= self.config.min_chunk_size {
                chunks.push(Chunk {
                    text: chunk_text,
                    start_offset: start,
                    end_offset: actual_end,
                    sequence: seq,
                });
                seq += 1;
            }

            if actual_end >= total {
                break;
            }

            start = if actual_end > self.config.chunk_overlap {
                actual_end - self.config.chunk_overlap
            } else {
                actual_end
            };
        }

        chunks
    }

    /// Find a good break point (end of sentence or paragraph)
    fn find_break_point(&self, chars: &[char], start: usize, end: usize) -> usize {
        // Search backward from end for sentence endings
        let search_start = if end > start + 50 { end - 50 } else { start };
        for i in (search_start..end).rev() {
            if chars[i] == '.' || chars[i] == '!' || chars[i] == '?' || chars[i] == '\n' {
                return i + 1;
            }
        }
        end
    }

    /// Get statistics for chunks
    pub fn stats(&self, chunks: &[Chunk]) -> ChunkStats {
        let total_chars: usize = chunks.iter().map(|c| c.text.len()).sum();
        ChunkStats {
            total_chunks: chunks.len(),
            total_chars,
            avg_chunk_size: if chunks.is_empty() {
                0
            } else {
                total_chars / chunks.len()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_chunking() {
        let chunker = Chunker::new(ChunkerConfig {
            chunk_size: 100,
            chunk_overlap: 20,
            min_chunk_size: 10,
        });
        let text = "This is a test. ".repeat(50);
        let chunks = chunker.chunk(&text);
        assert!(!chunks.is_empty());
        assert!(chunks.len() > 1);
    }

    #[test]
    fn test_small_text() {
        let chunker = Chunker::new(ChunkerConfig::default());
        let chunks = chunker.chunk("Hello world");
        assert_eq!(chunks.len(), 1);
    }
}
