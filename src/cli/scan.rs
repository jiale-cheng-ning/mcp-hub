use crate::config::discover::discover;
use crate::config::model::ServerEntry;

pub fn run(client_filter: Option<&str>, json: bool) {
    let entries = discover().unwrap_or_else(|e| {
        eprintln!("Error discovering configs: {}", e);
        Vec::new()
    });

    let filtered: Vec<&ServerEntry> = entries
        .iter()
        .filter(|e| {
            if let Some(filter) = client_filter {
                format!("{}", e.source_client)
                    .to_lowercase()
                    .contains(&filter.to_lowercase())
            } else {
                true
            }
        })
        .collect();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&filtered).unwrap_or_default()
        );
        return;
    }

    if filtered.is_empty() {
        println!("No MCP servers discovered. Check that Claude Desktop, Cursor, or other clients are configured.");
        return;
    }

    println!("{:<20} {:<20} {:<40} Status", "Name", "Client", "Command");
    println!("{}", "-".repeat(90));
    for entry in &filtered {
        let status = match &entry.status {
            crate::config::model::ServerStatus::Active => "✅ Active",
            crate::config::model::ServerStatus::Disabled => "⏸  Disabled",
            crate::config::model::ServerStatus::ParseError(_) => "❌ Error",
        };
        println!(
            "{:<20} {:<20} {:<40} {}",
            entry.name,
            format!("{}", entry.source_client),
            entry.command,
            status
        );
    }
    println!("\n{} server(s) found", filtered.len());
}
