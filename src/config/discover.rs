use crate::config::model::{ClientType, ServerEntry};
use crate::config::parser::parse_config;
use std::path::PathBuf;

#[allow(dead_code)]
pub fn known_config_paths() -> Vec<(ClientType, PathBuf)> {
    let mut paths = Vec::new();

    if let Some(app_data) = dirs::config_dir() {
        paths.push((
            ClientType::ClaudeDesktop,
            app_data.join("Claude").join("claude_desktop_config.json"),
        ));
    }

    if let Some(home) = dirs::home_dir() {
        paths.push((ClientType::ClaudeCode, home.join(".claude").join("settings.json")));
        paths.push((ClientType::Cursor, home.join(".cursor").join("mcp.json")));
        paths.push((
            ClientType::Windsurf,
            home.join(".codeium").join("windsurf").join("mcp_config.json"),
        ));
    }

    paths
}

#[allow(dead_code)]
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
