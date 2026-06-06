use crate::config::discover::discover;
use crate::config::model::{ExportData, ServerEntry};

pub fn run(output: Option<&str>) {
    let entries = discover().unwrap_or_else(|e| {
        eprintln!("Warning: {}", e);
        Vec::new()
    });

    if entries.is_empty() {
        println!("No MCP servers found to export.");
        return;
    }

    let export = ExportData::from_servers(&entries);

    let json = serde_json::to_string_pretty(&export).unwrap_or_default();

    let path = output.unwrap_or("mcp-hub.json");
    match std::fs::write(path, &json) {
        Ok(()) => {
            println!("Exported {} server(s) to {}", export.servers.len(), path);

            // Count redacted secrets
            let secret_count = count_secrets(&entries);
            if secret_count > 0 {
                println!(
                    "Note: {} secret(s) were replaced with ${{VAR_NAME}} references",
                    secret_count
                );
            }
            println!("\nImport with: mcp-hub import {}", path);
        }
        Err(e) => {
            eprintln!("Failed to write {}: {}", path, e);
        }
    }
}

fn count_secrets(servers: &[ServerEntry]) -> usize {
    servers
        .iter()
        .flat_map(|s| s.env.iter())
        .filter(|(k, _)| has_secret_keyword(k))
        .count()
}

fn has_secret_keyword(name: &str) -> bool {
    let upper = name.to_uppercase();
    [
        "TOKEN",
        "KEY",
        "SECRET",
        "PASSWORD",
        "API_KEY",
        "ACCESS_KEY",
        "PRIVATE_KEY",
    ]
    .iter()
    .any(|k| upper.contains(k))
}
