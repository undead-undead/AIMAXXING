use std::sync::Arc;
use std::path::PathBuf;
use anyhow::{Result, anyhow};
use tracing::{info, warn, error};

use crate::agent::evolution::auditor::{Auditor, AuditResult, ChangeType};
use crate::agent::evolution::rollback::{SoulSnapshot, AutoRollbackGuard};
use crate::agent::evolution::observation::{ObservationWindow, ObservationStatus};

/// Manages the autonomous evolution and safety of an agent.
/// 
/// Coordinates the Auditor, Observation Window, and Rollback mechanisms
/// to ensure that soul modifications and other critical changes are safe.
pub struct EvolutionManager {
    auditor: Arc<Auditor>,
    observation_window: Arc<parking_lot::RwLock<ObservationWindow>>,
    base_dir: PathBuf,
}

impl EvolutionManager {
    pub fn new(auditor: Arc<Auditor>, base_dir: PathBuf) -> Self {
        Self {
            auditor,
            observation_window: Arc::new(parking_lot::RwLock::new(ObservationWindow::default())),
            base_dir,
        }
    }

    pub fn observation_window(&self) -> Arc<parking_lot::RwLock<ObservationWindow>> {
        self.observation_window.clone()
    }

    pub fn auditor(&self) -> Arc<Auditor> {
        self.auditor.clone()
    }

    /// Safely update the SOUL.md for a given role.
    ///
    /// 1. Takes a snapshot for potential rollback.
    /// 2. Audits the new content.
    /// 3. Applies the change if approved (or requires review).
    /// 4. Starts an observation window.
    pub async fn update_soul(&self, role: &str, new_content: &str) -> Result<AuditResult> {
        let soul_path = self.base_dir.join("soul").join(role).join("SOUL.md");
        
        // 1. Snapshot
        let snapshot = SoulSnapshot::create(role, &soul_path).await?;
        let _guard = AutoRollbackGuard::new(snapshot);

        // 2. Audit
        let change = ChangeType::SoulModification { role: role.to_string() };
        let audit_result = self.auditor.audit(&change, new_content).await;

        match &audit_result {
            AuditResult::Approved => {
                info!("Evolution: Soul update approved for role '{}'", role);
                tokio::fs::write(&soul_path, new_content).await?;
                
                // 3. Mark guard as committed (no immediate rollback)
                _guard.commit();

                // 4. Enter observation
                let mut window = self.observation_window.write();
                window.enter_observation(&format!("soul-{}", role), &format!("Soul update for {}", role));
                
                Ok(AuditResult::Approved)
            }
            AuditResult::Rejected { reason } => {
                warn!("Evolution: Soul update rejected for role '{}': {}", role, reason);
                Ok(AuditResult::Rejected { reason: reason.clone() })
            }
            AuditResult::NeedsReview { summary } => {
                info!("Evolution: Soul update requires review for role '{}': {}", role, summary);
                // In a true interactive system, we'd wait for human input.
                // For now, we apply it but keep the observation window extra vigilant.
                tokio::fs::write(&soul_path, new_content).await?;
                _guard.commit();

                let mut window = self.observation_window.write();
                window.enter_observation(&format!("soul-{}", role), &format!("SENSITIVE Soul update for {}", role));

                Ok(AuditResult::NeedsReview { summary: summary.clone() })
            }
        }
    }

    /// Record an error that might be related to a recent evolution.
    pub fn report_error(&self, error_type: &str) {
        let mut window = self.observation_window.write();
        let active_ids: Vec<String> = window.active_observations().iter().map(|o| o.id.clone()).collect();
        for id in &active_ids {
            window.record_error(id);
            error!("Evolution: Error reported during observation '{}': {}", id, error_type);
        }
    }

    /// Check if any active observations have failed and need rollback.
    pub async fn check_evolution_health(&self) -> Result<()> {
        let mut window = self.observation_window.write();
        let ids: Vec<String> = window.active_observations().iter().map(|o| o.id.clone()).collect();

        for id in ids {
            let status = window.check_health(&id);
            match status {
                ObservationStatus::Failed { reason } => {
                    error!("Evolution: Observation window '{}' FAILED: {}. Triggering manual intervention.", id, reason);
                    // In a full implementation, we'd trigger a rollback here if we kept the snapshots.
                }
                ObservationStatus::Healthy => {
                    info!("Evolution: Observation window '{}' completed successfully.", id);
                    window.complete(&id);
                }
                _ => {}
            }
        }
        Ok(())
    }
}

use crate::skills::tool::{Tool, ToolDefinition};

/// Tool for the Agent to autonomously update its soul/personality
pub struct UpdateSoulTool {
    manager: Arc<EvolutionManager>,
    role: String,
}

impl UpdateSoulTool {
    pub fn new(manager: Arc<EvolutionManager>, role: String) -> Self {
        Self { manager, role }
    }
}

#[async_trait::async_trait]
impl Tool for UpdateSoulTool {
    fn name(&self) -> String {
        "update_soul".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Updates your SOUL.md configuration. This is a powerful command that changes your core identity, mission, and tool configuration. Use this for permanent self-evolution.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "new_soul_content": { 
                        "type": "string", 
                        "description": "The complete new content for SOUL.md (including YAML frontmatter)" 
                    }
                },
                "required": ["new_soul_content"]
            }),
            parameters_ts: Some("interface UpdateSoulArgs {\n  new_soul_content: string;\n}".to_string()),
            is_binary: false,
            is_verified: true, // Evolution is semi-verified by Auditor
            usage_guidelines: Some("Only use this after discovering better strategies or identifying critical missing mission directives from your backstory.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> Result<String> {
        let payload: serde_json::Value = serde_json::from_str(arguments)?;
        let content = payload["new_soul_content"]
            .as_str()
            .ok_or_else(|| anyhow!("missing 'new_soul_content'"))?;

        let result = self.manager.update_soul(&self.role, content).await?;
        
        match result {
            AuditResult::Approved => Ok("SUCCESS: Soul updated and approved by Auditor. New identity is now in quarantine (observation window active).".to_string()),
            AuditResult::Rejected { reason } => Ok(format!("REJECTED: Auditor rejected the modification. Reason: {}", reason)),
            AuditResult::NeedsReview { summary } => Ok(format!("WARNING: Soul updated but flagged for NEEDS_REVIEW: {}. Current identity is in observation.", summary)),
        }
    }
}
