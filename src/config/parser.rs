use crate::config::model::{ClientType, ServerEntry, ServerStatus};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// (name, command, args, env) for a stdio MCP server written to DSH YAML.
pub type DshServerSpec = (String, String, Vec<String>, HashMap<String, String>);

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

/// Strip Cordis `!!js` / custom YAML tags so expressions become plain strings.
/// Needed because DSH configs use `!!js process.env.FOO` style values.
fn strip_yaml_tags(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    for line in content.lines() {
        if let Some(idx) = line.find("!!") {
            let before = &line[..idx];
            let after = &line[idx..];
            // Drop the tag token; keep the rest of the scalar as a plain string.
            let rest = match after.find(char::is_whitespace) {
                Some(sp) => {
                    let value = after[sp..].trim_start();
                    if value.is_empty()
                        || value == ">-"
                        || value == ">"
                        || value == "|"
                        || value == "|-"
                    {
                        after[sp..].to_string()
                    } else {
                        format!(" {}", value)
                    }
                }
                None => String::new(),
            };
            out.push_str(before.trim_end());
            out.push_str(&rest);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn yaml_string(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Null => None,
        _ => None,
    }
}

fn yaml_args(v: &serde_yaml::Value) -> Vec<String> {
    match v {
        serde_yaml::Value::Sequence(seq) => seq.iter().filter_map(yaml_string).collect(),
        _ => Vec::new(),
    }
}

fn yaml_env(v: &serde_yaml::Value) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let serde_yaml::Value::Mapping(m) = v {
        for (k, val) in m {
            if let (Some(key), Some(s)) = (yaml_string(k), yaml_string(val)) {
                map.insert(key, s);
            }
        }
    }
    map
}

/// Walk a Cordis patch / config document and collect `@deepseek-ai/dsh-mcp-client` entries.
fn collect_dsh_mcp_entries(
    node: &serde_yaml::Value,
    client: &ClientType,
    source_path: &PathBuf,
    entries: &mut Vec<ServerEntry>,
) {
    match node {
        serde_yaml::Value::Sequence(seq) => {
            for item in seq {
                collect_dsh_mcp_entries(item, client, source_path, entries);
            }
        }
        serde_yaml::Value::Mapping(map) => {
            // Patch op: { insert: [ plugins... ] } / { patch: [ plugins... ] }
            for key in ["insert", "patch", "plugins", "config"] {
                if let Some(serde_yaml::Value::Sequence(list)) =
                    map.get(serde_yaml::Value::String(key.to_string()))
                {
                    for plugin in list {
                        collect_dsh_mcp_entries(plugin, client, source_path, entries);
                    }
                }
            }

            // Plugin row: { name: '@deepseek-ai/dsh-mcp-client', config: { ... } }
            let name = map
                .get(serde_yaml::Value::String("name".into()))
                .and_then(yaml_string)
                .unwrap_or_default();
            if !name.contains("dsh-mcp-client") {
                return;
            }
            let Some(config) = map.get(serde_yaml::Value::String("config".into())) else {
                return;
            };
            let Some(cfg_map) = config.as_mapping() else {
                return;
            };
            let get = |k: &str| {
                cfg_map
                    .get(serde_yaml::Value::String(k.to_string()))
                    .cloned()
                    .unwrap_or(serde_yaml::Value::Null)
            };

            let server_name = get("serverName")
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    map.get(serde_yaml::Value::String("id".into()))
                        .and_then(yaml_string)
                        .unwrap_or_else(|| "dsh-mcp".into())
                });

            let transport = get("transport").as_str().unwrap_or("stdio").to_string();
            let (command, args, env, status) = if transport == "stdio" || get("command").is_string()
            {
                let command = yaml_string(&get("command")).unwrap_or_default();
                let status = if command.is_empty() {
                    ServerStatus::ParseError("missing 'command' field".into())
                } else {
                    ServerStatus::Active
                };
                (
                    command,
                    yaml_args(&get("args")),
                    yaml_env(&get("env")),
                    status,
                )
            } else {
                // streamable-http: surface URL as command so scan/audit still see something
                let url = yaml_string(&get("url")).unwrap_or_default();
                let status = if url.is_empty() {
                    ServerStatus::ParseError("missing 'url' field".into())
                } else {
                    ServerStatus::Active
                };
                (url, Vec::new(), yaml_env(&get("headers")), status)
            };

            entries.push(ServerEntry {
                name: server_name,
                command,
                args,
                env,
                source_client: client.clone(),
                source_path: source_path.clone(),
                status,
            });
        }
        _ => {}
    }
}

