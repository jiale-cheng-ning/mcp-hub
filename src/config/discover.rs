use crate::config::model::{ClientType, ServerEntry};
use crate::config::parser::{parse_config, parse_dsh_cordis_yaml};
use std::path::PathBuf;

/// Resolve DeepSeek Harness home: `$DSH_HOME` or `~/.dsh`.
pub fn dsh_home() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("DSH_HOME") {
        if !home.trim().is_empty() {
            return Some(PathBuf::from(home));
        }
    }
    dirs::home_dir().map(|p| p.join(".dsh"))
}

/// Primary user-level DSH config (used for install/write paths).
pub fn dsh_primary_config_path() -> Option<PathBuf> {
    dsh_home().map(|p| p.join("cordis.patch.yml"))
}

/// All known DSH Cordis patch files (global + every profile).
pub fn dsh_config_paths() -> Vec<PathBuf> {
    let Some(home) = dsh_home() else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    let global = home.join("cordis.patch.yml");
    if global.exists() {
        paths.push(global);
    }
    let profiles = home.join("profiles");
    if profiles.is_dir() {
        if let Ok(read) = std::fs::read_dir(&profiles) {
            for entry in read.flatten() {
                let patch = entry.path().join("cordis.patch.yml");
                if patch.is_file() {
                    paths.push(patch);
                }
            }
        }
    }
    paths
}

/// Return the config file path for a given client type.
pub fn config_path_for(client: &ClientType) -> Option<PathBuf> {
    match client {
        ClientType::ClaudeDesktop => {
            dirs::config_dir().map(|p| p.join("Claude").join("claude_desktop_config.json"))
        }
        ClientType::ClaudeCode => dirs::home_dir().map(|p| p.join(".claude.json")),
        ClientType::Cursor => dirs::home_dir().map(|p| p.join(".cursor").join("mcp.json")),
        ClientType::VSCode => {
            dirs::config_dir().map(|p| p.join("Code").join("User").join("mcp.json"))
        }
        ClientType::Windsurf => {
            dirs::home_dir().map(|p| p.join(".codeium").join("windsurf").join("mcp_config.json"))
        }
        ClientType::DeepSeekHarness => dsh_primary_config_path(),
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
    let mut paths: Vec<(ClientType, PathBuf)> = clients
        .iter()
        .filter_map(|c| config_path_for(c).map(|p| (c.clone(), p)))
        .collect();
    if let Ok(cwd) = std::env::current_dir() {
        for (client, relative) in [
            (ClientType::VSCode, ".vscode/mcp.json"),
            (ClientType::ClaudeCode, ".mcp.json"),
        ] {
            let path = cwd.join(relative);
            if path.exists() {
                paths.push((client, path));
            }
        }
    }
    for p in dsh_config_paths() {
        paths.push((ClientType::DeepSeekHarness, p));
    }
    paths
}

/// Diagnostics omit parser messages, which may contain credentials.
#[derive(Debug, serde::Serialize)]
pub struct DiscoveryWarning {
    pub client: String,
    pub code: &'static str,
}

pub fn discover_with_warnings(
    filter: Option<&ClientType>,
) -> (Vec<ServerEntry>, Vec<DiscoveryWarning>) {
    let mut all_entries = Vec::new();
    let mut warnings = Vec::new();

    for (client, path) in known_config_paths() {
        if filter.is_some_and(|selected| selected != &client) {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                warnings.push(DiscoveryWarning {
                    client: client.to_string(),
                    code: "unreadable_config",
                });
                continue;
            }
        };
        let parsed = if matches!(client, ClientType::DeepSeekHarness) {
            parse_dsh_cordis_yaml(&content, client.clone(), path.clone()).map_err(|e| e.to_string())
        } else {
            parse_config(&content, client.clone(), path.clone()).map_err(|e| e.to_string())
        };
        match parsed {
            Ok(entries) => all_entries.extend(entries),
            Err(_) => {
                warnings.push(DiscoveryWarning {
                    client: client.to_string(),
                    code: "invalid_config",
                });
            }
        }
    }

    (all_entries, warnings)
}

pub fn discover() -> Result<Vec<ServerEntry>, Box<dyn std::error::Error>> {
    let (entries, warnings) = discover_with_warnings(None);
    for warning in warnings {
        eprintln!("Warning: {}: {}", warning.client, warning.code);
    }
    Ok(entries)
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

    #[test]
    fn test_dsh_home_honors_env() {
        let prev = std::env::var("DSH_HOME").ok();
        std::env::set_var("DSH_HOME", "/tmp/fake-dsh-home");
        assert_eq!(dsh_home(), Some(PathBuf::from("/tmp/fake-dsh-home")));
        match prev {
            Some(v) => std::env::set_var("DSH_HOME", v),
            None => std::env::remove_var("DSH_HOME"),
        }
    }

    #[test]
    fn test_config_path_for_dsh_is_under_home() {
        let path = config_path_for(&ClientType::DeepSeekHarness).unwrap();
        assert!(path.ends_with("cordis.patch.yml"));
    }
}
