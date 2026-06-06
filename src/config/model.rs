use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClientType {
    ClaudeDesktop,
    ClaudeCode,
    Cursor,
    VSCode,
    Windsurf,
}

impl std::fmt::Display for ClientType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientType::ClaudeDesktop => write!(f, "Claude Desktop"),
            ClientType::ClaudeCode => write!(f, "Claude Code"),
            ClientType::Cursor => write!(f, "Cursor"),
            ClientType::VSCode => write!(f, "VS Code"),
            ClientType::Windsurf => write!(f, "Windsurf"),
        }
    }
}

impl ClientType {
    pub fn from_str_loose(s: &str) -> Option<ClientType> {
        match s.to_lowercase().as_str() {
            "claude desktop" | "claudedesktop" | "claude-desktop" => {
                Some(ClientType::ClaudeDesktop)
            }
            "claude code" | "claudecode" | "claude-code" => Some(ClientType::ClaudeCode),
            "cursor" => Some(ClientType::Cursor),
            "vscode" | "vs code" | "vs-code" | "visual studio code" => Some(ClientType::VSCode),
            "windsurf" => Some(ClientType::Windsurf),
            _ => None,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub enum ServerStatus {
    Active,
    Disabled,
    ParseError(String),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct ServerEntry {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub source_client: ClientType,
    pub source_path: PathBuf,
    pub status: ServerStatus,
}

// ─── Export/Import data model ───

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportData {
    pub version: u32,
    pub exported_at: String,
    pub servers: HashMap<String, ExportedServer>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportedServer {
    pub command: String,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

impl ExportData {
    pub fn from_servers(servers: &[ServerEntry]) -> Self {
        let mut map: HashMap<String, ExportedServer> = HashMap::new();
        for server in servers {
            let redacted_env = server
                .env
                .iter()
                .map(|(k, v)| {
                    if has_secret_keyword(k) && !v.is_empty() {
                        (k.clone(), format!("${{{}}}", k))
                    } else {
                        (k.clone(), v.clone())
                    }
                })
                .collect();
            // Deduplicate: keep first occurrence
            map.entry(server.name.clone())
                .or_insert_with(|| ExportedServer {
                    command: server.command.clone(),
                    args: server.args.clone(),
                    env: redacted_env,
                });
        }
        Self {
            version: 1,
            exported_at: chrono_now(),
            servers: map,
        }
    }

    pub fn to_config_json(&self) -> serde_json::Value {
        let mut root = serde_json::Map::new();
        let mut mcp_servers = serde_json::Map::new();
        for (name, server) in &self.servers {
            let mut entry = serde_json::Map::new();
            entry.insert(
                "command".into(),
                serde_json::Value::String(server.command.clone()),
            );
            entry.insert(
                "args".into(),
                serde_json::Value::Array(
                    server
                        .args
                        .iter()
                        .map(|a| serde_json::Value::String(a.clone()))
                        .collect(),
                ),
            );
            if !server.env.is_empty() {
                let env_map: serde_json::Map<String, serde_json::Value> = server
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                entry.insert("env".into(), serde_json::Value::Object(env_map));
            }
            mcp_servers.insert(name.clone(), serde_json::Value::Object(entry));
        }
        root.insert("mcpServers".into(), serde_json::Value::Object(mcp_servers));
        serde_json::Value::Object(root)
    }
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

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_server(
        name: &str,
        cmd: &str,
        args: Vec<&str>,
        env: HashMap<String, String>,
    ) -> ServerEntry {
        ServerEntry {
            name: name.into(),
            command: cmd.into(),
            args: args.into_iter().map(String::from).collect(),
            env,
            source_client: ClientType::ClaudeDesktop,
            source_path: PathBuf::from("/fake/config.json"),
            status: ServerStatus::Active,
        }
    }

    #[test]
    fn test_export_redacts_secrets() {
        let mut env = HashMap::new();
        env.insert("GITHUB_TOKEN".into(), "ghp_secret123".into());
        env.insert("NODE_ENV".into(), "production".into());
        let servers = vec![make_server("github", "npx", vec!["-y", "pkg"], env)];

        let export = ExportData::from_servers(&servers);
        let server = export.servers.get("github").unwrap();
        assert_eq!(server.env.get("GITHUB_TOKEN").unwrap(), "${GITHUB_TOKEN}");
        assert_eq!(server.env.get("NODE_ENV").unwrap(), "production");
    }

    #[test]
    fn test_export_deduplicates() {
        let servers = vec![
            make_server("github", "npx", vec!["-y", "pkg"], HashMap::new()),
            make_server("github", "npx", vec!["-y", "pkg"], HashMap::new()),
        ];
        let export = ExportData::from_servers(&servers);
        assert_eq!(export.servers.len(), 1);
    }

    #[test]
    fn test_export_to_config_json_roundtrip() {
        let mut env = HashMap::new();
        env.insert("API_KEY".into(), "${API_KEY}".into());
        let servers = vec![make_server("test", "npx", vec!["-y", "pkg"], env)];

        let export = ExportData::from_servers(&servers);
        let json = export.to_config_json();
        let mcp = json.get("mcpServers").unwrap();
        let test_server = mcp.get("test").unwrap();
        assert_eq!(test_server.get("command").unwrap().as_str().unwrap(), "npx");
    }

    #[test]
    fn test_export_serializes_and_deserializes() {
        let servers = vec![make_server("gh", "npx", vec!["-y", "pkg"], HashMap::new())];
        let export = ExportData::from_servers(&servers);
        let json_str = serde_json::to_string(&export).unwrap();
        let parsed: ExportData = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed.version, 1);
        assert!(parsed.servers.contains_key("gh"));
    }

    #[test]
    fn test_client_type_from_str_loose() {
        assert_eq!(
            ClientType::from_str_loose("cursor"),
            Some(ClientType::Cursor)
        );
        assert_eq!(
            ClientType::from_str_loose("Claude Desktop"),
            Some(ClientType::ClaudeDesktop)
        );
        assert_eq!(
            ClientType::from_str_loose("claude-code"),
            Some(ClientType::ClaudeCode)
        );
        assert_eq!(ClientType::from_str_loose("unknown"), None);
    }
}
