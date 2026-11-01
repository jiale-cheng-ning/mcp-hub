use crate::config::discover::config_path_for;
use crate::config::model::{ClientType, ExportData};

const CLIENT_HELP: &str = "claude-desktop, claude-code, cursor, vscode, windsurf, dsh";

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
                eprintln!("Unknown client '{}'. Use: {}", t, CLIENT_HELP);
                return;
            }
        },
        None => {
            // Default: prompt or pick first available
            println!("No --target specified. Available clients:");
            println!("  {}", CLIENT_HELP);
            println!("\nUsage: mcp-hub import {} --target <client>", file);
            return;
        }
    };

    // Get config path for target client
    let config_path = config_path_for(&client);
    let config_path = match config_path {
        Some(p) => p,
        None => {
            eprintln!("Could not determine config path for {}", client);
            return;
        }
    };

    if matches!(client, ClientType::DeepSeekHarness) {
        let servers: Vec<crate::config::parser::DshServerSpec> = export
            .servers
            .iter()
            .map(|(name, s)| {
                (
                    name.clone(),
                    s.command.clone(),
                    s.args.clone(),
                    s.env.clone(),
                )
            })
            .collect();
        match crate::config::parser::append_dsh_servers(&config_path, &servers) {
            Ok((imported, skipped)) => {
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
            Err(e) => eprintln!("Failed to write {}: {}", config_path.display(), e),
        }
        return;
    }

    // Read existing config or create empty
    let mut existing = match crate::config::safe_io::read_json_config(&config_path) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("Refusing to overwrite invalid or unreadable config: {}", e);
            return;
        }
    };

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
    match crate::config::safe_io::write_config_atomic(&config_path, &output) {
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
