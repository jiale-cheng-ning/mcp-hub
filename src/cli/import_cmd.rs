use crate::config::model::{ClientType, ExportData};
use std::path::PathBuf;

pub fn run(file: &str, target: Option<&str>) {
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read {}: {}", file, e);
            return;
        }
    };

    let export: ExportData = match serde_json::from_str(&content) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Invalid export file: {}", e);
            return;
        }
    };

    if export.servers.is_empty() {
        println!("No servers in export file.");
        return;
    }

    // Determine target client
    let client = match target {
        Some(t) => match ClientType::from_str_loose(t) {
            Some(c) => c,
            None => {
                eprintln!("Unknown client '{}'. Use: claude-desktop, claude-code, cursor, vscode, windsurf", t);
                return;
            }
        },
        None => {
            // Default: prompt or pick first available
            println!("No --target specified. Available clients:");
            println!("  claude-desktop, claude-code, cursor, vscode, windsurf");
            println!("\nUsage: mcp-hub import {} --target <client>", file);
            return;
        }
    };

    // Get config path for target client
    let config_path = get_config_path(&client);
    let config_path = match config_path {
        Some(p) => p,
        None => {
            eprintln!("Could not determine config path for {}", client);
            return;
        }
    };

    // Read existing config or create empty
    let mut existing: serde_json::Value = if config_path.exists() {
        match std::fs::read_to_string(&config_path) {
            Ok(content) => serde_json::from_str(&content)
                .unwrap_or_else(|_| serde_json::json!({ "mcpServers": {} })),
            Err(_) => serde_json::json!({ "mcpServers": {} }),
        }
    } else {
        serde_json::json!({ "mcpServers": {} })
    };

    // Ensure mcpServers object exists
    if existing.get("mcpServers").is_none() {
        existing["mcpServers"] = serde_json::json!({});
    }

    let config_json = export.to_config_json();
    let new_servers = config_json
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let existing_servers = existing
        .get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
        .unwrap();

    let mut imported = 0;
    let mut skipped = 0;

    for (name, server_val) in &new_servers {
        if existing_servers.contains_key(name) {
            skipped += 1;
            println!("  ⏭ {} (already exists, skipped)", name);
        } else {
            existing_servers.insert(name.clone(), server_val.clone());
            imported += 1;
            println!("  ✓ {}", name);
        }
    }

    // Write back
    let output = serde_json::to_string_pretty(&existing).unwrap_or_default();
    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&config_path, &output) {
        Ok(()) => {
            println!(
                "\nImported {} server(s) to {} ({})",
                imported,
                client,
                config_path.display()
            );
            if skipped > 0 {
                println!("Skipped {} existing server(s)", skipped);
            }
        }
        Err(e) => {
            eprintln!("Failed to write {}: {}", config_path.display(), e);
        }
    }
}

fn get_config_path(client: &ClientType) -> Option<PathBuf> {
    match client {
        ClientType::ClaudeDesktop => {
            dirs::config_dir().map(|p| p.join("Claude").join("claude_desktop_config.json"))
        }
        ClientType::ClaudeCode => dirs::home_dir().map(|p| p.join(".claude").join("settings.json")),
        ClientType::Cursor => dirs::home_dir().map(|p| p.join(".cursor").join("mcp.json")),
        ClientType::VSCode => dirs::home_dir().map(|p| p.join(".vscode").join("mcp.json")),
        ClientType::Windsurf => {
            dirs::home_dir().map(|p| p.join(".codeium").join("windsurf").join("mcp_config.json"))
        }
    }
}
