//! Phase 12-A: Observation window for quarantining new changes.
//!
//! New interactions/changes are isolated for a configurable period
//! before being promoted to full status.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Status of an observation
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum ObservationStatus {
    /// Within observation window, monitoring
    Active,
    /// Observation completed successfully
    Healthy,
    /// Anomalies detected during observation
    Degraded { anomalies: Vec<String> },
    /// Observation failed, rollback recommended
    Failed { reason: String },
}

/// Handle to track a specific observation
#[derive(Debug, Clone)]
pub struct ObservationHandle {
    pub id: String,
    pub started_at: Instant,
    pub duration: Duration,
    pub change_description: String,
}

/// Manages observation windows for newly applied changes.
pub struct ObservationWindow {
    /// Active observations being monitored
    observations: HashMap<String, ObservationHandle>,
    /// Duration for the observation window (default 60 minutes)
    window_duration: Duration,
    /// Error counter per observation
    error_counts: HashMap<String, u32>,
    /// Threshold for error count before marking as failed
    error_threshold: u32,
}

impl Default for ObservationWindow {
    fn default() -> Self {
        Self {
            observations: HashMap::new(),
            window_duration: Duration::from_secs(60 * 60), // 60 minutes
            error_counts: HashMap::new(),
            error_threshold: 5,
        }
    }
}

impl ObservationWindow {
    /// Create with custom window duration
    pub fn with_duration(duration: Duration) -> Self {
        Self {
            window_duration: duration,
            ..Default::default()
        }
    }

    /// Enter a new observation window for a change
    pub fn enter_observation(&mut self, change_id: &str, description: &str) -> ObservationHandle {
        let handle = ObservationHandle {
            id: change_id.to_string(),
            started_at: Instant::now(),
            duration: self.window_duration,
            change_description: description.to_string(),
        };
        self.observations
            .insert(change_id.to_string(), handle.clone());
        self.error_counts.insert(change_id.to_string(), 0);
        handle
    }

    /// Record an error during observation
    pub fn record_error(&mut self, change_id: &str) {
        if let Some(count) = self.error_counts.get_mut(change_id) {
            *count += 1;
        }
    }

    /// Check the health status of an observation
    pub fn check_health(&self, change_id: &str) -> ObservationStatus {
        let handle = match self.observations.get(change_id) {
            Some(h) => h,
            None => {
                return ObservationStatus::Failed {
                    reason: "Observation not found".to_string(),
                }
            }
        };

        let error_count = self.error_counts.get(change_id).copied().unwrap_or(0);
        let elapsed = handle.started_at.elapsed();

        // Check if error threshold exceeded
        if error_count >= self.error_threshold {
            return ObservationStatus::Failed {
                reason: format!(
                    "Error count ({}) exceeded threshold ({})",
                    error_count, self.error_threshold
                ),
            };
        }

        // Check if observation window has expired
        if elapsed >= handle.duration {
            if error_count > 0 {
                return ObservationStatus::Degraded {
                    anomalies: vec![format!(
                        "{} errors recorded during observation",
                        error_count
                    )],
                };
            }
            return ObservationStatus::Healthy;
        }

        ObservationStatus::Active
    }

    /// Remove a completed observation
    pub fn complete(&mut self, change_id: &str) {
        self.observations.remove(change_id);
        self.error_counts.remove(change_id);
    }

    /// List all active observations
    pub fn active_observations(&self) -> Vec<&ObservationHandle> {
        self.observations.values().collect()
    }

    /// Check if any observations are currently active
    pub fn is_active(&self) -> bool {
        !self.observations.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observation_lifecycle() {
        let mut window = ObservationWindow::with_duration(Duration::from_millis(10));
        let handle = window.enter_observation("test-1", "Test change");

        // Should be active immediately
        assert_eq!(window.check_health("test-1"), ObservationStatus::Active);
        assert_eq!(handle.id, "test-1");

        // Wait for window to expire
        std::thread::sleep(Duration::from_millis(15));
        assert_eq!(window.check_health("test-1"), ObservationStatus::Healthy);

        window.complete("test-1");
        assert!(window.active_observations().is_empty());
    }

    #[test]
    fn test_observation_error_threshold() {
        let mut window = ObservationWindow::default();
        window.error_threshold = 3;
        window.enter_observation("test-2", "Error test");

        window.record_error("test-2");
        window.record_error("test-2");
        assert_eq!(window.check_health("test-2"), ObservationStatus::Active);

        window.record_error("test-2"); // Threshold reached
        assert!(matches!(
            window.check_health("test-2"),
            ObservationStatus::Failed { .. }
        ));
    }
}
