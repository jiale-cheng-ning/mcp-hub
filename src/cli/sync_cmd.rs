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

    if from_entries
        .iter()
        .any(|s| !matches!(s.status, crate::config::model::ServerStatus::Active))
    {
        eprintln!("Source contains disabled/invalid entries; refusing to migrate them");
        return;
    }
    let Some(to_path) = config_path_for(&to_client) else {
        eprintln!("Missing destination path");
        return;
    };
    let export = crate::config::model::ExportData::for_sync(&from_entries);
    match crate::config::write::merge(&to_path, &to_client, &export, dry_run) {
        Ok((added, skipped)) => println!(
            "{}: {} server(s), {} existing skipped",
            if dry_run { "Dry run" } else { "Synced" },
            added,
            skipped
        ),
        Err(error) => eprintln!("Sync failed: {}", error),
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
