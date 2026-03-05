//! Security module for AIMAXXING.
//!
//! Provides utilities for:
//! 1. Secret Leak Detection (redacting API keys from tool outputs).
//! 2. Prompt Injection Detection (detecting adversarial inputs).
//! 3. Policy Enforcement (optional future extension).

pub mod injection;
pub mod leaks;
pub mod output_auditor;
pub mod shell_firewall;
pub mod skill_verifier;
pub mod vessel;

pub use injection::{InjectionDetector, SanitizedOutput};
pub use leaks::{LeakAction, LeakDetection, LeakDetector};
pub use shell_firewall::ShellFirewall;
pub use vessel::VesselInspector;

/// Security configuration
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityConfig {
    pub leak_detection_enabled: bool,
    pub injection_check_enabled: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            leak_detection_enabled: true,
            injection_check_enabled: true,
        }
    }
}

/// Central manager for security checks.
pub struct SecurityManager {
    config: SecurityConfig,
    leak_detector: LeakDetector,
    injection_detector: InjectionDetector,
}

impl SecurityManager {
    pub fn new(config: SecurityConfig) -> Self {
        Self {
            config,
            leak_detector: LeakDetector::new(),
            injection_detector: InjectionDetector::new(),
        }
    }

    /// Scan input text (usually from User) for prompt injection attempts.
    /// Returns sanitized text (wrapped in markers) if enabled.
    pub fn check_input(&self, text: &str) -> SanitizedOutput {
        if self.config.injection_check_enabled {
            self.injection_detector.check_injection(text)
        } else {
            SanitizedOutput {
                content: text.to_string(),
                warnings: vec![],
                was_modified: false,
            }
        }
    }

    /// Scan output text (usually from Tools) for secret leaks.
    /// Returns redacted text and detections.
    pub fn check_output(&self, text: &str) -> (String, Vec<LeakDetection>) {
        if self.config.leak_detection_enabled {
            self.leak_detector.redact(text)
        } else {
            (text.to_string(), vec![])
        }
    }
}

impl Default for SecurityManager {
    fn default() -> Self {
        Self::new(SecurityConfig::default())
    }
}

#[cfg(test)]
mod tests;
