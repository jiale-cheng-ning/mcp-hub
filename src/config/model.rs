use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, PartialEq)]
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
