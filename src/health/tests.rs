use super::{mcp_check::*, rpc};
use crate::config::{
    model::{ClientType, ServerEntry, ServerStatus},
    transport::Transport,
};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn python() -> String {
    std::env::var("MCP_HUB_TEST_PYTHON")
        .unwrap_or_else(|_| if cfg!(windows) { "python" } else { "python3" }.into())
}
fn fixture() -> String {
    format!(
        "{}/tests/fixtures/mcp_server.py",
        env!("CARGO_MANIFEST_DIR")
    )
}
fn entry(mode: &str) -> ServerEntry {
    ServerEntry {
        name: "audit mock".into(),
        transport: Transport::Stdio,
        command: python(),
        args: vec!["-u".into(), fixture(), "stdio".into(), mode.into()],
        env: HashMap::new(),
        source_client: ClientType::Cursor,
        source_path: "mock.json".into(),
        status: ServerStatus::Active,
    }
}
struct HttpFixture(Child);
impl Drop for HttpFixture {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn http(mode: &str) -> (HttpFixture, ServerEntry) {
    let mut command = Command::new(python());
    command
        .args(["-u", &fixture(), "http", mode])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .expect("Protocol tests require Python 3 (or MCP_HUB_TEST_PYTHON)");
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port: u16 = line
        .trim()
        .parse()
        .expect("Mock HTTP server must report loopback port");
    let mut e = entry(mode);
    e.command.clear();
    e.args.clear();
    e.transport = Transport::StreamableHttp {
        url: format!("http://127.0.0.1:{port}/mcp"),
        headers: HashMap::new(),
    };
    (HttpFixture(child), e)
}
#[test]
fn stdio_legacy_modern_notifications_and_pagination() {
    for mode in ["legacy", "modern", "notifications", "pages", "no-tools"] {
        let result = check_entry(&entry(mode), Duration::from_secs(5));
        assert!(
            matches!(result.status, McpStatus::Healthy),
            "{mode}: {:?}",
            result.status
        );
        assert_eq!(
            result.protocol.as_deref(),
            Some(if mode == "legacy" {
                "2025-03-26"
            } else {
                rpc::MODERN
            })
        );
        assert_eq!(
            result.tools.len(),
            if mode == "pages" {
                2
            } else if mode == "no-tools" {
                0
            } else {
                1
            }
        );
    }
}
#[test]
fn errors_and_incompatible_versions_never_report_success() {
    for mode in [
        "tool-error",
        "unsupported",
        "legacy-bad-version",
        "cursor-loop",
    ] {
        let result = check_entry(&entry(mode), Duration::from_secs(5));
        assert!(!matches!(result.status, McpStatus::Healthy), "{mode}");
        assert_ne!(
            bench_entry(&entry(mode), Duration::from_secs(5)).status,
            "ok",
            "{mode}"
        );
    }
}
#[test]
fn stdio_timeout_has_one_total_deadline() {
    let now = Instant::now();
    assert!(matches!(
        check_entry(&entry("delay"), Duration::from_millis(800)).status,
        McpStatus::Timeout
    ));
    assert!(now.elapsed() < Duration::from_secs(3));
}
#[test]
fn http_json_sse_legacy_session_and_pages() {
    for mode in ["modern", "sse", "legacy", "pages", "no-tools", "headers"] {
        let (_server, mut entry) = http(mode);
        if mode == "headers" {
            if let Transport::StreamableHttp { headers, .. } = &mut entry.transport {
                headers.insert("Authorization".into(), "Bearer audit-only".into());
            }
        }
        let result = check_entry(&entry, Duration::from_secs(5));
        assert!(
            matches!(result.status, McpStatus::Healthy),
            "{mode}: {:?}",
            result.status
        );
        assert_eq!(
            result.tools.len(),
            if mode == "pages" {
                2
            } else if mode == "no-tools" {
                0
            } else {
                1
            }
        );
    }
}
#[test]
fn http_auth_errors_are_not_legacy_fallback() {
    for mode in ["auth", "forbidden"] {
        let (_server, entry) = http(mode);
        assert!(matches!(
            check_entry(&entry, Duration::from_secs(3)).status,
            McpStatus::AuthenticationFailed
        ));
    }
    for mode in ["server-error", "unsupported", "tool-error"] {
        let (_server, entry) = http(mode);
        assert_ne!(bench_entry(&entry, Duration::from_secs(3)).status, "ok");
    }
}
#[test]
fn http_timeout_and_deprecated_sse_are_explicit() {
    let (_server, entry) = http("delay");
    assert!(matches!(
        check_entry(&entry, Duration::from_millis(800)).status,
        McpStatus::Timeout
    ));
    let mut entry = entry;
    entry.transport = Transport::Sse {
        url: "http://127.0.0.1:1/sse".into(),
        headers: HashMap::new(),
    };
    assert!(matches!(
        check_entry(&entry, Duration::from_secs(1)).status,
        McpStatus::Unsupported(_)
    ));
}

#[test]
fn reserved_and_malformed_http_headers_fail_before_connection() {
    for (name, value) in [
        ("MCP-Protocol-Version", "wrong"),
        ("Host", "example.invalid"),
        ("X-Test", "value\r\nAuthorization: injection"),
    ] {
        let mut entry = entry("modern");
        entry.transport = Transport::StreamableHttp {
            url: "http://127.0.0.1:1/mcp".into(),
            headers: HashMap::from([(name.into(), value.into())]),
        };
        assert!(matches!(
            check_entry(&entry, Duration::from_secs(1)).status,
            McpStatus::InvalidResponse(_)
        ));
    }
}
