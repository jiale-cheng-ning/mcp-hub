use super::model::{ClientType, ExportData};
use super::{parser, safe_io};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

pub fn bucket(client: &ClientType) -> &'static str {
    if matches!(client, ClientType::VSCode) {
        "servers"
    } else {
        "mcpServers"
    }
}

/// Validate and prepare everything before the single atomic write. Dry run is read-only.
pub fn merge(
    path: &Path,
    client: &ClientType,
    export: &ExportData,
    dry_run: bool,
) -> Result<(usize, usize), String> {
    let incoming = export.config_for(client)?;
    let values = incoming[bucket(client)]
        .as_object()
        .ok_or("Invalid incoming server collection")?;
    let original = safe_io::read_optional(path).map_err(|_| "Unable to read destination config")?;
    if matches!(client, ClientType::DeepSeekHarness) {
        let text = original.unwrap_or_default();
        if !text.trim().is_empty() {
            let root: serde_yaml::Value =
                serde_yaml::from_str(&text).map_err(|_| "Invalid destination YAML")?;
            if !root.is_sequence() {
                return Err("DSH config must be a YAML sequence".into());
            }
        }
        let entries = parser::parse_dsh_cordis_yaml(&text, client.clone(), path.into())
            .map_err(|_| "Invalid destination YAML")?;
        let names: HashSet<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
        let mut plugins = Vec::new();
        for (name, value) in values {
            if names.contains(name.as_str()) {
                continue;
            }
            let mut cfg = value.clone();
            let kind = cfg
                .as_object_mut()
                .unwrap()
                .remove("type")
                .unwrap_or("stdio".into());
            cfg["transport"] = if kind == "http" {
                "streamable-http".into()
            } else {
                kind
            };
            cfg["serverName"] = name.clone().into();
            plugins.push(serde_json::json!({"id":format!("mcp-{name}"), "name":"@deepseek-ai/dsh-mcp-client", "config":cfg}));
        }
        let added = plugins.len();
        if !dry_run && added > 0 {
            let mut document = serde_yaml::to_value(serde_json::json!([{"insert":plugins}]))
                .map_err(|_| "Unable to serialize DSH config")?;
            dsh_references(&mut document)?;
            let block =
                serde_yaml::to_string(&document).map_err(|_| "Unable to serialize DSH config")?;
            let text = format!("{}\n{}", text.trim_end(), block);
            let _: serde_yaml::Value = serde_yaml::from_str(&text).map_err(|_| {
                "Generated DSH configuration is not valid YAML; destination unchanged"
            })?;
            safe_io::write_config_atomic(path, &text)
                .map_err(|_| "Unable to back up or replace destination config")?;
        }
        return Ok((added, values.len() - added));
    }
    let mut existing: Value = match original {
        Some(text) => serde_json::from_str(&text)
            .map_err(|_| "Invalid destination JSON; refusing overwrite")?,
        None => serde_json::json!({}),
    };
    let object = existing
        .as_object_mut()
        .ok_or("Config root must be an object")?;
    let servers = object
        .entry(bucket(client))
        .or_insert(serde_json::json!({}))
        .as_object_mut()
        .ok_or("Server collection must be an object")?;
    let mut added = 0;
    for (name, value) in values {
        if !servers.contains_key(name) {
            servers.insert(name.clone(), value.clone());
            added += 1;
        }
    }
    if !dry_run && added > 0 {
        let output =
            serde_json::to_string_pretty(&existing).map_err(|_| "Unable to serialize config")?;
        safe_io::write_config_atomic(path, &output)
            .map_err(|_| "Unable to back up or replace destination config")?;
    }
    Ok((added, values.len() - added))
}

fn dsh_references(value: &mut serde_yaml::Value) -> Result<(), String> {
    match value {
        serde_yaml::Value::Sequence(values) => {
            for value in values {
                dsh_references(value)?;
            }
        }
        serde_yaml::Value::Mapping(fields) => {
            for (key, value) in fields {
                if matches!(key.as_str(), Some("env" | "headers")) {
                    if let Some(values) = value.as_mapping_mut() {
                        for (_, value) in values {
                            if let Some(text) = value.as_str() {
                                if let Some(name) =
                                    text.strip_prefix("${").and_then(|s| s.strip_suffix('}'))
                                {
                                    if name.is_empty()
                                        || !name
                                            .chars()
                                            .all(|c| c.is_ascii_alphanumeric() || c == '_')
                                    {
                                        return Err("Invalid DSH environment reference".into());
                                    }
                                    *value = serde_yaml::Value::Tagged(Box::new(
                                        serde_yaml::value::TaggedValue {
                                            tag: serde_yaml::value::Tag::new("!!js"),
                                            value: format!("process.env.{name}").into(),
                                        },
                                    ));
                                } else if text.contains("${") {
                                    return Err("DSH requires an entire env/header value to be a single environment reference".into());
                                }
                            }
                        }
                    }
                } else {
                    dsh_references(value)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
