use super::rpc::{self, Error, LEGACY, MODERN};
use super::wire::Wire;
use crate::config::{
    model::{ClientType, ServerEntry, ServerStatus},
    transport::Transport,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct McpCheckResult {
    pub server_name: String,
    pub status: McpStatus,
    pub tools: Vec<String>,
    pub resources: Vec<String>,
    pub latency_ms: u64,
    pub protocol: Option<String>,
    pub transport: String,
}
#[derive(Debug)]
pub enum McpStatus {
    Healthy,
    ConnectionFailed(String),
    Timeout,
    InvalidResponse(String),
    Unsupported(String),
    AuthenticationFailed,
}
impl std::fmt::Display for McpStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "MCP probe OK"),
            Self::ConnectionFailed(s) => write!(f, "Connection failed: {s}"),
            Self::Timeout => write!(f, "No MCP response (timeout)"),
            Self::InvalidResponse(s) => write!(f, "Invalid response: {s}"),
            Self::Unsupported(s) => write!(f, "Unsupported: {s}"),
            Self::AuthenticationFailed => write!(f, "Authentication required or forbidden"),
        }
    }
}
#[derive(Debug, Clone)]
pub struct BenchResult {
    pub server_name: String,
    pub spawn_ms: u64,
    pub init_ms: u64,
    pub tools_list_ms: u64,
    pub total_ms: u64,
    pub tools_count: usize,
    pub status: String,
    pub error: Option<String>,
}
fn legacy_version(value: &str) -> bool {
    LEGACY.contains(&value)
}
fn capabilities(result: &Value) -> Result<Value, Error> {
    let caps = result
        .get("capabilities")
        .filter(|v| v.is_object())
        .cloned()
        .ok_or(Error::Invalid("Missing server capabilities"))?;
    for name in ["tools", "resources"] {
        if caps.get(name).is_some_and(|value| !value.is_object()) {
            return Err(Error::Invalid("Invalid server capability declaration"));
        }
    }
    Ok(caps)
}
fn modern_discovery(result: &Value) -> Result<Value, Error> {
    if result["resultType"] != "complete" {
        return Err(Error::Invalid("Discovery result is not complete"));
    }
    let versions = result
        .get("supportedVersions")
        .and_then(Value::as_array)
        .ok_or(Error::Invalid("Discovery is missing supportedVersions"))?;
    if !versions.iter().any(|v| v.as_str() == Some(MODERN)) {
        return Err(Error::Unsupported(
            "Server offers no supported modern protocol version",
        ));
    }
    capabilities(result)
}
fn fallback(error: &Error, stdio: bool) -> bool {
    match error {
        Error::Rpc(code, _) => !(-32099..=-32020).contains(code),
        Error::Timeout | Error::Connection => stdio,
        Error::Http(400 | 404 | 405) => !stdio,
        _ => false,
    }
}
type Negotiated = (Wire, bool, String, Value, u64);
fn open(entry: &ServerEntry, timeout: Duration, deadline: Instant) -> Result<Negotiated, Error> {
    let stdio = matches!(entry.transport, Transport::Stdio);
    let spawn_start = Instant::now();
    let mut probe = Wire::connect(entry)?;
    let mut spawn_ms = if stdio {
        spawn_start.elapsed().as_millis() as u64
    } else {
        0
    };
    let probe_deadline = if stdio {
        Instant::now() + std::cmp::min(timeout / 2, Duration::from_secs(2))
    } else {
        deadline
    };
    let discovery = probe.request(
        &rpc::request(1, "server/discover", json!({}), true),
        probe_deadline,
    );
    let modern = match discovery {
        Ok(result) => Some(modern_discovery(&result)?),
        Err(Error::Rpc(-32022, data)) => {
            let versions =
                data.get("supported")
                    .and_then(Value::as_array)
                    .ok_or(Error::Unsupported(
                        "Version rejection omitted supported versions",
                    ))?;
            if versions.iter().any(|v| v.as_str() == Some(MODERN)) {
                Some(modern_discovery(&probe.request(
                    &rpc::request(2, "server/discover", json!({}), true),
                    deadline,
                )?)?)
            } else if versions
                .iter()
                .any(|v| v.as_str().is_some_and(legacy_version))
            {
                None
            } else {
                return Err(Error::Unsupported("No mutually supported protocol version"));
            }
        }
        Err(error) if fallback(&error, stdio) => None,
        Err(error) => return Err(error),
    };
    rpc::remaining(deadline)?;
    // Probe on a disposable child: some old servers exit before initialize.
    let mut wire = if stdio {
        drop(probe);
        let now = Instant::now();
        let wire = Wire::connect(entry)?;
        spawn_ms += now.elapsed().as_millis() as u64;
        wire
    } else {
        probe
    };
    if let Some(caps) = modern {
        return Ok((wire, true, MODERN.into(), caps, spawn_ms));
    }
    let mut version = LEGACY[0].to_string();
    wire.version(&version);
    let init = |v: &str| {
        rpc::request(
            3,
            "initialize",
            json!({
                "protocolVersion":v,"capabilities":{},"clientInfo":{"name":"mcp-hub","version":env!("CARGO_PKG_VERSION")}
            }),
            false,
        )
    };
    let response = match wire.request(&init(&version), deadline) {
        Err(Error::Rpc(-32022, data)) => {
            version = data
                .get("supported")
                .and_then(Value::as_array)
                .and_then(|versions| {
                    LEGACY
                        .iter()
                        .find(|v| versions.iter().any(|x| x.as_str() == Some(**v)))
                })
                .ok_or(Error::Unsupported("No mutually supported legacy version"))?
                .to_string();
            wire.version(&version);
            wire.request(&init(&version), deadline)?
        }
        other => other?,
    };
    let selected = response
        .get("protocolVersion")
        .and_then(Value::as_str)
        .ok_or(Error::Invalid("Initialize omitted protocolVersion"))?;
    if !legacy_version(selected) {
        return Err(Error::Unsupported(
            "Server selected an unsupported legacy version",
        ));
    }
    version = selected.into();
    wire.version(&version);
    let caps = capabilities(&response)?;
    wire.notify(
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        deadline,
    )?;
    Ok((wire, false, version, caps, spawn_ms))
}
fn list(
    wire: &mut Wire,
    modern: bool,
    method: &str,
    key: &str,
    field: &str,
    id: &mut u64,
    deadline: Instant,
) -> Result<Vec<String>, Error> {
    let mut result = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen = HashSet::new();
    for _ in 0..100 {
        let mut params = json!({});
        if let Some(value) = &cursor {
            params["cursor"] = value.clone().into();
        }
        *id += 1;
        let page = wire.request(&rpc::request(*id, method, params, modern), deadline)?;
        if modern && page["resultType"] != "complete" {
            return Err(Error::Unsupported(
                "Listing requires unsupported client input",
            ));
        }
        let values = page
            .get(key)
            .and_then(Value::as_array)
            .ok_or(Error::Invalid("List response omitted its array"))?;
        for value in values {
            let name = value
                .get(field)
                .and_then(Value::as_str)
                .ok_or(Error::Invalid("List item omitted name/URI"))?;
            result.push(name.to_string());
            if result.len() > 10000 {
                return Err(Error::Invalid("List exceeds 10000 entries"));
            }
        }
        match page.get("nextCursor") {
            None | Some(Value::Null) => return Ok(result),
            Some(Value::String(next)) if !next.is_empty() && seen.insert(next.clone()) => {
                cursor = Some(next.clone())
            }
            _ => return Err(Error::Invalid("Invalid or repeated pagination cursor")),
        }
    }
    Err(Error::Invalid("List exceeds 100 pages"))
}
fn probe(entry: &ServerEntry, timeout: Duration, resources: bool) -> (McpCheckResult, BenchResult) {
    let start = Instant::now();
    let mut check = McpCheckResult {
        server_name: entry.name.clone(),
        status: McpStatus::Healthy,
        tools: Vec::new(),
        resources: Vec::new(),
        latency_ms: 0,
        protocol: None,
        transport: entry.transport.label().into(),
    };
    let mut bench = BenchResult {
        server_name: entry.name.clone(),
        spawn_ms: 0,
        init_ms: 0,
        tools_list_ms: 0,
        total_ms: 0,
        tools_count: 0,
        status: "ok".into(),
        error: None,
    };
    let mut stage = "connect";
    let outcome = (|| {
        if !matches!(entry.status, ServerStatus::Active) {
            return Err(Error::Config(
                "Server configuration is invalid or disabled".into(),
            ));
        }
        if timeout.is_zero() {
            return Err(Error::Timeout);
        }
        let deadline = start
            .checked_add(timeout)
            .ok_or(Error::Config("Timeout is too large".into()))?;
        stage = "version/capability discovery";
        let (mut wire, modern, version, caps, spawn_ms) = open(entry, timeout, deadline)?;
        check.protocol = Some(version);
        bench.spawn_ms = spawn_ms;
        bench.init_ms = (start.elapsed().as_millis() as u64).saturating_sub(spawn_ms);
        let mut id = 10;
        if caps.get("tools").is_some() {
            stage = "tools/list";
            let now = Instant::now();
            check.tools = list(
                &mut wire,
                modern,
                "tools/list",
                "tools",
                "name",
                &mut id,
                deadline,
            )?;
            bench.tools_list_ms = now.elapsed().as_millis() as u64;
        }
        if resources && caps.get("resources").is_some() {
            stage = "resources/list";
            check.resources = list(
                &mut wire,
                modern,
                "resources/list",
                "resources",
                "uri",
                &mut id,
                deadline,
            )?;
        }
        Ok(())
    })();
    if let Err(error) = outcome {
        let detail = format!("{stage}: {error}");
        check.status = match &error {
            Error::Timeout => McpStatus::Timeout,
            Error::Connection => McpStatus::ConnectionFailed(detail.clone()),
            Error::Unsupported(_) => McpStatus::Unsupported(detail.clone()),
            Error::Http(401 | 403) => McpStatus::AuthenticationFailed,
            _ => McpStatus::InvalidResponse(detail.clone()),
        };
        bench.status = match check.status {
            McpStatus::Timeout => "timeout",
            McpStatus::Unsupported(_) => "unsupported",
            McpStatus::AuthenticationFailed => "authentication_failed",
            _ => "failed",
        }
        .into();
        bench.error = Some(detail);
    }
    check.latency_ms = start.elapsed().as_millis() as u64;
    bench.total_ms = check.latency_ms;
    bench.tools_count = check.tools.len();
    (check, bench)
}
pub fn check_entry(entry: &ServerEntry, timeout: Duration) -> McpCheckResult {
    probe(entry, timeout, true).0
}
pub fn bench_entry(entry: &ServerEntry, timeout: Duration) -> BenchResult {
    probe(entry, timeout, false).1
}
#[allow(dead_code)]
pub fn check_server(
    name: &str,
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
    timeout: Duration,
) -> McpCheckResult {
    check_entry(&stdio_entry(name, command, args, env), timeout)
}
#[allow(dead_code)]
pub fn bench_server(
    name: &str,
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
    timeout: Duration,
) -> BenchResult {
    bench_entry(&stdio_entry(name, command, args, env), timeout)
}
fn stdio_entry(
    name: &str,
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
) -> ServerEntry {
    ServerEntry {
        transport: Transport::Stdio,
        name: name.into(),
        command: command.into(),
        args: args.into(),
        env: env.clone(),
        source_client: ClientType::ClaudeDesktop,
        source_path: "".into(),
        status: ServerStatus::Active,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nonexistent_command_is_not_healthy() {
        assert!(matches!(
            check_server(
                "test",
                "nonexistent_command_xyz_12345",
                &[],
                &HashMap::new(),
                Duration::from_secs(1)
            )
            .status,
            McpStatus::ConnectionFailed(_)
        ));
    }
}
