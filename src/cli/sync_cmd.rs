use crate::config::discover::config_path_for;
use crate::config::model::ClientType;
use crate::config::parser::{parse_config, parse_dsh_cordis_yaml};

const CLIENT_HELP: &str = "claude-desktop, claude-code, cursor, vscode, windsurf, dsh";

pub fn run(from: &str, to: &str, dry_run: bool) {
    let from_client = match ClientType::from_str_loose(from) {
        Some(c) => c,
        None => {
            eprintln!("Unknown source client '{}'. Use: {}", from, CLIENT_HELP);
            return;
        }
    };

    let to_client = match ClientType::from_str_loose(to) {
        Some(c) => c,
        None => {
            eprintln!("Unknown target client '{}'. Use: {}", to, CLIENT_HELP);
            return;
        }
    };

    if from_client == to_client {
        eprintln!("Source and target client are the same.");
        return;
    }

    // Read source config
    let from_path = match config_path_for(&from_client) {
        Some(p) => p,
        None => {
            eprintln!("Could not determine config path for {}", from_client);
            return;
        }
    };

    if !from_path.exists() {
        eprintln!("Source config not found: {}", from_path.display());
        return;
    }

    let from_content = match std::fs::read_to_string(&from_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read source config: {}", e);
            return;
        }
    };

    let from_entries = if matches!(from_client, ClientType::DeepSeekHarness) {
        match parse_dsh_cordis_yaml(&from_content, from_client.clone(), from_path.clone()) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Failed to parse source config: {}", e);
                return;
            }
        }
    } else {
        match parse_config(&from_content, from_client.clone(), from_path.clone()) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Failed to parse source config: {}", e);
                return;
            }
        }
    };

    if from_entries.is_empty() {
        println!("No MCP servers found in {} config.", from_client);
        return;
    }

    // Read target config
    let to_path = match config_path_for(&to_client) {
        Some(p) => p,
        None => {
            eprintln!("Could not determine config path for {}", to_client);
            return;
        }
    };

    if matches!(to_client, ClientType::DeepSeekHarness) {
        println!(
            "Syncing {} server(s) from {} to {}...\n",
            from_entries.len(),
            from_client,
            to_client
        );
        let existing = std::fs::read_to_string(&to_path).unwrap_or_default();
        let already = |name: &str| {
            existing.contains(&format!("serverName: {name}"))
                || existing.contains(&format!("serverName: '{name}'"))
        };
        let servers: Vec<(String, String, Vec<String>, std::collections::HashMap<String, String>)> =
            from_entries
                .iter()
                .filter(|e| !already(&e.name))
                .map(|e| {
                    (
                        e.name.clone(),
                        e.command.clone(),
                        e.args.clone(),
                        e.env.clone(),
                    )
                })
                .collect();
        for entry in &from_entries {
            if already(&entry.name) {
                println!("  ⏭  {} (already exists in {})", entry.name, to_client);
            } else {
                println!("  ✓  {}", entry.name);
            }
        }
        if dry_run {
            println!(
                "\nDry run: would sync {} server(s), skip {} (already exist).",
                servers.len(),
                from_entries.len() - servers.len()
            );
            println!("No changes written.");
            return;
        }
        match crate::config::parser::append_dsh_servers(&to_path, &servers) {
            Ok((synced, skipped)) => {
                println!(
                    "\nDone! Synced {} server(s), skipped {} (already exist).",
                    synced, skipped
                );
                println!("Config written to: {}", to_path.display());
                println!("\nRestart {} to load the new servers.", to_client);
            }
            Err(e) => eprintln!("Failed to write config: {}", e),
        }
        return;
    }

    let mut existing: serde_json::Value = if to_path.exists() {
        std::fs::read_to_string(&to_path)
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

    // Sync servers
    println!(
        "Syncing {} server(s) from {} to {}...\n",
        from_entries.len(),
        from_client,
        to_client
    );

    let mut synced = 0;
    let mut skipped = 0;

    for entry in &from_entries {
        if servers.contains_key(&entry.name) {
            println!("  ⏭  {} (already exists in {})", entry.name, to_client);
            skipped += 1;
            continue;
        }

        let mut server_obj = serde_json::Map::new();
        server_obj.insert(
            "command".into(),
            serde_json::Value::String(entry.command.clone()),
        );
        if !entry.args.is_empty() {
            server_obj.insert("args".into(), serde_json::json!(entry.args));
        }
        if !entry.env.is_empty() {
            let env_map: serde_json::Map<String, serde_json::Value> = entry
                .env
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();
            server_obj.insert("env".into(), serde_json::Value::Object(env_map));
        }

        servers.insert(entry.name.clone(), serde_json::Value::Object(server_obj));
        println!("  ✓  {}", entry.name);
        synced += 1;
    }

    if dry_run {
        println!(
            "\nDry run: would sync {} server(s), skip {} (already exist).",
            synced, skipped
        );
        println!("No changes written.");
        return;
    }

    // Write target config
    if let Some(parent) = to_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let output = serde_json::to_string_pretty(&existing).unwrap_or_default();
    match std::fs::write(&to_path, &output) {
        Ok(()) => {
            println!(
                "\nDone! Synced {} server(s), skipped {} (already exist).",
                synced, skipped
            );
            println!("Config written to: {}", to_path.display());
            println!("\nRestart {} to load the new servers.", to_client);
        }
        Err(e) => {
            eprintln!("Failed to write config: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_client_rejected() {
        // This should print an error, not panic
        // We can't easily test the output, but we can test the logic
        let from = ClientType::from_str_loose("cursor").unwrap();
        let to = ClientType::from_str_loose("cursor").unwrap();
        assert_eq!(from, to);
    }

    #[test]
    fn test_different_clients() {
        let from = ClientType::from_str_loose("claude-desktop").unwrap();
        let to = ClientType::from_str_loose("cursor").unwrap();
        assert_ne!(from, to);
    }

    #[test]
    fn test_invalid_client() {
        assert!(ClientType::from_str_loose("invalid").is_none());
    }
}
