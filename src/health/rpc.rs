use serde_json::{json, Value};
use std::fmt;
use std::io::{BufRead, Read};
use std::time::{Duration, Instant};

pub const MODERN: &str = "2026-07-28";
pub const LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
pub const LIMIT: usize = 1024 * 1024;

#[derive(Debug)]
pub enum Error {
    Timeout,
    Connection,
    Invalid(&'static str),
    Rpc(i64, Value),
    Http(u16),
    Unsupported(&'static str),
    Config(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => write!(f, "request deadline exceeded"),
            Self::Connection => write!(f, "connection closed or unavailable"),
            Self::Invalid(s) | Self::Unsupported(s) => write!(f, "{s}"),
            Self::Rpc(code, _) => write!(f, "JSON-RPC error {code}"),
            Self::Http(code) => write!(f, "HTTP {code}"),
            Self::Config(s) => write!(f, "{s}"),
        }
    }
}

pub fn remaining(deadline: Instant) -> Result<Duration, Error> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or(Error::Timeout)
}

pub fn io_error(error: std::io::Error) -> Error {
    if matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) {
        Error::Timeout
    } else {
        Error::Connection
    }
}

pub fn request(id: u64, method: &str, mut params: Value, modern: bool) -> Value {
    if modern {
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": MODERN,
            "io.modelcontextprotocol/clientInfo": {"name":"mcp-hub","version":env!("CARGO_PKG_VERSION")},
            "io.modelcontextprotocol/clientCapabilities": {},
        });
    }
    json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params})
}

/// Notifications and unrelated responses never complete the pending request.
pub fn response(value: Value, id: &Value) -> Result<Option<Value>, Error> {
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(Error::Invalid("Invalid JSON-RPC version"));
    }
    if value.get("method").is_some() {
        if !value["method"].is_string() {
            return Err(Error::Invalid("Invalid JSON-RPC method"));
        }
        if value.get("id").is_some() {
            return Err(Error::Invalid("Unsupported server-initiated request"));
        }
        return Ok(None);
    }
    if value.get("id").is_none() {
        return Err(Error::Invalid("Response is missing ID"));
    }
    if &value["id"] != id {
        return Ok(None);
    }
    match (value.get("result"), value.get("error")) {
        (Some(result), None) if result.is_object() => Ok(Some(result.clone())),
        (None, Some(error)) if error["code"].as_i64().is_some() => Err(Error::Rpc(
            error["code"].as_i64().unwrap(),
            error.get("data").cloned().unwrap_or(Value::Null),
        )),
        _ => Err(Error::Invalid(
            "Response must have exactly one valid result or error",
        )),
    }
}

pub fn read_line(reader: &mut impl BufRead) -> Result<Option<String>, Error> {
    let mut bytes = Vec::new();
    let size = reader
        .take((LIMIT + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(io_error)?;
    if size == 0 {
        return Ok(None);
    }
    if size > LIMIT {
        return Err(Error::Invalid("Message exceeds 1 MiB limit"));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| Error::Invalid("Message is not UTF-8"))
}

pub fn parse_json(text: &str) -> Result<Value, Error> {
    serde_json::from_str(text).map_err(|_| Error::Invalid("Malformed JSON response"))
}

/// Read one request-scoped SSE stream, including multi-line data events/comments.
pub fn read_sse(reader: &mut impl BufRead, id: &Value, deadline: Instant) -> Result<Value, Error> {
    let mut event = String::new();
    let mut total = 0;
    while let Some(line) = read_line(reader)? {
        remaining(deadline)?;
        total += line.len();
        if total > LIMIT {
            return Err(Error::Invalid("SSE response exceeds 1 MiB limit"));
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if !event.is_empty() {
                if let Some(result) = response(parse_json(event.trim_end())?, id)? {
                    return Ok(result);
                }
                event.clear();
            }
        } else if let Some(data) = line.strip_prefix("data:") {
            event.push_str(data.strip_prefix(' ').unwrap_or(data));
            event.push('\n');
        }
    }
    Err(Error::Invalid("SSE ended without a matching response"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notifications_and_ids_do_not_complete_response() {
        assert!(response(
            json!({"jsonrpc":"2.0","method":"notifications/message"}),
            &json!(1)
        )
        .unwrap()
        .is_none());
        assert!(
            response(json!({"jsonrpc":"2.0","id":9,"result":{}}), &json!(1))
                .unwrap()
                .is_none()
        );
        assert!(response(
            json!({"jsonrpc":"2.0","id":1,"result":{},"error":{"code":-1}}),
            &json!(1)
        )
        .is_err());
        assert!(
            response(json!({"jsonrpc":"2.0","id":1,"result":{}}), &json!(1))
                .unwrap()
                .is_some()
        );
    }
    #[test]
    fn sse_handles_comments_notifications_and_multiline_data() {
        let s=b":keepalive\r\n\r\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\ndata: {\"jsonrpc\":\"2.0\",\n data-ignored:ignored\ndata: \"id\":1,\"result\":{\"tools\":[]}}\n\n";
        let mut reader = std::io::Cursor::new(s);
        assert_eq!(
            read_sse(
                &mut reader,
                &json!(1),
                Instant::now() + Duration::from_secs(1)
            )
            .unwrap()["tools"],
            json!([])
        );
    }
}
