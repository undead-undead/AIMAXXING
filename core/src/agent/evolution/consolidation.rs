//! Phase 12-C: Sleep-consolidation mechanism.
//!
//! During low-activity periods, reviews UNVERIFIED memory entries
//! and marks them as VERIFIED, PRUNED, or CONFLICT.

use std::sync::Arc;

use crate::agent::memory::Memory;

use crate::agent::evolution::auditor::{Auditor, AuditResult, ChangeType};

/// Status of a consolidation run
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConsolidationReport {
    pub entries_reviewed: usize,
    pub entries_verified: usize,
    pub entries_pruned: usize,
    pub entries_conflicted: usize,
    pub duration_ms: u64,
}

/// Consolidates memory during sleep/maintenance periods.
///
/// Queries unverified memory entries, evaluates their quality,
/// and marks them accordingly. Uses an independent Auditor (LLM-based)
/// for assessment to ensure memory quality and safety.
pub struct SleepConsolidator {
    memory: Arc<dyn Memory>,
    auditor: Arc<Auditor>,
    /// Max entries to process per consolidation run
    batch_size: usize,
}

impl SleepConsolidator {
    pub fn new(memory: Arc<dyn Memory>, auditor: Arc<Auditor>) -> Self {
        Self {
            memory,
            auditor,
            batch_size: 50,
        }
    }

    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Run a consolidation cycle.
    ///
    /// 1. Fetch unverified entries
    /// 2. Evaluation via Auditor (LLM or Rule-based)
    /// 3. Mark as VERIFIED / PRUNED / CONFLICT
    pub async fn consolidate(&self) -> anyhow::Result<ConsolidationReport> {
        let start = std::time::Instant::now();

        let unverified = self.memory.list_unverified(self.batch_size).await
            .map_err(|e| anyhow::anyhow!("Failed to list unverified entries: {}", e))?;

        let total = unverified.len();
        let mut verified = 0usize;
        let mut pruned = 0usize;
        let mut conflicted = 0usize;

        for msg in &unverified {
            let text = msg.text();
            
            // Generate a unique ID for evaluation
            let docid = uuid::Uuid::new_v4().to_string();

            // Auditor-based evaluation
            let decision = self.evaluate(&docid, &text).await;
            match decision {
                ConsolidationDecision::Verify => {
                    let _ = self.memory.mark_verified(&text).await;
                    // Phase 14: Reward verification (implicit quality)
                    // The Memory implementation handles finding the document by content.
                    verified += 1;
                }
                ConsolidationDecision::Prune => {
                    let _ = self.memory.mark_pruned(&text).await;
                    pruned += 1;
                }
                ConsolidationDecision::Conflict => {
                    conflicted += 1;
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        let report = ConsolidationReport {
            entries_reviewed: total,
            entries_verified: verified,
            entries_pruned: pruned,
            entries_conflicted: conflicted,
            duration_ms,
        };

        tracing::info!(
            reviewed = total,
            verified = verified,
            pruned = pruned,
            conflicted = conflicted,
            duration_ms = duration_ms,
            "Sleep consolidation complete"
        );

        Ok(report)
    }

    /// Evaluate a memory entry using the auditor
    async fn evaluate(&self, docid: &str, content: &str) -> ConsolidationDecision {
        let change = ChangeType::MemoryPurification { docid: docid.to_string() };
        let result = self.auditor.audit(&change, content).await;

        match result {
            AuditResult::Approved => ConsolidationDecision::Verify,
            AuditResult::Rejected { .. } => ConsolidationDecision::Prune,
            AuditResult::NeedsReview { .. } => ConsolidationDecision::Conflict,
        }
    }
}

#[derive(Debug)]
enum ConsolidationDecision {
    Verify,
    Prune,
    Conflict,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::memory::InMemoryMemory;

    #[tokio::test]
    async fn test_consolidation_empty() {
        let memory = Arc::new(InMemoryMemory::new());
        let provider = Arc::new(crate::agent::provider::MockProvider::new("APPROVED"));
        let auditor = Arc::new(Auditor::new(provider, "test-model".to_string()));
        let consolidator = SleepConsolidator::new(memory, auditor);
        let report = consolidator.consolidate().await.unwrap();
        assert_eq!(report.entries_reviewed, 0);
    }
}
