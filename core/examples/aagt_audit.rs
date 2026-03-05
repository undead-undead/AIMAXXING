//! AIMAXXING Security Audit Tool
//!
//! A CLI utility to check for exposed ports, weak configurations,
//! and authentication status of an AIMAXXING Gateway.

use brain::infra::aimaxxing_gateway::{protocol::ClientRole, GatewayConfig};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

fn main() {
    println!("🛡️  AIMAXXING Security Auditor v0.1.0");
    println!("=================================");

    let config = GatewayConfig::default();
    run_audit(&config);
}

fn run_audit(config: &GatewayConfig) {
    let mut score = 100;
    let mut findings = Vec::new();

    // 1. Check Binding
    println!("Checking network binding...");
    if config.host == "0.0.0.0" {
        findings.push("⚠️  [MEDIUM] Gateway is binding to 0.0.0.0 (all interfaces). Ensure a firewall is active if exposed to the public internet.");
        score -= 20;
    } else if config.host == "127.0.0.1" {
        println!("✅ Gateway is restricted to localhost.");
    }

    // 2. Check Authentication
    println!("Checking authentication...");
    if config.auth_token.is_none() {
        findings.push(
            "❌ [CRITICAL] No authentication token configured. Anyone can control your agents!",
        );
        score -= 60;
    } else {
        println!("✅ Authentication token is configured.");
    }

    // 3. Check Web Root (Secret Path)
    println!("Checking secret path...");
    if config.web_root == "/aimaxxing/" {
        findings.push("ℹ️  [LOW] Using default web root '/aimaxxing/'. Consider changing this to a more unique secret path.");
        score -= 5;
    } else {
        println!("✅ Custom secret path is in use.");
    }

    // 4. Port Check
    println!("Checking port status...");
    let addr = format!("{}:{}", config.host, config.port);
    if is_port_reachable(&addr) {
        println!("ℹ️  Port {} is reachable from this machine.", config.port);
    } else {
        println!(
            "ℹ️  Port {} is closed or not responding (Gateway might not be running).",
            config.port
        );
    }

    println!("\nSummary:");
    println!("--------");
    println!("Security Score: {}/100", score);

    if findings.is_empty() {
        println!("🎉 No major security issues detected!");
    } else {
        println!("Found {} issues:", findings.len());
        for finding in findings {
            println!("{}", finding);
        }
    }
}

fn is_port_reachable(addr: &str) -> bool {
    let addrs = addr.to_socket_addrs().ok();
    if let Some(mut iter) = addrs {
        if let Some(socket_addr) = iter.next() {
            return TcpStream::connect_timeout(&socket_addr, Duration::from_millis(500)).is_ok();
        }
    }
    false
}
