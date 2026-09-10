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

    if !json {
        println!("Checking {} MCP server(s)...\n", filtered.len());
    }

    // Run all checks in parallel threads
    let handles: Vec<_> = filtered
        .iter()
        .map(|server| {
            let name = server.name.clone();
            let command = server.command.clone();
            let args = server.args.clone();
            let env = server.env.clone();
            let status_err = matches!(server.status, ServerStatus::ParseError(_));
            let is_empty_cmd = command.is_empty();

            std::thread::spawn(move || {
                if status_err || is_empty_cmd {
                    return mcp_check::McpCheckResult {
                        server_name: name,
                        status: McpStatus::ConnectionFailed("invalid config".into()),
                        tools: vec![],
                        resources: vec![],
                        latency_ms: 0,
                    };
                }
                mcp_check::check_server(&name, &command, &args, &env, timeout)
            })
        })
        .collect();

    let results: Vec<mcp_check::McpCheckResult> = handles
        .into_iter()
        .map(|h| h.join().unwrap_or_else(|_| mcp_check::McpCheckResult {
            server_name: "unknown".into(),
            status: McpStatus::ConnectionFailed("thread panicked".into()),
            tools: vec![],
            resources: vec![],
            latency_ms: 0,
        }))
        .collect();

    if !json {
        for result in &results {
            print_result(result);
        }
    }

    if json {
        let json_output: Vec<serde_json::Value> = results
            .iter()
            .zip(filtered.iter())
            .map(|(r, entry)| {
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
                    "client": format!("{}", entry.source_client),
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
        println!(
            "{}",
            serde_json::to_string_pretty(&json_output).unwrap_or_default()
        );
    } else {
        let healthy = results
            .iter()
            .filter(|r| matches!(r.status, McpStatus::Healthy))
            .count();
        let failed = results
            .iter()
            .filter(|r| {
                matches!(
                    r.status,
                    McpStatus::ConnectionFailed(_) | McpStatus::InvalidResponse(_)
                )
            })
            .count();
        let timeouts = results
            .iter()
            .filter(|r| matches!(r.status, McpStatus::Timeout))
            .count();
        println!(
            "\nResult: {}/{} healthy, {} failed, {} timeout",
            healthy,
            results.len(),
            failed,
            timeouts
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
            println!("  {:<20} ✗ Connection failed: {}", result.server_name, msg);
        }
        McpStatus::Timeout => {
            println!("  {:<20} ⚠ No MCP response (timeout)", result.server_name);
        }
        McpStatus::InvalidResponse(msg) => {
            println!("  {:<20} ✗ Invalid response: {}", result.server_name, msg);
        }
    }
}
