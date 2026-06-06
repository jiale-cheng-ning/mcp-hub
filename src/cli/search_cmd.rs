use crate::config::model::ClientType;
use crate::registry;
use std::io::{self, Write};

pub fn run(query: &str, limit: usize, install: bool, target: Option<&str>) {
    println!("Searching MCP Registry for \"{}\"...\n", query);

    let results = match registry::search(query, limit) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Search failed: {}", e);
            return;
        }
    };

    if results.is_empty() {
        println!("No servers found matching '{}'.", query);
        return;
    }

    for (i, server) in results.iter().enumerate() {
        println!("  {}. {}", i + 1, server.server.name);
        if !server.server.description.is_empty() {
            println!("     {}", server.server.description);
        }
        println!();
    }

    println!("{} server(s) found.", results.len());

    if !install {
        println!("\nUse --install to add a server to your config.");
        return;
    }

    // Install flow
    let target_client = match target {
        Some(t) => match ClientType::from_str_loose(t) {
            Some(c) => c,
            None => {
                eprintln!(
                    "Unknown client '{}'. Use: claude-desktop, claude-code, cursor, vscode, windsurf",
                    t
                );
                return;
            }
        },
        None => {
            println!("\nNo --target specified. Available clients:");
            println!("  claude-desktop, claude-code, cursor, vscode, windsurf");
            println!(
                "\nUsage: mcp-hub search \"{}\" --install --target <client>",
                query
            );
            return;
        }
    };

    // Pick which server to install
    let chosen = if results.len() == 1 {
        &results[0]
    } else {
        print!("\nEnter server number to install (1-{}): ", results.len());
        let _ = io::stdout().flush();
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            eprintln!("Failed to read input");
            return;
        }
        let idx: usize = match input.trim().parse::<usize>() {
            Ok(n) if n >= 1 && n <= results.len() => n - 1,
            _ => {
                eprintln!("Invalid selection");
                return;
            }
        };
        &results[idx]
    };

    // Get latest version info
    println!("\nFetching latest version of {}...", chosen.server.name);
    let version = match registry::get_latest(&chosen.server.name) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to get version info: {}", e);
            return;
        }
    };

    // Find npm package
    let npm_package = version
        .packages
        .iter()
        .find(|p| p.pkg_type == "npm")
        .or_else(|| version.packages.first());

    let pkg = match npm_package {
        Some(p) => p,
        None => {
            eprintln!("No installable package found for this server.");
            return;
        }
    };

    println!(
        "Installing {} ({}) v{} to {}...",
        chosen.server.name, pkg.identifier, version.version, target_client
    );

    // Generate config entry
    let server_name = chosen
        .server
        .name
        .split('/')
        .next_back()
        .unwrap_or(&chosen.server.name)
        .to_string();

    let config_path = match get_config_path(&target_client) {
        Some(p) => p,
        None => {
            eprintln!("Could not determine config path for {}", target_client);
            return;
        }
    };

    // Read existing config or create empty
    let mut existing: serde_json::Value = if config_path.exists() {
        std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({ "mcpServers": {} }))
    } else {
        serde_json::json!({ "mcpServers": {} })
    };

    if existing.get("mcpServers").is_none() {
        existing["mcpServers"] = serde_json::json!({});
    }

    let servers = existing
        .get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
        .unwrap();

    if servers.contains_key(&server_name) {
        println!(
            "Server '{}' already exists in config. Skipping.",
            server_name
        );
        return;
    }

    // Build the server config
    let mut entry = serde_json::Map::new();
    entry.insert("command".into(), serde_json::Value::String("npx".into()));
    entry.insert("args".into(), serde_json::json!(["-y", pkg.identifier]));

    servers.insert(server_name.clone(), serde_json::Value::Object(entry));

    // Write config
    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let output = serde_json::to_string_pretty(&existing).unwrap_or_default();
    match std::fs::write(&config_path, &output) {
        Ok(()) => {
            println!(
                "\n✓ Added '{}' to {} ({})",
                server_name,
                target_client,
                config_path.display()
            );
            println!("  command: npx -y {}", pkg.identifier);
            println!("\nRestart {} to load the new server.", target_client);
        }
        Err(e) => {
            eprintln!("Failed to write config: {}", e);
        }
    }
}

fn get_config_path(client: &ClientType) -> Option<std::path::PathBuf> {
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
