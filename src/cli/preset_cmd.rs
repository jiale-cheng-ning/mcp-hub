use crate::config::model::ClientType;
use crate::preset;

pub fn run(subcmd: &str, name: Option<&str>, target: Option<&str>, json: bool) {
    match subcmd {
        "list" => list_presets(json),
        "install" => {
            let preset_name = match name {
                Some(n) => n,
                None => {
                    eprintln!("Usage: mcp-hub preset install <name> --target <client>");
                    eprintln!("Run 'mcp-hub preset list' to see available presets.");
                    return;
                }
            };
            install_preset(preset_name, target);
        }
        _ => {
            eprintln!("Unknown subcommand '{}'. Use 'list' or 'install'.", subcmd);
        }
    }
}

fn list_presets(json: bool) {
    let presets = match preset::list_presets() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to load presets: {}", e);
            return;
        }
    };

    if json {
        let entries: Vec<serde_json::Value> = presets
            .iter()
            .map(|(name, desc)| serde_json::json!({"name": name, "description": desc}))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&entries).unwrap_or_default()
        );
        return;
    }

    println!("Available presets:\n");
    for (name, desc) in &presets {
        println!("  {:<16} {}", name, desc);
    }
    println!("\nInstall with: mcp-hub preset install <name> --target <client>");
}

fn install_preset(name: &str, target: Option<&str>) {
    let preset = match preset::get_preset(name) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

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
            println!("Preset '{}' — {}", name, preset.description);
            println!(
                "Servers: {}",
                preset
                    .servers
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            println!("\nNo --target specified. Available clients:");
            println!("  claude-desktop, claude-code, cursor, vscode, windsurf");
            println!("\nUsage: mcp-hub preset install {} --target <client>", name);
            return;
        }
    };

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

    println!("Installing preset '{}' to {}...\n", name, target_client);

    let mut installed = 0;
    let mut skipped = 0;

    for (server_name, server_def) in &preset.servers {
        if servers.contains_key(server_name) {
            println!("  ⏭  {} (already exists)", server_name);
            skipped += 1;
            continue;
        }

        let mut entry = serde_json::Map::new();
        entry.insert("command".into(), serde_json::Value::String("npx".into()));
        entry.insert("args".into(), serde_json::json!(server_def.args));

        // Add env vars as placeholders
        if !server_def.env.is_empty() {
            let env_map: serde_json::Map<String, serde_json::Value> = server_def
                .env
                .iter()
                .map(|k| (k.clone(), serde_json::Value::String(format!("${{{}}}", k))))
                .collect();
            entry.insert("env".into(), serde_json::Value::Object(env_map));
        }

        servers.insert(server_name.clone(), serde_json::Value::Object(entry));
        println!("  ✓  {}", server_name);
        installed += 1;
    }

    // Write config
    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let output = serde_json::to_string_pretty(&existing).unwrap_or_default();
    match std::fs::write(&config_path, &output) {
        Ok(()) => {
            println!(
                "\nDone! Installed {} server(s), skipped {} (already exist).",
                installed, skipped
            );
            println!("Config written to: {}", config_path.display());
            println!("\nRestart {} to load the new servers.", target_client);

            // Remind about env vars
            let env_vars: Vec<&str> = preset
                .servers
                .values()
                .flat_map(|s| s.env.iter())
                .map(|s| s.as_str())
                .collect();
            if !env_vars.is_empty() {
                println!(
                    "\n⚠  Set these environment variables before using:\n   {}",
                    env_vars.join(", ")
                );
            }
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
