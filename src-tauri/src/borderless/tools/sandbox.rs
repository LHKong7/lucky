//! Sandbox — 4-tier permission system for tool execution.

use crate::borderless::agent_core::PermissionLevel;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Maximum permission level allowed. Default: Dangerous.
    #[serde(default = "default_max_level")]
    pub max_permission_level: PermissionLevel,
    /// Allowed file path patterns (glob-style).
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    /// Denied file path patterns.
    #[serde(default)]
    pub denied_paths: Vec<String>,
    /// Whether to completely disable the sandbox.
    #[serde(default)]
    pub dangerously_disable: bool,
}

fn default_max_level() -> PermissionLevel {
    PermissionLevel::Dangerous
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_permission_level: PermissionLevel::Dangerous,
            allowed_paths: Vec::new(),
            denied_paths: Vec::new(),
            dangerously_disable: false,
        }
    }
}

/// Risk level for a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Safe,
    Low,
    Moderate,
    High,
    Critical,
}

/// Result of analyzing a command's risk.
#[derive(Debug, Clone)]
pub struct CommandAnalysis {
    pub risk_level: RiskLevel,
    pub is_dangerous: bool,
    pub matched_patterns: Vec<String>,
}

/// Permission decision for a tool/command.
#[derive(Debug, Clone)]
pub struct PermissionDecision {
    pub behavior: PermissionBehavior,
    pub message: String,
    pub risk_level: RiskLevel,
    pub warning: Option<PermissionWarning>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionBehavior {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone)]
pub struct PermissionWarning {
    pub level: String,
    pub title: String,
    pub message: String,
}

/// Dangerous command patterns.
static DANGEROUS_PATTERNS: &[(&str, &str)] = &[
    (r"rm\s+(-[rRf]+\s+)", "recursive/force delete"),
    (r"rm\s+-[rRf]*\s+/", "delete from root"),
    (r"mkfs\b", "format filesystem"),
    (r"dd\s+", "raw disk write"),
    (r"chmod\s+-R\s+777", "world-writable permissions"),
    (r">\s*/dev/sd[a-z]", "write to raw disk"),
    (r"curl.*\|\s*(sh|bash)", "pipe remote script to shell"),
    (r"wget.*\|\s*(sh|bash)", "pipe remote script to shell"),
    (r"eval\s+", "arbitrary code execution"),
    (r"sudo\s+", "elevated privileges"),
];

/// Safe command patterns (auto-allowed).
static SAFE_PATTERNS: &[&str] = &[
    r"^ls\b",
    r"^cat\b",
    r"^head\b",
    r"^tail\b",
    r"^wc\b",
    r"^echo\b",
    r"^pwd$",
    r"^whoami$",
    r"^date$",
    r"^git\s+(status|log|diff|branch|show)\b",
    r"^git\s+rev-parse\b",
    r"^find\b",
    r"^grep\b",
    r"^rg\b",
];

/// The sandbox that enforces permission policies.
pub struct Sandbox {
    config: SandboxConfig,
    dangerous_patterns: Vec<(Regex, String)>,
    safe_patterns: Vec<Regex>,
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Self {
        let dangerous_patterns: Vec<(Regex, String)> = DANGEROUS_PATTERNS
            .iter()
            .filter_map(|&(pat, desc)| {
                Regex::new(pat).ok().map(|r| (r, desc.to_string()))
            })
            .collect();

        let safe_patterns: Vec<Regex> = SAFE_PATTERNS
            .iter()
            .filter_map(|pat| Regex::new(pat).ok())
            .collect();

        Self {
            config,
            dangerous_patterns,
            safe_patterns,
        }
    }

    /// Analyze a command's risk level.
    pub fn analyze_command(&self, command: &str) -> CommandAnalysis {
        if self.config.dangerously_disable {
            return CommandAnalysis {
                risk_level: RiskLevel::Safe,
                is_dangerous: false,
                matched_patterns: Vec::new(),
            };
        }

        // Check safe patterns
        for pattern in &self.safe_patterns {
            if pattern.is_match(command) {
                return CommandAnalysis {
                    risk_level: RiskLevel::Safe,
                    is_dangerous: false,
                    matched_patterns: Vec::new(),
                };
            }
        }

        // Check dangerous patterns
        let mut matched = Vec::new();
        for (pattern, description) in &self.dangerous_patterns {
            if pattern.is_match(command) {
                matched.push(description.clone());
            }
        }

        if !matched.is_empty() {
            return CommandAnalysis {
                risk_level: RiskLevel::High,
                is_dangerous: true,
                matched_patterns: matched,
            };
        }

        // Default: moderate risk for unknown commands
        CommandAnalysis {
            risk_level: RiskLevel::Moderate,
            is_dangerous: false,
            matched_patterns: Vec::new(),
        }
    }

