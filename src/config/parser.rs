use crate::config::model::{ClientType, ServerEntry, ServerStatus};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Deserialize)]
struct RawServerConfig {
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RawConfig {
    #[serde(rename = "mcpServers", default)]
    mcp_servers: HashMap<String, RawServerConfig>,
}

pub fn parse_config(
    json: &str,
    client: ClientType,
    source_path: PathBuf,
) -> Result<Vec<ServerEntry>, serde_json::Error> {
    let raw: RawConfig = serde_json::from_str(json)?;
    let entries = raw
        .mcp_servers
        .into_iter()
        .map(|(name, cfg)| {
            let (command, status) = match cfg.command {
                Some(cmd) => (cmd, ServerStatus::Active),
                None => (
                    String::new(),
                    ServerStatus::ParseError("missing 'command' field".into()),
                ),
            };
            ServerEntry {
                name,
                command,
                args: cfg.args,
                env: cfg.env,
                source_client: client.clone(),
                source_path: source_path.clone(),
                status,
            }
        })
        .collect();
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_claude_desktop_config() {
        let json = r#"{
            "mcpServers": {
                "filesystem": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/user/projects"],
                    "env": {}
                },
                "github": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-github"],
                    "env": {
                        "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_test123"
                    }
                }
            }
        }"#;
        let entries =
            parse_config(json, ClientType::ClaudeDesktop, PathBuf::from("/fake/path")).unwrap();
        assert_eq!(entries.len(), 2);
        // HashMap iteration order is not guaranteed, so find by name
        let fs = entries.iter().find(|e| e.name == "filesystem").unwrap();
        assert_eq!(fs.command, "npx");
        assert_eq!(fs.args.len(), 3);
        let gh = entries.iter().find(|e| e.name == "github").unwrap();
        assert_eq!(
            gh.env.get("GITHUB_PERSONAL_ACCESS_TOKEN").unwrap(),
            "ghp_test123"
        );
    }

    #[test]
    fn test_parse_empty_config() {
        let json = r#"{"mcpServers": {}}"#;
        let entries = parse_config(json, ClientType::Cursor, PathBuf::from("/fake")).unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse_config("not json", ClientType::Cursor, PathBuf::from("/fake"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_command() {
        let json = r#"{"mcpServers": {"bad": {"args": []}}}"#;
        let entries = parse_config(json, ClientType::Cursor, PathBuf::from("/fake")).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].status, ServerStatus::ParseError(_)));
    }
}
