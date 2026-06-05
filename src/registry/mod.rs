use serde::Deserialize;

const REGISTRY_BASE: &str = "https://registry.modelcontextprotocol.io";

#[derive(Debug, Deserialize)]
pub struct RegistryServer {
    #[serde(rename = "server")]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct RegistryResponse {
    pub servers: Vec<RegistryServer>,
    #[allow(dead_code)]
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ServerVersion {
    pub version: String,
    #[allow(dead_code)]
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub packages: Vec<PackageInfo>,
}

#[derive(Debug, Deserialize)]
pub struct PackageInfo {
    #[serde(rename = "type", default)]
    pub pkg_type: String,
    #[serde(default)]
    pub identifier: String,
    #[allow(dead_code)]
    #[serde(default)]
    pub registry_url: String,
    #[allow(dead_code)]
    #[serde(default)]
    pub transport: serde_json::Value,
}

/// Search the MCP Registry for servers matching a query.
/// Since the API has no search param, we fetch a page and filter locally.
pub fn search(query: &str, limit: usize) -> Result<Vec<RegistryServer>, String> {
    let url = format!("{}/v0.1/servers?limit=100", REGISTRY_BASE);
    let body = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .map_err(|e| format!("Registry request failed: {}", e))?
        .into_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;

    let resp: RegistryResponse =
        serde_json::from_str(&body).map_err(|e| format!("Invalid registry response: {}", e))?;

    let query_lower = query.to_lowercase();
    let mut results: Vec<RegistryServer> = resp
        .servers
        .into_iter()
        .filter(|s| {
            s.name.to_lowercase().contains(&query_lower)
                || s.description.to_lowercase().contains(&query_lower)
        })
        .take(limit)
        .collect();

    // Sort by relevance: exact name match first, then description match
    results.sort_by(|a, b| {
        let a_name = a.name.to_lowercase().contains(&query_lower);
        let b_name = b.name.to_lowercase().contains(&query_lower);
        b_name.cmp(&a_name)
    });

    Ok(results)
}

/// Get the latest version info for a server from the registry.
pub fn get_latest(server_name: &str) -> Result<ServerVersion, String> {
    let encoded = urlencoding::encode(server_name);
    let url = format!(
        "{}/v0.1/servers/{}/versions/latest",
        REGISTRY_BASE, encoded
    );
    let body = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .map_err(|e| format!("Registry request failed: {}", e))?
        .into_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;

    serde_json::from_str(&body).map_err(|e| format!("Invalid version response: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_response_deserialize() {
        let json = r#"{
            "servers": [
                {"server": "com.example/test-mcp", "description": "A test server"},
                {"server": "io.github.cool/awesome-mcp", "description": "An awesome MCP server"}
            ],
            "next_cursor": null
        }"#;
        let resp: RegistryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.servers.len(), 2);
        assert_eq!(resp.servers[0].name, "com.example/test-mcp");
        assert!(resp.next_cursor.is_none());
    }

    #[test]
    fn test_server_version_deserialize() {
        let json = r#"{
            "version": "1.0.0",
            "description": "Test server",
            "packages": [
                {
                    "type": "npm",
                    "identifier": "@example/test-mcp",
                    "registry_url": "https://www.npmjs.com/package/@example/test-mcp",
                    "transport": {}
                }
            ]
        }"#;
        let ver: ServerVersion = serde_json::from_str(json).unwrap();
        assert_eq!(ver.version, "1.0.0");
        assert_eq!(ver.packages.len(), 1);
        assert_eq!(ver.packages[0].identifier, "@example/test-mcp");
        assert_eq!(ver.packages[0].pkg_type, "npm");
    }

    #[test]
    fn test_search_filter_logic() {
        // Test the filtering logic by simulating server list
        let servers = vec![
            RegistryServer {
                name: "com.example/postgres-mcp".into(),
                description: "PostgreSQL integration".into(),
            },
            RegistryServer {
                name: "io.github/weather".into(),
                description: "Weather data".into(),
            },
            RegistryServer {
                name: "org.test/pg-admin".into(),
                description: "Database admin tool".into(),
            },
        ];

        let query = "postgres";
        let query_lower = query.to_lowercase();
        let results: Vec<_> = servers
            .into_iter()
            .filter(|s| {
                s.name.to_lowercase().contains(&query_lower)
                    || s.description.to_lowercase().contains(&query_lower)
            })
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "com.example/postgres-mcp");
    }
}
