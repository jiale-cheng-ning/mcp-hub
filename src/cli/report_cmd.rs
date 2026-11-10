//! Small, versioned DTOs for plugins. Never project raw configs or server text.
use crate::audit::{engine::run_audit, rules::all_rules};
use crate::config::{
    discover::discover_with_warnings,
    model::{ClientType, ServerStatus},
};
use crate::health::mcp_check::{check_entry, McpStatus};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Operation {
    Scan,
    Audit,
    Doctor,
}

pub fn run(operation: Operation, client: Option<&str>, server: Option<&str>, timeout: u64) {
    match report(operation, client, server, timeout) {
        Ok(value) => println!("{value}"),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}

fn report(
    operation: Operation,
    client: Option<&str>,
    server: Option<&str>,
    timeout: u64,
) -> Result<Value, &'static str> {
    let client_type = client
        .map(|c| ClientType::from_str_loose(c).ok_or("Unknown client"))
        .transpose()?;
    let (entries, warnings) = discover_with_warnings(client_type.as_ref());
    let entries: Vec<_> = entries
        .into_iter()
        .filter(|e| {
            client_type.as_ref().is_none_or(|c| c == &e.source_client)
                && server.is_none_or(|s| s == e.name)
        })
        .collect();
    let warnings: Vec<_> = warnings
        .into_iter()
        .filter(|w| {
            client_type
                .as_ref()
                .is_none_or(|c| c.to_string() == w.client)
        })
        .collect();
    let mut results = match operation {
        Operation::Scan => entries.iter().map(|e| json!({
            "server": e.name, "client": e.source_client.to_string(), "transport": e.transport.label(),
            "status": match e.status { ServerStatus::Active => "active", ServerStatus::Disabled => "disabled", ServerStatus::ParseError(_) => "invalid_config" },
        })).collect::<Vec<_>>(),
        Operation::Audit => run_audit(&entries).iter().map(|f| {
            // CLI findings may quote arguments. Use static descriptions to
            // keep credentials out of model context and durable tool results.
            let description = all_rules().iter().find(|r| r.id() == f.rule_id)
                .map(|r| r.description()).unwrap_or("Legacy SSE transport cannot be actively probed");
            json!({"server": f.server_name, "rule": f.rule_id, "severity": f.severity, "description": description})
        }).collect(),
        Operation::Doctor => {
            if server.is_none_or(|s| s.trim().is_empty()) { return Err("Doctor requires an exact server name"); }
            if entries.len() != 1 { return Err("Doctor requires exactly one match; select a client to disambiguate"); }
            if !matches!(entries[0].status, ServerStatus::Active) { return Err("Cannot probe a disabled or invalid server"); }
            let r = check_entry(&entries[0], Duration::from_secs(timeout));
            let status = match r.status {
                McpStatus::Healthy => "healthy", McpStatus::Timeout => "timeout",
                McpStatus::ConnectionFailed(_) => "connection_failed", McpStatus::InvalidResponse(_) => "invalid_response",
                McpStatus::Unsupported(_) => "unsupported", McpStatus::AuthenticationFailed => "authentication_failed",
            };
            vec![json!({"server": r.server_name, "client": entries[0].source_client.to_string(), "status": status,
                "transport": r.transport, "protocol": r.protocol, "latency_ms": r.latency_ms,
                "tools_count": r.tools.len(), "resources_count": r.resources.len()})]
        }
    };
    let total = results.len();
    results.truncate(200);
    let invalid_configs = entries
        .iter()
        .filter(|e| matches!(e.status, ServerStatus::ParseError(_)))
        .count();
    Ok(
        json!({"api_version": 1, "operation": match operation { Operation::Scan => "scan", Operation::Audit => "audit", Operation::Doctor => "doctor" },
        "servers_scanned": entries.len(), "invalid_configs": invalid_configs, "warnings": warnings,
        "results": results, "total": total, "truncated": total > 200}),
    )
}
