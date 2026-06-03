use crate::config::discover::discover;
use crate::config::model::ServerStatus;
use crate::health::mcp_check::{self, McpStatus};
use std::time::Duration;

pub fn run(server_filter: Option<&str>, json: bool, timeout_secs: u64) {
    let entries = discover().unwrap_or_else(|e| {
        eprintln!("Warning: {}", e);
        Vec::new()
    });

    if entries.is_empty() {
        println!("No MCP servers found to check.");
        return;
    }

    let filtered: Vec<_> = entries
        .iter()
        .filter(|s| {
            if let Some(filter) = server_filter {
                s.name.to_lowercase().contains(&filter.to_lowercase())
            } else {
                true
            }
        })
        .collect();

    if filtered.is_empty() {
        println!("No servers match filter '{}'.", server_filter.unwrap_or(""));
        return;
    }

    let timeout = Duration::from_secs(timeout_secs);
    let mut results = Vec::new();

    if !json {
        println!("Checking {} MCP server(s)...\n", filtered.len());
    }

    for server in &filtered {
        // Skip servers with parse errors or missing commands
        if matches!(server.status, ServerStatus::ParseError(_)) || server.command.is_empty() {
            let result = mcp_check::McpCheckResult {
                server_name: server.name.clone(),
                status: McpStatus::ConnectionFailed("invalid config".into()),
                tools: vec![],
                resources: vec![],
                latency_ms: 0,
            };
            if !json {
                print_result(&result);
            }
            results.push(result);
            continue;
        }

        let result = mcp_check::check_server(
            &server.name,
            &server.command,
            &server.args,
            &server.env,
            timeout,
        );

        if !json {
            print_result(&result);
        }
        results.push(result);
    }

    if json {
        let json_output: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                let status_str = match &r.status {
                    McpStatus::Healthy => "healthy",
                    McpStatus::ConnectionFailed(_) => "failed",
                    McpStatus::Timeout => "timeout",
                    McpStatus::InvalidResponse(_) => "invalid",
                };
                let error = match &r.status {
                    McpStatus::ConnectionFailed(msg) | McpStatus::InvalidResponse(msg) => {
                        Some(msg.as_str())
                    }
                    _ => None,
                };
                let mut obj = serde_json::json!({
                    "server": r.server_name,
                    "status": status_str,
                    "tools_count": r.tools.len(),
                    "resources_count": r.resources.len(),
                    "latency_ms": r.latency_ms,
                });
                if let Some(err) = error {
                    obj["error"] = serde_json::Value::String(err.to_string());
                }
                if !r.tools.is_empty() {
                    obj["tools"] = serde_json::json!(r.tools);
                }
                if !r.resources.is_empty() {
                    obj["resources"] = serde_json::json!(r.resources);
                }
                obj
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_output).unwrap_or_default());
    } else {
        // Summary
        let healthy = results.iter().filter(|r| matches!(r.status, McpStatus::Healthy)).count();
        let failed = results
            .iter()
            .filter(|r| matches!(r.status, McpStatus::ConnectionFailed(_) | McpStatus::InvalidResponse(_)))
            .count();
        let timeouts = results.iter().filter(|r| matches!(r.status, McpStatus::Timeout)).count();
        println!(
            "\nResult: {}/{} healthy, {} failed, {} timeout",
            healthy, results.len(), failed, timeouts
        );
    }
}

fn print_result(result: &mcp_check::McpCheckResult) {
    match &result.status {
        McpStatus::Healthy => {
            let tools_str = if result.tools.is_empty() {
                String::new()
            } else {
                format!(" — {} tools", result.tools.len())
            };
            let resources_str = if result.resources.is_empty() {
                String::new()
            } else {
                format!(", {} resources", result.resources.len())
            };
            println!(
                "  {:<20} ✓ MCP handshake OK (stdio){}{}  [{}ms]",
                result.server_name, tools_str, resources_str, result.latency_ms
            );
        }
        McpStatus::ConnectionFailed(msg) => {
            println!(
                "  {:<20} ✗ Connection failed: {}",
                result.server_name, msg
            );
        }
        McpStatus::Timeout => {
            println!(
                "  {:<20} ⚠ No MCP response (timeout)",
                result.server_name
            );
        }
        McpStatus::InvalidResponse(msg) => {
            println!(
                "  {:<20} ✗ Invalid response: {}",
                result.server_name, msg
            );
        }
    }
}