/// Parse DSH `$DSH_HOME/cordis.patch.yml` (and profile patches).
pub fn parse_dsh_cordis_yaml(
    content: &str,
    client: ClientType,
    source_path: PathBuf,
) -> Result<Vec<ServerEntry>, serde_yaml::Error> {
    let cleaned = strip_yaml_tags(content);
    let root: serde_yaml::Value = serde_yaml::from_str(&cleaned)?;
    let mut entries = Vec::new();
    collect_dsh_mcp_entries(&root, &client, &source_path, &mut entries);
    Ok(entries)
}

/// Quote a scalar for Cordis/YAML output (single quotes, escape `'` as `''`).
fn yaml_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Serialize servers into a Cordis patch YAML insert block for DSH.
pub fn dsh_cordis_yaml_from_servers(servers: &[DshServerSpec]) -> String {
    let mut out = String::from("# Managed by mcp-hub — DeepSeek Harness MCP servers\n- insert:\n");
    for (name, command, args, env) in servers {
        out.push_str(&format!(
            "    - id: {}\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        serverName: {}\n        transport: stdio\n        command: {}\n",
            yaml_quote(&format!("mcp-{name}")),
            yaml_quote(name),
            yaml_quote(command)
        ));
        if !args.is_empty() {
            out.push_str("        args:\n");
            for a in args {
                out.push_str(&format!("          - {}\n", yaml_quote(a)));
            }
        }
        if !env.is_empty() {
            out.push_str("        env:\n");
            for (k, v) in env {
                out.push_str(&format!("          {k}: {}\n", yaml_quote(v)));
            }
        }
    }
    out
}

