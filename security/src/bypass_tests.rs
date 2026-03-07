use super::*;
use shell_firewall::ShellFirewall;

#[test]
fn test_obfuscation_bypasses() {
    let firewall = ShellFirewall;

    // 1. Quoted commands
    // Note: Our current regex (?i)\brm\b won't match "r'm'" or "'rm'".
    // This is a known limitation of regex-based firewalls without a real parser.
    // However, we should see if we can catch common patterns.

    // 2. Backslash escapes
    // assert!(ShellFirewall::check("r\\m -rf /").blocked);

    // 3. Variable assignments (caught if the pattern is in the string)
    assert!(ShellFirewall::check("A=rm; B=-rf; $A $B /").blocked); // Matches "-rf" rule

    // 4. Command chaining
    assert!(ShellFirewall::check("ls -la; rm -rf /").blocked);
    assert!(ShellFirewall::check("ls -la && rm -rf /").blocked);
    assert!(ShellFirewall::check("ls -la || rm -rf /").blocked);
}

#[test]
fn test_path_traversal_sensitive() {
    // Basic sensitive path
    assert!(ShellFirewall::check("cat /etc/shadow").blocked);

    // Traversal (normalized to /)
    assert!(ShellFirewall::check("cat /etc/../etc/shadow").blocked);

    // Windows 스타일
    assert!(ShellFirewall::check("type C:\\Windows\\System32\\config\\SAM").blocked);
}

#[test]
fn test_encoded_payloads() {
    // Base64 (Rule: eval_base64)
    assert!(ShellFirewall::check("echo \"cm0gLXJmIC8=\" | base64 -d | sh").blocked);

    // Hex (caught if it piped to bash)
    assert!(ShellFirewall::check("echo 726d202d7266202f | xxd -r -p | bash").blocked);
}
