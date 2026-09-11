use crate::config::discover::config_path_for;
use crate::config::model::ClientType;
use crate::preset;

const CLIENT_HELP: &str = "claude-desktop, claude-code, cursor, vscode, windsurf, dsh";

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
                eprintln!("Unknown client '{}'. Use: {}", t, CLIENT_HELP);
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
            println!("  {}", CLIENT_HELP);
            println!("\nUsage: mcp-hub preset install {} --target <client>", name);
            return;
        }
    };

    if matches!(target_client, ClientType::DeepSeekHarness) {
        install_preset_dsh(name, &preset);
        return;
    }

    let config_path = match config_path_for(&target_client) {
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

fn install_preset_dsh(preset_name: &str, preset: &crate::preset::Preset) {
    let config_path = match crate::config::discover::dsh_primary_config_path() {
        Some(p) => p,
        None => {
            eprintln!("Could not determine DeepSeek Harness home");
            return;
        }
    };

    println!(
        "Installing preset '{}' to DeepSeek Harness...\n",
        preset_name
    );

    let servers: Vec<crate::config::parser::DshServerSpec> = preset
        .servers
        .iter()
        .map(|(name, def)| {
            let env = def
                .env
                .iter()
                .map(|k| (k.clone(), format!("${{{}}}", k)))
                .collect();
            (name.clone(), "npx".into(), def.args.clone(), env)
        })
        .collect();

    // Report already-present names the same way as the JSON path.
    let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
    for name in servers
        .iter()
        .filter(|(name, _, _, _)| {
            existing.contains(&format!("serverName: {name}"))
                || existing.contains(&format!("serverName: '{name}'"))
        })
        .map(|(name, _, _, _)| name)
    {
        println!("  ⏭  {} (already exists)", name);
    }

    match crate::config::parser::append_dsh_servers(&config_path, &servers) {
        Ok((installed, skipped)) => {
            for (name, _, _, _) in servers.iter().filter(|(name, _, _, _)| {
                !(existing.contains(&format!("serverName: {name}"))
                    || existing.contains(&format!("serverName: '{name}'")))
            }) {
                println!("  ✓  {}", name);
            }
            println!(
                "\nDone! Installed {} server(s), skipped {} (already exist).",
                installed, skipped
            );
            println!("Config written to: {}", config_path.display());
            println!("\nRestart DeepSeek Harness to load the new servers.");
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
        Err(e) => eprintln!("Failed to write config: {}", e),
    }
}
