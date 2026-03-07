//! Phase 11-C: Runtime environment dependency scanner.
//!
//! Checks presence and version of external tools the agent may need.

use std::collections::HashMap;
use std::path::PathBuf;

/// Status of a single dependency check
#[derive(Debug, Clone, serde::Serialize)]
pub enum DependencyStatus {
    Available { version: String, path: PathBuf },
    Missing,
    WrongVersion { expected: String, actual: String },
}

/// Result of a full environment scan
#[derive(Debug, Clone, serde::Serialize)]
pub struct EnvReport {
    pub results: HashMap<String, DependencyStatus>,
    pub all_satisfied: bool,
}

/// Scans the runtime environment for required dependencies.
pub struct EnvScanner {
    /// List of (dependency_name, optional minimum version)
    dependencies: Vec<(String, Option<String>)>,
}

impl Default for EnvScanner {
    fn default() -> Self {
        Self {
            dependencies: vec![
                ("git".into(), None),
                ("python3".into(), None),
                ("ffmpeg".into(), None),
                ("pandoc".into(), None),
                ("bwrap".into(), None),
                ("node".into(), None),
                ("cargo".into(), None),
            ],
        }
    }
}

impl EnvScanner {
    /// Create a scanner with a custom dependency list
    pub fn new(deps: Vec<(String, Option<String>)>) -> Self {
        Self { dependencies: deps }
    }

    /// Run the full dependency scan
    pub async fn scan(&self) -> EnvReport {
        let mut results = HashMap::new();
        let mut all_ok = true;

        for (name, min_version) in &self.dependencies {
            let status = self.check_one(name, min_version.as_deref()).await;
            if matches!(status, DependencyStatus::Missing | DependencyStatus::WrongVersion { .. }) {
                all_ok = false;
            }
            results.insert(name.clone(), status);
        }

        EnvReport {
            results,
            all_satisfied: all_ok,
        }
    }

    /// Check a single dependency using `which` + `--version`
    async fn check_one(&self, name: &str, _min_version: Option<&str>) -> DependencyStatus {
        // Try `which <name>` to find the path
        let which_result = tokio::process::Command::new("which")
            .arg(name)
            .output()
            .await;

        let path = match which_result {
            Ok(output) if output.status.success() => {
                let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
                PathBuf::from(p)
            }
            _ => return DependencyStatus::Missing,
        };

        // Try `<name> --version` to get version string
        let version_result = tokio::process::Command::new(name)
            .arg("--version")
            .output()
            .await;

        let version = match version_result {
            Ok(output) if output.status.success() => {
                let raw = String::from_utf8_lossy(&output.stdout);
                // Extract first line as version info
                raw.lines().next().unwrap_or("unknown").trim().to_string()
            }
            _ => "unknown".to_string(),
        };

        DependencyStatus::Available { version, path }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_env_scanner_default() {
        let scanner = EnvScanner::default();
        let report = scanner.scan().await;
        // At minimum, these should not panic
        assert!(!report.results.is_empty());
    }

    #[tokio::test]
    async fn test_env_scanner_custom() {
        let scanner = EnvScanner::new(vec![
            ("ls".into(), None),                    // should exist
            ("nonexistent_tool_xyz".into(), None),   // should not exist
        ]);
        let report = scanner.scan().await;
        assert!(matches!(report.results.get("ls"), Some(DependencyStatus::Available { .. })));
        assert!(matches!(report.results.get("nonexistent_tool_xyz"), Some(DependencyStatus::Missing)));
        assert!(!report.all_satisfied);
    }
}
