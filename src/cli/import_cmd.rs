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

    let mut export: ExportData = match serde_json::from_str(&content) {
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

    if let Err(error) = export.resolve_references() {
        eprintln!(
            "Import failed: {}. Supply credentials using the named environment variables.",
            error
        );
        return;
    }
    match crate::config::write::merge(&config_path, &client, &export, false) {
        Ok((added, skipped)) => {
            println!("Imported {} server(s), {} existing skipped", added, skipped)
        }
        Err(error) => eprintln!("Import failed: {}", error),
    }
}
