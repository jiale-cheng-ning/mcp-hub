use crate::config::discover::discover;
use crate::health::mcp_check::{self, McpStatus};
use std::time::Duration;

pub fn run(server_filter: Option<&str>, json: bool, timeout_secs: u64) {
    let entries = match discover() {
        Ok(entries) => entries,
        Err(_) => {
            eprintln!("Unable to discover server configs");
            return;
        }
    };
    let entries: Vec<_> = entries
        .into_iter()
        .filter(|s| server_filter.is_none_or(|f| s.name.to_lowercase().contains(&f.to_lowercase())))
        .collect();
    if entries.is_empty() {
        println!(
            "{}",
            if json {
                "[]"
            } else {
                "No matching MCP servers found."
            }
        );
        return;
    }
    let handles: Vec<_> = entries
        .into_iter()
        .map(|entry| {
            std::thread::spawn(move || {
                let client = entry.source_client.to_string();
                (
                    client,
                    mcp_check::check_entry(&entry, Duration::from_secs(timeout_secs)),
                )
            })
        })
        .collect();
    let mut output = Vec::new();
    for handle in handles {
        match handle.join() {
            Ok((client, result)) => {
                let (status, error) = match &result.status {
                    McpStatus::Healthy => ("healthy", None),
                    McpStatus::Timeout => {
                        ("timeout", Some("request deadline exceeded".to_string()))
                    }
                    McpStatus::ConnectionFailed(s) => ("failed", Some(s.clone())),
                    McpStatus::InvalidResponse(s) => ("invalid", Some(s.clone())),
                    McpStatus::Unsupported(s) => ("unsupported", Some(s.clone())),
                    McpStatus::AuthenticationFailed => (
                        "authentication_failed",
                        Some("Authentication required or forbidden".into()),
                    ),
                };
                if !json {
                    println!(
                        "  {}: {} [{}; protocol {}; {}ms; {} tools; {} resources]",
                        result.server_name,
                        result.status,
                        result.transport,
                        result.protocol.as_deref().unwrap_or("not negotiated"),
                        result.latency_ms,
                        result.tools.len(),
                        result.resources.len()
                    );
                }
                output.push(serde_json::json!({
                    "server":result.server_name,"client":client,"status":status,"error":error,
                    "protocol":result.protocol,"transport":result.transport,"latency_ms":result.latency_ms,
                    "tools_count":result.tools.len(),"resources_count":result.resources.len(),
                    "tools":result.tools,"resources":result.resources,
                }));
            }
            Err(_) => {
                output.push(serde_json::json!({"status":"failed","error":"probe worker failed"}))
            }
        }
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    }
}
