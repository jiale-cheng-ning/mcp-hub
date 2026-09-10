use crate::config::model::{ClientType, ServerEntry};
use crate::config::parser::parse_config;
use std::path::PathBuf;

/// Return the config file path for a given client type.
pub fn config_path_for(client: &ClientType) -> Option<PathBuf> {
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

pub fn known_config_paths() -> Vec<(ClientType, PathBuf)> {
    let clients = [
        ClientType::ClaudeDesktop,
        ClientType::ClaudeCode,
        ClientType::Cursor,
        ClientType::VSCode,
        ClientType::Windsurf,
    ];
    clients
        .iter()
        .filter_map(|c| config_path_for(c).map(|p| (c.clone(), p)))
        .collect()
}

pub fn discover() -> Result<Vec<ServerEntry>, Box<dyn std::error::Error>> {
    let mut all_entries = Vec::new();

    for (client, path) in known_config_paths() {
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            match parse_config(&content, client, path.clone()) {
                Ok(entries) => all_entries.extend(entries),
                Err(e) => {
                    eprintln!("Warning: failed to parse {}: {}", path.display(), e);
                }
            }
        }
    }

    Ok(all_entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_paths_returns_entries() {
        let paths = known_config_paths();
        assert!(paths.len() >= 3);
        for (client, path) in &paths {
            assert!(!path.as_os_str().is_empty());
            let _ = format!("{}", client);
        }
    }

    #[test]
    fn test_discover_returns_vec() {
        let result = discover();
        assert!(result.is_ok());
    }
}
