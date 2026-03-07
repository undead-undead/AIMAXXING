//! Tool call history and similarity detection for loop prevention.

use std::collections::{HashMap, HashSet};
use tracing::debug;

/// Represents a record of a tool call
#[derive(Debug, Clone)]
pub struct CallRecord {
    pub tool_name: String,
    pub input: String,
}

/// Tracks history of tool calls to detect repeating patterns
#[derive(Debug, Default, Clone)]
pub struct QueryHistory {
    records: Vec<CallRecord>,
    counts: HashMap<String, usize>,
}

impl QueryHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a call to the history
    pub fn record(&mut self, tool_name: String, input: String) {
        let count = self.counts.entry(tool_name.clone()).or_insert(0);
        *count += 1;
        self.records.push(CallRecord { tool_name, input });
    }

    /// Calculate Jaccard similarity between two strings
    /// Based on word overlap
    pub fn calculate_similarity(s1: &str, s2: &str) -> f64 {
        let tokens1: HashSet<_> = s1
            .split_whitespace()
            .map(|s| s.to_lowercase().replace(|c: char| !c.is_alphanumeric(), ""))
            .filter(|s| s.len() > 2)
            .collect();

        let tokens2: HashSet<_> = s2
            .split_whitespace()
            .map(|s| s.to_lowercase().replace(|c: char| !c.is_alphanumeric(), ""))
            .filter(|s| s.len() > 2)
            .collect();

        if tokens1.is_empty() && tokens2.is_empty() {
            return 1.0;
        }

        let intersection: HashSet<_> = tokens1.intersection(&tokens2).collect();
        let union: HashSet<_> = tokens1.union(&tokens2).collect();

        intersection.len() as f64 / union.len() as f64
    }

    /// Check if a tool call is too similar to previous calls of the same tool
    /// Returns a message suggesting an alternative if similarity is high.
    pub fn check_loop(&self, tool_name: &str, input: &str, threshold: f64) -> Option<String> {
        for record in self.records.iter().rev() {
            if record.tool_name == tool_name {
                let similarity = Self::calculate_similarity(&record.input, input);
                if similarity >= threshold {
                    debug!(tool = %tool_name, similarity = %similarity, "Detected potential loop call");
                    return Some(format!(
                        "WARNING: This call to '{}' is {:.0}% similar to a previous call. \
                        If the previous call did not yield the results you wanted, try a different approach, \
                        different keywords, or a different tool instead of repeating the same action.",
                        tool_name, similarity * 100.0
                    ));
                }
            }
        }
        None
    }

    pub fn get_count(&self, tool_name: &str) -> usize {
        *self.counts.get(tool_name).unwrap_or(&0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_similarity() {
        let s1 = "Search for Apple stock price in 2024";
        let s2 = "Search for Apple stock price 2024";
        let sim = QueryHistory::calculate_similarity(s1, s2);
        assert!(sim > 0.8);

        let s3 = "Get latest news about Tesla";
        let sim2 = QueryHistory::calculate_similarity(s1, s3);
        assert!(sim2 < 0.2);
    }

    #[test]
    fn test_loop_detection() {
        let mut history = QueryHistory::new();
        history.record("search".to_string(), "Apple stock".to_string());

        let result = history.check_loop("search", "Apple stock price", 0.6);
        assert!(result.is_some());

        let result2 = history.check_loop("search", "Tesla news", 0.6);
        assert!(result2.is_none());
    }
}
