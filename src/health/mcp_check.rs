use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Kill a child process and wait for it to reap (avoids zombies on Unix).
fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Spawn a reader thread that sends each non-empty line over a channel.
fn spawn_line_reader(
    stdout: std::process::ChildStdout,
) -> (
    mpsc::Receiver<Result<String, String>>,
    thread::JoinHandle<()>,
) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    let _ = tx.send(Err("EOF".into()));
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim().to_string();
                    if !trimmed.is_empty() && tx.send(Ok(trimmed)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(format!("read error: {}", e)));
                    break;
                }
            }
        }
    });
    (rx, handle)
}

/// Receive the next JSON-RPC response with a real timeout.
fn recv_mcp_response(
    rx: &mpsc::Receiver<Result<String, String>>,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    match rx.recv_timeout(timeout) {
        Ok(Ok(line)) => serde_json::from_str(&line).map_err(|e| format!("invalid JSON: {}", e)),
        Ok(Err(e)) => Err(e),
        Err(mpsc::RecvTimeoutError::Timeout) => Err("timeout".into()),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err("channel closed".into()),
    }
}

// ─── Types ───

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

// ─── Health check ───

pub fn check_server(
    name: &str,
    command: &str,
    args: &[String],
    env: &std::collections::HashMap<String, String>,
    timeout: Duration,
) -> McpCheckResult {
    let start = Instant::now();

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
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    });

    {
        let stdin = child.stdin.as_mut().unwrap();
        let msg = serde_json::to_string(&init_request).unwrap();
        let frame = format!("{}\n", msg);
        if stdin.write_all(frame.as_bytes()).is_err() {
            kill_and_reap(&mut child);
            return McpCheckResult {
                server_name: name.to_string(),
                status: McpStatus::ConnectionFailed("failed to write to stdin".into()),
                tools: vec![],
                resources: vec![],
                latency_ms: start.elapsed().as_millis() as u64,
            };
        }
        let _ = stdin.flush();
    }

    let stdout = child.stdout.take().unwrap();
    let (rx, _reader_handle) = spawn_line_reader(stdout);

    let init_response = match recv_mcp_response(&rx, timeout) {
        Ok(resp) => resp,
        Err(e) => {
            kill_and_reap(&mut child);
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

    if init_response.get("error").is_some() {
        let err_msg = init_response["error"]
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        kill_and_reap(&mut child);
        return McpCheckResult {
            server_name: name.to_string(),
            status: McpStatus::InvalidResponse(format!("initialize error: {}", err_msg)),
            tools: vec![],
            resources: vec![],
            latency_ms: start.elapsed().as_millis() as u64,
        };
    }

    // Send initialized notification + tools/list
    {
        let stdin = child.stdin.as_mut().unwrap();
        let initialized = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let msg = serde_json::to_string(&initialized).unwrap();
        let _ = stdin.write_all(format!("{}\n", msg).as_bytes());
        let _ = stdin.flush();

        let tools_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });
        let msg = serde_json::to_string(&tools_request).unwrap();
        let _ = stdin.write_all(format!("{}\n", msg).as_bytes());
        let _ = stdin.flush();
    }

    let tools_response = match recv_mcp_response(&rx, timeout) {
        Ok(resp) => resp,
        Err(_) => {
            kill_and_reap(&mut child);
            return McpCheckResult {
                server_name: name.to_string(),
                status: McpStatus::Healthy,
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
    {
        let stdin = child.stdin.as_mut().unwrap();
        let resources_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "resources/list"
        });
        let msg = serde_json::to_string(&resources_request).unwrap();
        let _ = stdin.write_all(format!("{}\n", msg).as_bytes());
        let _ = stdin.flush();
    }

    let resources: Vec<String> = match recv_mcp_response(&rx, timeout) {
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

    kill_and_reap(&mut child);
    let latency = start.elapsed().as_millis() as u64;

    McpCheckResult {
        server_name: name.to_string(),
        status: McpStatus::Healthy,
        tools,
        resources,
        latency_ms: latency,
    }
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
            "clientInfo": {"name": "mcp-hub", "version": env!("CARGO_PKG_VERSION")}
        }
    });
    {
        let stdin = child.stdin.as_mut().unwrap();
        let msg = serde_json::to_string(&init_request).unwrap();
        let frame = format!("{}\n", msg);
        if stdin.write_all(frame.as_bytes()).is_err() {
            kill_and_reap(&mut child);
            return fail("write init failed".into());
        }
        let _ = stdin.flush();
    }

    let stdout = child.stdout.take().unwrap();
    let (rx, _reader_handle) = spawn_line_reader(stdout);

    match recv_mcp_response(&rx, timeout) {
        Ok(resp) => {
            if resp.get("error").is_some() {
                kill_and_reap(&mut child);
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
            kill_and_reap(&mut child);
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

    // Send initialized notification + tools/list
    {
        let stdin = child.stdin.as_mut().unwrap();
        let initialized = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let msg = serde_json::to_string(&initialized).unwrap();
        let _ = stdin.write_all(format!("{}\n", msg).as_bytes());
        let _ = stdin.flush();

        let tools_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });
        let msg = serde_json::to_string(&tools_request).unwrap();
        let _ = stdin.write_all(format!("{}\n", msg).as_bytes());
        let _ = stdin.flush();
    }

    let tools_start = Instant::now();
    let tools_count = match recv_mcp_response(&rx, timeout) {
        Ok(resp) => resp
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_array())
            .map(|a| a.len())
            .unwrap_or(0),
        Err(_) => 0,
    };
    let tools_list_ms = tools_start.elapsed().as_millis() as u64;

    kill_and_reap(&mut child);

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
    fn test_recv_timeout() {
        let (_tx, rx) = mpsc::channel::<Result<String, String>>();
        // Drop sender immediately so channel is closed... but we want timeout.
        // Keep sender alive by leaking it.
        let result = recv_mcp_response(&rx, Duration::from_millis(100));
        assert!(result.is_err());
    }

    #[test]
    fn test_recv_valid_json() {
        let (tx, rx) = mpsc::channel();
        tx.send(Ok(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#.to_string()))
            .unwrap();
        let result = recv_mcp_response(&rx, Duration::from_secs(1));
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp["id"], 1);
    }

    #[test]
    fn test_recv_eof() {
        let (tx, rx) = mpsc::channel();
        tx.send(Err("EOF".to_string())).unwrap();
        let result = recv_mcp_response(&rx, Duration::from_secs(1));
        assert_eq!(result.unwrap_err(), "EOF");
    }
}