/// Append stdio MCP servers into a DSH `cordis.patch.yml`.
/// Returns `(installed, skipped)` counts.
pub fn append_dsh_servers(
    path: &std::path::Path,
    servers: &[DshServerSpec],
) -> std::io::Result<(usize, usize)> {
    let existing = crate::config::safe_io::read_optional(path)?.unwrap_or_default();
    if !existing.trim().is_empty() {
        let root: serde_yaml::Value =
            serde_yaml::from_str(&strip_yaml_tags(&existing)).map_err(std::io::Error::other)?;
        if !root.is_sequence() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "DSH config root must be a sequence",
            ));
        }
    }
    let mut skipped = 0;
    let mut pending: Vec<DshServerSpec> = Vec::new();

    for (name, command, args, env) in servers {
        if existing.contains(&format!("serverName: {name}"))
            || existing.contains(&format!("serverName: '{name}'"))
        {
            skipped += 1;
            continue;
        }
        pending.push((name.clone(), command.clone(), args.clone(), env.clone()));
    }

    if pending.is_empty() {
        return Ok((0, skipped));
    }

    let snippet = dsh_cordis_yaml_from_servers(&pending);
    let body: String = snippet
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.starts_with("- insert:"))
        .map(|l| format!("{l}\n"))
        .collect();

    let mut content = existing.trim_end().to_string();
    if content.is_empty() {
        content = format!("- insert:\n{body}");
    } else if content.contains("- insert:") {
        content.push('\n');
        content.push_str(&body);
    } else {
        content.push('\n');
        content.push_str("- insert:\n");
        content.push_str(&body);
    }
    if !content.ends_with('\n') {
        content.push('\n');
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _: serde_yaml::Value =
        serde_yaml::from_str(&strip_yaml_tags(&content)).map_err(std::io::Error::other)?;
    crate::config::safe_io::write_config_atomic(path, &content)?;
    Ok((pending.len(), skipped))
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

    #[test]
    fn test_parse_dsh_cordis_yaml_stdio() {
        let yaml = r#"
- insert:
    - id: mcp-github
      name: '@deepseek-ai/dsh-mcp-client'
      config:
        serverName: github
        transport: stdio
        command: npx
        args:
          - -y
          - '@modelcontextprotocol/server-github'
        env:
          GITHUB_TOKEN: token123
    - id: other-plugin
      name: '@deepseek-ai/dsh-base'
      config: {}
"#;
        let entries = parse_dsh_cordis_yaml(
            yaml,
            ClientType::DeepSeekHarness,
            PathBuf::from("/fake/cordis.patch.yml"),
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "github");
        assert_eq!(entries[0].command, "npx");
        assert_eq!(
            entries[0].args,
            vec!["-y", "@modelcontextprotocol/server-github"]
        );
        assert_eq!(entries[0].env.get("GITHUB_TOKEN").unwrap(), "token123");
        assert!(matches!(entries[0].status, ServerStatus::Active));
    }

    #[test]
    fn test_parse_dsh_cordis_yaml_strips_js_tags() {
        let yaml = r#"
- insert:
    - id: mcp-gh
      name: '@deepseek-ai/dsh-mcp-client'
      config:
        serverName: gh
        transport: stdio
        command: npx
        env:
          GITHUB_TOKEN: !!js process.env.GITHUB_TOKEN
"#;
        let entries = parse_dsh_cordis_yaml(
            yaml,
            ClientType::DeepSeekHarness,
            PathBuf::from("/fake/cordis.patch.yml"),
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].command, "npx");
        assert_eq!(
            entries[0].env.get("GITHUB_TOKEN").unwrap(),
            "process.env.GITHUB_TOKEN"
        );
    }

    #[test]
    fn test_parse_dsh_http_transport() {
        let yaml = r#"
- id: mcp-web
  name: '@deepseek-ai/dsh-mcp-client'
  config:
    serverName: web
    transport: streamable-http
    url: http://localhost:3000/mcp
"#;
        let entries = parse_dsh_cordis_yaml(
            yaml,
            ClientType::DeepSeekHarness,
            PathBuf::from("/fake/cordis.patch.yml"),
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "web");
        assert_eq!(entries[0].command, "http://localhost:3000/mcp");
    }

    #[test]
    fn test_dsh_cordis_yaml_roundtrip_shape() {
        let mut env = HashMap::new();
        env.insert("GITHUB_TOKEN".into(), "token".into());
        let yaml = dsh_cordis_yaml_from_servers(&[(
            "github".into(),
            "npx".into(),
            vec!["-y".into(), "@modelcontextprotocol/server-github".into()],
            env,
        )]);
        assert!(yaml.contains("serverName: 'github'"));
        assert!(yaml.contains("command: 'npx'"));
        let entries = parse_dsh_cordis_yaml(
            &yaml,
            ClientType::DeepSeekHarness,
            PathBuf::from("/fake/cordis.patch.yml"),
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "github");
    }

    #[test]
    fn test_append_dsh_servers_to_temp_file() {
        let dir = std::env::temp_dir().join(format!("mcp-hub-dsh-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cordis.patch.yml");

        let mut env = HashMap::new();
        env.insert("TOKEN".into(), "abc".into());
        let (installed, skipped) = append_dsh_servers(
            &path,
            &[
                (
                    "github".into(),
                    "npx".into(),
                    vec!["-y".into()],
                    env.clone(),
                ),
                ("fs".into(), "npx".into(), vec!["-y".into()], env.clone()),
            ],
        )
        .unwrap();
        assert_eq!((installed, skipped), (2, 0));

        let (installed2, skipped2) = append_dsh_servers(
            &path,
            &[("github".into(), "npx".into(), vec!["-y".into()], env)],
        )
        .unwrap();
        assert_eq!((installed2, skipped2), (0, 1));

        let entries = parse_dsh_cordis_yaml(
            &std::fs::read_to_string(&path).unwrap(),
            ClientType::DeepSeekHarness,
            path.clone(),
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
