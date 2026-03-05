//! Phase 14: Memory Decay — biological memory dynamics.
//!
//! Implements multi-dimensional scoring for memory entries:
//! - Recency: time-based decay (exponential)
//! - Importance: subjective weight (core preferences persist)
//! - Access frequency: reinforcement through repeated retrieval
//!
//! Entries below a threshold are candidates for archival or pruning.

/// A scored memory entry with decay metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScoredEntry {
    /// Unique identifier
    pub id: String,
    /// Content preview
    pub content_preview: String,
    /// When the entry was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the entry was last accessed
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    /// Number of times this entry has been retrieved
    pub access_count: u32,
    /// Subjective importance (0.0 = trivial, 1.0 = core identity)
    pub importance: f64,
    /// Computed composite score (higher = more relevant)
    pub composite_score: f64,
}

/// Configuration for the decay algorithm
#[derive(Debug, Clone)]
pub struct DecayConfig {
    /// Half-life in hours for recency decay (default: 168h = 1 week)
    pub half_life_hours: f64,
    /// Weight of recency in composite score (0.0 to 1.0)
    pub recency_weight: f64,
    /// Weight of importance in composite score
    pub importance_weight: f64,
    /// Weight of access frequency in composite score
    pub frequency_weight: f64,
    /// Entries below this threshold are candidates for pruning
    pub prune_threshold: f64,
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            half_life_hours: 168.0, // 1 week
            recency_weight: 0.4,
            importance_weight: 0.4,
            frequency_weight: 0.2,
            prune_threshold: 0.1,
        }
    }
}

/// Memory decay engine that scores and prunes entries.
pub struct MemoryDecay {
    config: DecayConfig,
}

impl Default for MemoryDecay {
    fn default() -> Self {
        Self::new(DecayConfig::default())
    }
}

impl MemoryDecay {
    pub fn new(config: DecayConfig) -> Self {
        Self { config }
    }

    /// Compute the composite score for a single entry
    pub fn score(&self, entry: &mut ScoredEntry) {
        let now = chrono::Utc::now();

        // Recency: exponential decay based on hours since last access
        let hours_since_access = (now - entry.last_accessed).num_minutes() as f64 / 60.0;
        let recency = (-0.693 * hours_since_access / self.config.half_life_hours).exp();

        // Frequency: logarithmic scaling of access count
        let frequency = (1.0 + entry.access_count as f64).ln() / 5.0_f64.ln();
        let frequency = frequency.min(1.0); // Cap at 1.0

        // Composite score
        entry.composite_score = self.config.recency_weight * recency
            + self.config.importance_weight * entry.importance
            + self.config.frequency_weight * frequency;
    }

    /// Score all entries and return those below the prune threshold
    pub fn sweep(&self, entries: &mut [ScoredEntry]) -> Vec<String> {
        let mut prune_candidates = Vec::new();

        for entry in entries.iter_mut() {
            self.score(entry);
            if entry.composite_score < self.config.prune_threshold {
                prune_candidates.push(entry.id.clone());
            }
        }

        prune_candidates
    }

    /// Simulate a "touch" — update last_accessed and increment access_count
    pub fn touch(entry: &mut ScoredEntry) {
        entry.last_accessed = chrono::Utc::now();
        entry.access_count += 1;
    }
}

/// Report from a decay sweep
#[derive(Debug, Clone, serde::Serialize)]
pub struct DecaySweepReport {
    pub total_entries: usize,
    pub prune_candidates: usize,
    pub avg_score: f64,
    pub min_score: f64,
    pub max_score: f64,
}

impl MemoryDecay {
    /// Run a full sweep and generate a report
    pub fn sweep_report(&self, entries: &mut [ScoredEntry]) -> DecaySweepReport {
        let candidates = self.sweep(entries);

        let scores: Vec<f64> = entries.iter().map(|e| e.composite_score).collect();
        let avg = if scores.is_empty() {
            0.0
        } else {
            scores.iter().sum::<f64>() / scores.len() as f64
        };
        let min = scores.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        DecaySweepReport {
            total_entries: entries.len(),
            prune_candidates: candidates.len(),
            avg_score: avg,
            min_score: if min.is_infinite() { 0.0 } else { min },
            max_score: if max.is_infinite() { 0.0 } else { max },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: &str, hours_ago: i64, importance: f64, access_count: u32) -> ScoredEntry {
        let now = chrono::Utc::now();
        ScoredEntry {
            id: id.to_string(),
            content_preview: format!("Entry {}", id),
            created_at: now - chrono::Duration::hours(hours_ago * 2),
            last_accessed: now - chrono::Duration::hours(hours_ago),
            access_count,
            importance,
            composite_score: 0.0,
        }
    }

    #[test]
    fn test_recent_high_importance_scores_high() {
        let decay = MemoryDecay::default();
        let mut entry = make_entry("core", 1, 0.9, 10);
        decay.score(&mut entry);
        assert!(
            entry.composite_score > 0.7,
            "High importance + recent should score high: {}",
            entry.composite_score
        );
    }

    #[test]
    fn test_old_low_importance_scores_low() {
        let decay = MemoryDecay::default();
        let mut entry = make_entry("old", 720, 0.05, 1); // 30 days old, low importance
        decay.score(&mut entry);
        assert!(
            entry.composite_score < 0.2,
            "Old + low importance should score low: {}",
            entry.composite_score
        );
    }

    #[test]
    fn test_sweep_identifies_prune_candidates() {
        let decay = MemoryDecay::default();
        let mut entries = vec![
            make_entry("fresh", 1, 0.8, 5),
            make_entry("stale", 2000, 0.01, 0), // Very old, trivial
        ];
        let candidates = decay.sweep(&mut entries);
        assert!(candidates.contains(&"stale".to_string()));
        assert!(!candidates.contains(&"fresh".to_string()));
    }

    #[test]
    fn test_touch_updates_access() {
        let mut entry = make_entry("e1", 24, 0.5, 3);
        let old_count = entry.access_count;
        MemoryDecay::touch(&mut entry);
        assert_eq!(entry.access_count, old_count + 1);
    }
}