    /// Check if a path is allowed by the sandbox configuration.
    pub fn check_path(&self, path: &str) -> bool {
        if self.config.dangerously_disable {
            return true;
        }

        // Check denied paths first
        for denied in &self.config.denied_paths {
            if path.starts_with(denied) || path.contains(denied) {
                return false;
            }
        }

        // If allowed_paths is set, path must match one
        if !self.config.allowed_paths.is_empty() {
            return self.config.allowed_paths.iter().any(|allowed| {
                path.starts_with(allowed) || path.contains(allowed)
            });
        }

        true
    }

    /// Make a permission decision for a tool.
    pub fn check_permission(
        &self,
        tool_name: &str,
        permission_level: PermissionLevel,
    ) -> PermissionDecision {
        if self.config.dangerously_disable {
            return PermissionDecision {
                behavior: PermissionBehavior::Allow,
                message: "Sandbox disabled".into(),
                risk_level: RiskLevel::Safe,
                warning: None,
            };
        }

        if permission_level > self.config.max_permission_level {
            return PermissionDecision {
                behavior: PermissionBehavior::Deny,
                message: format!(
                    "Tool '{}' requires {:?} permission, but max allowed is {:?}",
                    tool_name, permission_level, self.config.max_permission_level
                ),
                risk_level: RiskLevel::High,
                warning: Some(PermissionWarning {
                    level: "error".into(),
                    title: "Permission denied".into(),
                    message: format!("Tool '{}' exceeds sandbox permissions", tool_name),
                }),
            };
        }

        match permission_level {
            PermissionLevel::Safe => PermissionDecision {
                behavior: PermissionBehavior::Allow,
                message: format!("Tool '{}' is safe", tool_name),
                risk_level: RiskLevel::Safe,
                warning: None,
            },
            PermissionLevel::Moderate => PermissionDecision {
                behavior: PermissionBehavior::Allow,
                message: format!("Tool '{}' has moderate permissions", tool_name),
                risk_level: RiskLevel::Moderate,
                warning: None,
            },
            PermissionLevel::Dangerous => PermissionDecision {
                behavior: PermissionBehavior::Ask,
                message: format!("Tool '{}' requires approval", tool_name),
                risk_level: RiskLevel::High,
                warning: Some(PermissionWarning {
                    level: "warning".into(),
                    title: "Dangerous operation".into(),
                    message: format!("Tool '{}' may modify system state", tool_name),
                }),
            },
            PermissionLevel::Critical => PermissionDecision {
                behavior: PermissionBehavior::Ask,
                message: format!("Tool '{}' is critical", tool_name),
                risk_level: RiskLevel::Critical,
                warning: Some(PermissionWarning {
                    level: "error".into(),
                    title: "Critical operation".into(),
                    message: format!("Tool '{}' has unrestricted access", tool_name),
                }),
            },
        }
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new(SandboxConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_commands() {
        let sandbox = Sandbox::default();
        assert_eq!(sandbox.analyze_command("ls -la").risk_level, RiskLevel::Safe);
        assert_eq!(sandbox.analyze_command("git status").risk_level, RiskLevel::Safe);
        assert_eq!(sandbox.analyze_command("cat file.txt").risk_level, RiskLevel::Safe);
    }

    #[test]
    fn test_dangerous_commands() {
        let sandbox = Sandbox::default();
        let analysis = sandbox.analyze_command("rm -rf /");
        assert!(analysis.is_dangerous);
        assert_eq!(analysis.risk_level, RiskLevel::High);
    }

    #[test]
    fn test_permission_check() {
        let sandbox = Sandbox::default();
        let decision = sandbox.check_permission("bash", PermissionLevel::Safe);
        assert_eq!(decision.behavior, PermissionBehavior::Allow);

        let decision = sandbox.check_permission("bash", PermissionLevel::Critical);
        // Default max_permission_level is Dangerous, so Critical is denied
        assert_eq!(decision.behavior, PermissionBehavior::Deny);

        let decision = sandbox.check_permission("bash", PermissionLevel::Dangerous);
        assert_eq!(decision.behavior, PermissionBehavior::Ask);
    }
}
