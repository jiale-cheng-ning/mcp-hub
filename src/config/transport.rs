use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Transport {
    #[default]
    Stdio,
    StreamableHttp {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    Sse {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
}

impl Transport {
    pub fn endpoint(&self) -> Option<(&str, &HashMap<String, String>)> {
        match self {
            Self::Stdio => None,
            Self::StreamableHttp { url, headers } | Self::Sse { url, headers } => {
                Some((url, headers))
            }
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::StreamableHttp { .. } => "http",
            Self::Sse { .. } => "sse (deprecated)",
        }
    }

    pub fn redacted(&self) -> Self {
        match self {
            Self::Stdio => Self::Stdio,
            Self::StreamableHttp { url, headers } | Self::Sse { url, headers } => {
                let headers = headers
                    .iter()
                    .map(|(key, value)| {
                        let value = if super::model::has_secret_keyword(key) && !value.is_empty() {
                            format!("${{MCP_HEADER_{}}}", key.to_uppercase().replace('-', "_"))
                        } else {
                            value.clone()
                        };
                        (key.clone(), value)
                    })
                    .collect();
                let url = redact_url(url);
                if matches!(self, Self::Sse { .. }) {
                    Self::Sse { url, headers }
                } else {
                    Self::StreamableHttp { url, headers }
                }
            }
        }
    }

    pub fn redacted_for(&self, name: &str) -> Self {
        let mut transport = self.redacted();
        let mut hash = 14695981039346656037u64;
        for byte in name.bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(1099511628211);
        }
        if let Self::StreamableHttp { headers, .. } | Self::Sse { headers, .. } = &mut transport {
            for value in headers.values_mut() {
                if value.starts_with("${MCP_HEADER_") {
                    *value =
                        value.replacen("${MCP_HEADER_", &format!("${{MCP_{hash:016X}_HEADER_"), 1);
                }
            }
        }
        transport
    }
}

pub fn redact_url(value: &str) -> String {
    let Ok(mut url) = url::Url::parse(value) else {
        return "<invalid-url>".into();
    };
    if !url.username().is_empty() || url.password().is_some() {
        let _ = url.set_username("REDACTED");
        let _ = url.set_password(None);
    }
    if url.query().is_some() {
        let keys: Vec<String> = url.query_pairs().map(|(k, _)| k.into_owned()).collect();
        url.set_query(None);
        for key in keys {
            url.query_pairs_mut().append_pair(&key, "REDACTED");
        }
    }
    url.set_fragment(None);
    url.into()
}

/// Expand portable and VS Code environment references without evaluating code.
pub fn expand_env(value: &str) -> Result<String, String> {
    if value.starts_with("process.env.") {
        return Err(
            "Dynamic DSH JavaScript references are not evaluated; use an environment placeholder"
                .into(),
        );
    }
    if value.contains("REDACTED") || value.contains("<redacted>") {
        return Err("Replace redacted credentials/URL before import or connection".into());
    }
    let mut rest = value;
    let mut output = String::new();
    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        let end = rest[start + 2..]
            .find('}')
            .ok_or("Unclosed environment reference")?
            + start
            + 2;
        let key = rest[start + 2..end]
            .strip_prefix("env:")
            .unwrap_or(&rest[start + 2..end]);
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err("Unsupported environment/input reference; resolve it before use".into());
        }
        output.push_str(
            &std::env::var(key).map_err(|_| format!("Missing environment variable {key}"))?,
        );
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

pub fn adapt_references(value: &str, client: &super::model::ClientType) -> Result<String, String> {
    if value.starts_with("process.env.") {
        return Err("Complex JavaScript config expressions cannot be migrated".into());
    }
    let mut rest = value;
    let mut result = String::new();
    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        let end = rest[start + 2..]
            .find('}')
            .ok_or("Unclosed environment reference")?
            + start
            + 2;
        let name = rest[start + 2..end]
            .strip_prefix("env:")
            .unwrap_or(&rest[start + 2..end]);
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err("Unsupported input/expression reference; resolve before migration".into());
        }
        use super::model::ClientType;
        match client {
            ClientType::VSCode | ClientType::Cursor | ClientType::Windsurf => {
                result.push_str(&format!("${{env:{name}}}"))
            }
            ClientType::ClaudeDesktop => result.push_str(&std::env::var(name).map_err(|_| {
                format!("Set environment variable {name} before writing Claude Desktop config")
            })?),
            _ => result.push_str(&format!("${{{name}}}")),
        }
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn url_secrets_are_hidden() {
        let value =
            redact_url("https://user:secret@example.com/mcp?token=secret&key=another#secret");
        assert!(!value.contains("secret") && !value.contains("another") && !value.contains("user"));
    }
    #[test]
    fn missing_references_are_rejected() {
        assert!(expand_env("Bearer ${MCP_HUB_MISSING_TEST_VARIABLE_9938}").is_err());
        assert!(expand_env("${input:secret}").is_err());
        assert!(expand_env("https://example.com/?token=REDACTED").is_err());
    }
}
