//! Utilities for LLM aimaxxing_providers

use crate::{Error, Result};
use bytes::{BufMut, BytesMut};

/// A buffer for accumulating SSE (Server-Sent Events) bytes.
///
/// This buffer is resilient to UTF-8 characters being split across network chunks.
/// It accumulates bytes and only returns complete UTF-8 strings.
#[derive(Debug)]
pub struct SseBuffer {
    buffer: BytesMut,
    max_capacity: usize,
}

impl Default for SseBuffer {
    fn default() -> Self {
        Self {
            buffer: BytesMut::new(),
            max_capacity: 10 * 1024 * 1024, // Default 10MB
        }
    }
}

impl SseBuffer {
    /// Create a new empty SSE buffer
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom capacity limit
    pub fn with_capacity_limit(max_capacity: usize) -> Self {
        Self {
            buffer: BytesMut::new(),
            max_capacity,
        }
    }

    /// Add bytes to the buffer
    pub fn extend_from_slice(&mut self, bytes: &[u8]) -> Result<()> {
        if self.buffer.len() + bytes.len() > self.max_capacity {
            return Err(Error::StreamInterrupted(format!(
                "SSE buffer exceeded max capacity of {} bytes",
                self.max_capacity
            )));
        }
        self.buffer.put_slice(bytes);
        Ok(())
    }

    /// Extract all complete UTF-8 SSE messages from the buffer.
    ///
    /// Returns a list of strings, each representing one or more lines of SSE data.
    /// Any incomplete UTF-8 sequence or incomplete SSE message (missing \n\n)
    /// will remain in the buffer for the next call.
    pub fn extract_messages(&mut self) -> Result<Vec<String>> {
        let mut messages = Vec::new();

        while let Some(pos) = self.find_sse_delimiter() {
            // Found a complete \n\n message
            let end_pos = pos + 2;
            let chunk = self.buffer.split_to(end_pos);

            // Try to convert to UTF-8
            match String::from_utf8(chunk.to_vec()) {
                Ok(s) => messages.push(s),
                Err(e) => {
                    // This should ideally not happen if we only split at \n\n
                    // unless the delimiter itself is part of a multi-byte char (impossible for ASCII \n)
                    return Err(Error::StreamInterrupted(format!(
                        "Invalid UTF-8 in SSE stream: {}",
                        e
                    )));
                }
            }
        }

        Ok(messages)
    }

    /// Check if the buffer ends with an incomplete UTF-8 sequence.
    /// This is a simplified check - in practice, we rely on the fact that
    /// SSE messages are delimited by \n\n (ASCII), which cannot be part of a
    /// multi-byte UTF-8 character's trailing bytes.
    fn find_sse_delimiter(&self) -> Option<usize> {
        let bytes = self.buffer.as_ref();
        (0..bytes.len().saturating_sub(1)).find(|&i| bytes[i] == b'\n' && bytes[i + 1] == b'\n')
    }

    /// Convert the entire buffer to a string, handling potentially split UTF-8 at the very end.
    pub fn push_and_get_text(&mut self, bytes: &[u8]) -> Result<String> {
        self.extend_from_slice(bytes)?;

        let bytes = self.buffer.as_ref();
        match std::str::from_utf8(bytes) {
            Ok(s) => {
                let text = s.to_string();
                self.buffer.clear();
                Ok(text)
            }
            Err(e) => {
                let valid_len = e.valid_up_to();
                let valid_bytes = self.buffer.split_to(valid_len);
                // The remaining invalid (incomplete) bytes stay in the buffer
                Ok(String::from_utf8_lossy(&valid_bytes).to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_buffer_split_utf8() {
        let mut buffer = SseBuffer::new();

        // "心" in UTF-8 is [0xE5, 0xBF, 0x83]
        let part1 = [0xE5, 0xBF];
        let part2 = [0x83];

        let text1 = buffer.push_and_get_text(&part1).unwrap();
        assert_eq!(text1, "");
        assert_eq!(buffer.buffer.len(), 2);

        let text2 = buffer.push_and_get_text(&part2).unwrap();
        assert_eq!(text2, "心");
        assert_eq!(buffer.buffer.len(), 0);
    }

    #[test]
    fn test_sse_buffer_overflow() {
        let mut buffer = SseBuffer::with_capacity_limit(10);
        let data = vec![0u8; 11];
        let res = buffer.extend_from_slice(&data);
        assert!(res.is_err());
    }
}
