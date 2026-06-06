use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct McpCheckResult {
    pub server_name: String,
    pub status: McpStatus,
    pub tools: Vec<String>,
    pub resources: Vec<String>,
    pub latency_ms: u64,
}

#[derive(Debug)]
pub enum McpStatus {
    Healthy,
    ConnectionFailed(String),
    Timeout,
    InvalidResponse(String),
}

impl std::fmt::Display for McpStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpStatus::Healthy => write!(f, "MCP handshake OK"),
            McpStatus::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            McpStatus::Timeout => write!(f, "No MCP response (timeout)"),
            McpStatus::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
        }
    }
}

pub fn check_server(
    name: &str,
    command: &str,
    args: &[String],
    env: &std::collections::HashMap<String, String>,
    timeout: Duration,
) -> McpCheckResult {
    let start = Instant::now();

    // Spawn the MCP server process
    let child = Command::new(command)
        .args(args)
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            return McpCheckResult {
                server_name: name.to_string(),
                status: McpStatus::ConnectionFailed(format!("spawn failed: {}", e)),
                tools: vec![],
                resources: vec![],
                latency_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    // Send initialize request
    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {
                "name": "mcp-hub",
                "version": "0.1.0"
            }
        }
    });

    let stdin = child.stdin.as_mut().unwrap();
    let msg = serde_json::to_string(&init_request).unwrap();
    let frame = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
    if stdin.write_all(frame.as_bytes()).is_err() {
        let _ = child.kill();
        return McpCheckResult {
            server_name: name.to_string(),
            status: McpStatus::ConnectionFailed("failed to write to stdin".into()),
            tools: vec![],
            resources: vec![],
            latency_ms: start.elapsed().as_millis() as u64,
        };
    }
    let _ = stdin.flush();

    // Read response with timeout
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let init_response = match read_mcp_response(&mut reader, timeout) {
        Ok(resp) => resp,
        Err(e) => {
            let _ = child.kill();
            let status = if e.contains("timeout") {
                McpStatus::Timeout
            } else {
                McpStatus::ConnectionFailed(e)
            };
            return McpCheckResult {
                server_name: name.to_string(),
                status,
                tools: vec![],
                resources: vec![],
                latency_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    // Validate initialize response
    if init_response.get("error").is_some() {
        let err_msg = init_response["error"]
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        let _ = child.kill();
        return McpCheckResult {
            server_name: name.to_string(),
            status: McpStatus::InvalidResponse(format!("initialize error: {}", err_msg)),
            tools: vec![],
            resources: vec![],
            latency_ms: start.elapsed().as_millis() as u64,
        };
    }

    // Send initialized notification
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let msg = serde_json::to_string(&initialized).unwrap();
    let frame = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
    let _ = stdin.write_all(frame.as_bytes());
    let _ = stdin.flush();

    // Request tools/list
    let tools_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    });
    let msg = serde_json::to_string(&tools_request).unwrap();
    let frame = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
    let _ = stdin.write_all(frame.as_bytes());
    let _ = stdin.flush();

    let tools_response = match read_mcp_response(&mut reader, timeout) {
        Ok(resp) => resp,
        Err(_) => {
            let _ = child.kill();
            return McpCheckResult {
                server_name: name.to_string(),
                status: McpStatus::Healthy, // initialize succeeded, tools timed out
                tools: vec![],
                resources: vec![],
                latency_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    let tools: Vec<String> = tools_response
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Request resources/list
    let resources_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "resources/list"
    });
    let msg = serde_json::to_string(&resources_request).unwrap();
    let frame = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
    let _ = stdin.write_all(frame.as_bytes());
    let _ = stdin.flush();

    let resources: Vec<String> = match read_mcp_response(&mut reader, timeout) {
        Ok(resp) => resp
            .get("result")
            .and_then(|r| r.get("resources"))
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| r.get("uri").and_then(|u| u.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        Err(_) => vec![],
    };

    let _ = child.kill();
    let latency = start.elapsed().as_millis() as u64;

    McpCheckResult {
        server_name: name.to_string(),
        status: McpStatus::Healthy,
        tools,
        resources,
        latency_ms: latency,
    }
}

/// Read a single MCP JSON-RPC response from the reader.
/// MCP uses a Content-Length header framing format.
fn read_mcp_response<R: BufRead>(
    reader: &mut R,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let deadline = Instant::now() + timeout;

    // Read headers
    let mut content_length: Option<usize> = None;
    let mut header_buf = String::new();

    loop {
        if Instant::now() > deadline {
            return Err("timeout reading headers".into());
        }

        header_buf.clear();
        let bytes_read = reader
            .read_line(&mut header_buf)
            .map_err(|e| format!("read error: {}", e))?;

        if bytes_read == 0 {
            return Err("EOF before response".into());
        }

        let trimmed = header_buf.trim();
        if trimmed.is_empty() {
            // End of headers
            break;
        }

        if let Some(val) = trimmed.strip_prefix("Content-Length:") {
            content_length = val.trim().parse::<usize>().ok();
        }
    }

    let length = content_length.ok_or("missing Content-Length header")?;

    // Read body
    let mut body = vec![0u8; length];
    let mut total_read = 0;
    while total_read < length {
        if Instant::now() > deadline {
            return Err("timeout reading body".into());
        }
        match reader.read(&mut body[total_read..]) {
            Ok(0) => return Err("EOF while reading body".into()),
            Ok(n) => total_read += n,
            Err(e) => return Err(format!("read error: {}", e)),
        }
    }

    serde_json::from_slice(&body).map_err(|e| format!("invalid JSON: {}", e))
}

// ─── Benchmark support ───

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

pub fn bench_server(
    name: &str,
    command: &str,
    args: &[String],
    env: &std::collections::HashMap<String, String>,
    timeout: Duration,
) -> BenchResult {
    let total_start = Instant::now();
    let fail = |msg: String| BenchResult {
        server_name: name.to_string(),
        spawn_ms: 0,
        init_ms: 0,
        tools_list_ms: 0,
        total_ms: total_start.elapsed().as_millis() as u64,
        tools_count: 0,
        status: "failed".into(),
        error: Some(msg),
    };

    // Spawn
    let spawn_start = Instant::now();
    let child = Command::new(command)
        .args(args)
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let spawn_ms = spawn_start.elapsed().as_millis() as u64;

    let mut child = match child {
        Ok(c) => c,
        Err(e) => return fail(format!("spawn: {}", e)),
    };

    // Initialize
    let init_start = Instant::now();
    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "mcp-hub", "version": "0.1.0"}
        }
    });
    let stdin = child.stdin.as_mut().unwrap();
    let msg = serde_json::to_string(&init_request).unwrap();
    let frame = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
    if stdin.write_all(frame.as_bytes()).is_err() {
        let _ = child.kill();
        return fail("write init failed".into());
    }
    let _ = stdin.flush();

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    match read_mcp_response(&mut reader, timeout) {
        Ok(resp) => {
            if resp.get("error").is_some() {
                let _ = child.kill();
                let init_ms = init_start.elapsed().as_millis() as u64;
                return BenchResult {
                    server_name: name.to_string(),
                    spawn_ms,
                    init_ms,
                    tools_list_ms: 0,
                    total_ms: total_start.elapsed().as_millis() as u64,
                    tools_count: 0,
                    status: "init_error".into(),
                    error: Some("initialize returned error".into()),
                };
            }
        }
        Err(e) => {
            let _ = child.kill();
            let init_ms = init_start.elapsed().as_millis() as u64;
            return BenchResult {
                server_name: name.to_string(),
                spawn_ms,
                init_ms,
                tools_list_ms: 0,
                total_ms: total_start.elapsed().as_millis() as u64,
                tools_count: 0,
                status: if e.contains("timeout") {
                    "timeout"
                } else {
                    "failed"
                }
                .into(),
                error: Some(e),
            };
        }
    }
    let init_ms = init_start.elapsed().as_millis() as u64;

    // Send initialized notification
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let msg = serde_json::to_string(&initialized).unwrap();
    let frame = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
    let _ = stdin.write_all(frame.as_bytes());
    let _ = stdin.flush();

    // tools/list
    let tools_start = Instant::now();
    let tools_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    });
    let msg = serde_json::to_string(&tools_request).unwrap();
    let frame = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
    let _ = stdin.write_all(frame.as_bytes());
    let _ = stdin.flush();

    let tools_count = match read_mcp_response(&mut reader, timeout) {
        Ok(resp) => resp
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_array())
            .map(|a| a.len())
            .unwrap_or(0),
        Err(_) => 0,
    };
    let tools_list_ms = tools_start.elapsed().as_millis() as u64;

    let _ = child.kill();

    BenchResult {
        server_name: name.to_string(),
        spawn_ms,
        init_ms,
        tools_list_ms,
        total_ms: total_start.elapsed().as_millis() as u64,
        tools_count,
        status: "ok".into(),
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_status_display() {
        assert_eq!(format!("{}", McpStatus::Healthy), "MCP handshake OK");
        assert!(format!("{}", McpStatus::Timeout).contains("timeout"));
        assert!(format!("{}", McpStatus::ConnectionFailed("err".into())).contains("err"));
    }

    #[test]
    fn test_check_nonexistent_command() {
        let result = check_server(
            "test",
            "nonexistent_command_xyz_12345",
            &[],
            &std::collections::HashMap::new(),
            Duration::from_secs(1),
        );
        assert!(matches!(result.status, McpStatus::ConnectionFailed(_)));
    }

    #[test]
    fn test_read_mcp_response_empty() {
        let data = b"Content-Length: 0\r\n\r\n";
        let mut reader = BufReader::new(&data[..]);
        // Empty body should fail to parse as JSON
        let result = read_mcp_response(&mut reader, Duration::from_secs(1));
        assert!(result.is_err());
    }

    #[test]
    fn test_read_mcp_response_valid() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        let data = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let bytes = data.as_bytes();
        let mut reader = BufReader::new(&bytes[..]);
        let result = read_mcp_response(&mut reader, Duration::from_secs(1));
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp["id"], 1);
    }
}
