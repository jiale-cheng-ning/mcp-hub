use crate::config::model::ServerEntry;

#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

#[derive(Debug, serde::Serialize)]
pub struct Finding {
    pub rule_id: String,
    pub severity: String,
    pub server_name: String,
    pub message: String,
    pub fix: String,
}

// Check for secrets in env vars
pub fn check_env_secrets(server: &ServerEntry) -> Vec<Finding> {
    let secret_patterns = ["TOKEN", "KEY", "SECRET", "PASSWORD", "API_KEY"];
    let mut findings = Vec::new();

    for (name, value) in &server.env {
        let upper = name.to_uppercase();
        if secret_patterns.iter().any(|p| upper.contains(p)) && !value.is_empty() {
            findings.push(Finding {
                rule_id: "ENV_PLAINTEXT_SECRET".into(),
                severity: "Warning".into(),
                server_name: server.name.clone(),
                message: format!("Potential secret '{}' stored in plaintext config", name),
                fix: "Use environment variable reference or secret manager".into(),
            });
        }
    }
    findings
}

// Check for dangerous filesystem permissions
pub fn check_permissions(server: &ServerEntry) -> Vec<Finding> {
    let mut findings = Vec::new();
    let dangerous_paths = ["/", "C:\\", "C:/", "~"];

    for arg in &server.args {
        let trimmed = arg.trim();
        if dangerous_paths.contains(&trimmed) {
            let (rule, msg) = if trimmed == "/" || trimmed == "C:\\" || trimmed == "C:/" {
                ("PERM_ROOT", "has unrestricted access to root filesystem")
            } else {
                ("PERM_HOME", "has unrestricted access to home directory")
            };
            findings.push(Finding {
                rule_id: rule.into(),
                severity: "Warning".into(),
                server_name: server.name.clone(),
                message: format!("Server '{}'", server.name) + " " + msg,
                fix: "Restrict directory scope with a specific path".into(),
            });
        }
    }
    findings
}

// Check for unpinned package versions
pub fn check_version_pinning(server: &ServerEntry) -> Vec<Finding> {
    let mut findings = Vec::new();

    for arg in &server.args {
        // Match scoped npm packages: @scope/package without @version
        if arg.starts_with('@') {
            if let Some(pos) = arg.find('/') {
                let after_slash = &arg[pos + 1..];
                if !after_slash.contains('@') {
                    findings.push(Finding {
                        rule_id: "NO_VERSION_PIN".into(),
                        severity: "Info".into(),
                        server_name: server.name.clone(),
                        message: format!("Unpinned package version: '{}'", arg),
                        fix: "Pin to a specific version (e.g., @scope/pkg@1.2.0)".into(),
                    });
                }
            }
        }
    }
    findings
}

// Check for duplicate servers across clients
pub fn check_duplicates(servers: &[ServerEntry]) -> Vec<Finding> {
    let mut findings = Vec::new();

    for i in 0..servers.len() {
        for j in (i + 1)..servers.len() {
            let a = &servers[i];
            let b = &servers[j];
            if a.command == b.command && a.args == b.args && a.source_client != b.source_client {
                findings.push(Finding {
                    rule_id: "DUPLICATE_SERVER".into(),
                    severity: "Info".into(),
                    server_name: b.name.clone(),
                    message: format!(
                        "Server '{}' duplicates '{}' (same command in {} and {})",
                        b.name, a.name, b.source_client, a.source_client
                    ),
                    fix: "Consider using a shared configuration or removing the duplicate".into(),
                });
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::config::model::{ClientType, ServerStatus};
    use std::path::PathBuf;

    fn make_server(name: &str, cmd: &str, args: Vec<&str>, env: HashMap<String, String>) -> ServerEntry {
        ServerEntry {
            name: name.to_string(),
            command: cmd.to_string(),
            args: args.into_iter().map(String::from).collect(),
            env,
            source_client: ClientType::ClaudeDesktop,
            source_path: PathBuf::from("/fake"),
            status: ServerStatus::Active,
        }
    }

    #[test]
    fn test_detect_plaintext_secret() {
        let mut env = HashMap::new();
        env.insert("GITHUB_PERSONAL_ACCESS_TOKEN".to_string(), "ghp_abc123".to_string());
        let server = make_server("github", "npx", vec![], env);
        let findings = check_env_secrets(&server);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("GITHUB_PERSONAL_ACCESS_TOKEN"));
    }

    #[test]
    fn test_no_false_positive_on_non_secret_env() {
        let mut env = HashMap::new();
        env.insert("NODE_ENV".to_string(), "production".to_string());
        let server = make_server("test", "npx", vec![], env);
        let findings = check_env_secrets(&server);
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn test_detect_unpinned_version() {
        let server = make_server("github", "npx", vec!["-y", "@modelcontextprotocol/server-github"], HashMap::new());
        let findings = check_version_pinning(&server);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn test_pinned_version_no_finding() {
        let server = make_server("github", "npx", vec!["-y", "@modelcontextprotocol/server-github@1.2.0"], HashMap::new());
        let findings = check_version_pinning(&server);
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn test_detect_root_permission() {
        let server = make_server("filesystem", "npx", vec!["-y", "@modelcontextprotocol/server-filesystem", "/"], HashMap::new());
        let findings = check_permissions(&server);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_detect_duplicate_servers() {
        let servers = vec![
            make_server("github", "npx", vec!["-y", "@modelcontextprotocol/server-github"], HashMap::new()),
            ServerEntry {
                source_client: ClientType::Cursor,
                ..make_server("github-cursor", "npx", vec!["-y", "@modelcontextprotocol/server-github"], HashMap::new())
            },
        ];
        let findings = check_duplicates(&servers);
        assert_eq!(findings.len(), 1);
    }
}
