use super::rpc::{self, Error, LIMIT};
use crate::config::{
    model::ServerEntry,
    transport::{expand_env, Transport},
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

type WriteJob = (String, Sender<Result<(), Error>>);
pub struct StdioWire {
    child: Child,
    #[cfg(windows)]
    job: Option<super::job::Job>,
    rx: Receiver<Result<String, Error>>,
    tx: Sender<WriteJob>,
}
impl Drop for StdioWire {
    fn drop(&mut self) {
        #[cfg(windows)]
        drop(self.job.take());
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl StdioWire {
    fn spawn(entry: &ServerEntry) -> Result<Self, Error> {
        let command = expand_env(&entry.command).map_err(Error::Config)?;
        let args: Result<Vec<_>, _> = entry.args.iter().map(|s| expand_env(s)).collect();
        let env: Result<HashMap<_, _>, _> = entry
            .env
            .iter()
            .map(|(k, v)| expand_env(v).map(|v| (k, v)))
            .collect();
        let mut cmd = Command::new(command);
        cmd.args(args.map_err(Error::Config)?)
            .envs(env.map_err(Error::Config)?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let mut child = cmd.spawn().map_err(|_| Error::Connection)?;
        #[cfg(windows)]
        let job = match super::job::Job::assign(&child) {
            Ok(job) => Some(job),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Connection);
            }
        };
        let stdout = child.stdout.take().ok_or(Error::Connection)?;
        let mut stdin = child.stdin.take().ok_or(Error::Connection)?;
        let (tx_read, rx) = mpsc::sync_channel(16);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match rpc::read_line(&mut reader) {
                    Ok(Some(line)) => {
                        if tx_read.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = tx_read.send(Err(Error::Connection));
                        break;
                    }
                    Err(error) => {
                        let _ = tx_read.send(Err(error));
                        break;
                    }
                }
            }
        });
        let (tx, rx_write) = mpsc::channel::<WriteJob>();
        std::thread::spawn(move || {
            while let Ok((message, reply)) = rx_write.recv() {
                let result = stdin
                    .write_all(message.as_bytes())
                    .and_then(|_| stdin.flush())
                    .map_err(|_| Error::Connection);
                let failed = result.is_err();
                let _ = reply.send(result);
                if failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            rx,
            tx,
            #[cfg(windows)]
            job,
        })
    }
    fn send(&mut self, value: &Value, deadline: Instant) -> Result<(), Error> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send((format!("{value}\n"), tx))
            .map_err(|_| Error::Connection)?;
        rx.recv_timeout(rpc::remaining(deadline)?)
            .map_err(|_| Error::Timeout)?
    }
    fn request(&mut self, value: &Value, deadline: Instant) -> Result<Value, Error> {
        self.send(value, deadline)?;
        for _ in 0..1024 {
            let line = self
                .rx
                .recv_timeout(rpc::remaining(deadline)?)
                .map_err(|_| Error::Timeout)??;
            if line.trim().is_empty() {
                continue;
            }
            let message = rpc::parse_json(&line)?;
            if message.get("id").is_some() && message.get("method").is_some() {
                let reply = if message["method"] == "ping" {
                    json!({"jsonrpc":"2.0","id":message["id"],"result":{}})
                } else {
                    json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32601,"message":"Client capability not supported"}})
                };
                self.send(&reply, deadline)?;
                continue;
            }
            if let Some(result) = rpc::response(message, &value["id"])? {
                return Ok(result);
            }
        }
        Err(Error::Invalid("Excessive unrelated messages"))
    }
}

pub struct HttpWire {
    url: String,
    headers: HashMap<String, String>,
    session: Option<String>,
    pub version: String,
}
impl HttpWire {
    fn new(url: &str, headers: &HashMap<String, String>) -> Result<Self, Error> {
        let url = expand_env(url).map_err(Error::Config)?;
        let parsed = url::Url::parse(&url).map_err(|_| Error::Config("Invalid HTTP URL".into()))?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            return Err(Error::Config(
                "Use an HTTP(S) endpoint without userinfo/fragment; credentials belong in headers"
                    .into(),
            ));
        }
        let mut resolved = HashMap::new();
        for (name, value) in headers {
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            {
                return Err(Error::Config("Invalid header name".into()));
            }
            let value = expand_env(value).map_err(Error::Config)?;
            if !value.bytes().all(|b| (32..=126).contains(&b) || b == 9) {
                return Err(Error::Config("Invalid header value".into()));
            }
            let key = name.to_ascii_lowercase();
            if [
                "host",
                "content-length",
                "content-type",
                "accept",
                "connection",
                "transfer-encoding",
                "mcp-protocol-version",
                "mcp-method",
                "mcp-name",
                "mcp-session-id",
            ]
            .contains(&key.as_str())
            {
                return Err(Error::Config(
                    "Reserved transport header cannot be overridden".into(),
                ));
            }
            resolved.insert(name.clone(), value);
        }
        Ok(Self {
            url,
            headers: resolved,
            session: None,
            version: rpc::MODERN.into(),
        })
    }
    fn post(&mut self, value: &Value, deadline: Instant) -> Result<Option<Value>, Error> {
        let agent = ureq::AgentBuilder::new().redirects(0).build();
        let mut request = agent
            .post(&self.url)
            .timeout(rpc::remaining(deadline)?)
            .set("Content-Type", "application/json")
            .set("Accept", "application/json, text/event-stream")
            .set("MCP-Protocol-Version", &self.version)
            .set(
                "Mcp-Method",
                value["method"]
                    .as_str()
                    .ok_or(Error::Invalid("Missing method"))?,
            );
        for (key, val) in &self.headers {
            request = request.set(key, val);
        }
        if self.version != rpc::MODERN {
            if let Some(session) = &self.session {
                request = request.set("Mcp-Session-Id", session);
            }
        }
        let reply = match request.send_string(&value.to_string()) {
            Ok(reply) => reply,
            Err(ureq::Error::Status(_, reply)) => reply,
            Err(_) => {
                return if rpc::remaining(deadline).is_err() {
                    Err(Error::Timeout)
                } else {
                    Err(Error::Connection)
                }
            }
        };
        let status = reply.status();
        if status == 401 || status == 403 || status >= 500 || (300..400).contains(&status) {
            return Err(Error::Http(status));
        }
        if let Some(session) = reply.header("Mcp-Session-Id") {
            if self.version != rpc::MODERN {
                self.session = Some(session.into());
            }
        }
        if value.get("id").is_none() {
            return if status == 202 {
                Ok(None)
            } else {
                Err(Error::Http(status))
            };
        }
        let mime = reply
            .header("Content-Type")
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if status >= 400 && mime != "application/json" {
            return Err(Error::Http(status));
        }
        let result = if mime == "text/event-stream" && status < 400 {
            rpc::read_sse(
                &mut BufReader::new(reply.into_reader()),
                &value["id"],
                deadline,
            )
            .map(Some)
        } else if mime == "application/json" {
            let mut text = String::new();
            reply
                .into_reader()
                .take((LIMIT + 1) as u64)
                .read_to_string(&mut text)
                .map_err(rpc::io_error)?;
            if text.len() > LIMIT {
                return Err(Error::Invalid("HTTP response exceeds 1 MiB limit"));
            }
            rpc::response(rpc::parse_json(&text)?, &value["id"])
        } else {
            Err(Error::Invalid("HTTP response must be JSON or SSE"))
        };
        rpc::remaining(deadline)?;
        if status >= 400 {
            return match result {
                Err(Error::Rpc(code, data)) => Err(Error::Rpc(code, data)),
                _ => Err(Error::Http(status)),
            };
        }
        result
    }
}

pub enum Wire {
    Stdio(StdioWire),
    Http(HttpWire),
}
impl Wire {
    pub fn connect(entry: &ServerEntry) -> Result<Self, Error> {
        match &entry.transport {
            Transport::Stdio => StdioWire::spawn(entry).map(Self::Stdio),
            Transport::StreamableHttp {url,headers} => HttpWire::new(url,headers).map(Self::Http),
            Transport::Sse {..} => Err(Error::Unsupported("Legacy HTTP+SSE transport is deprecated and not probed; migrate to Streamable HTTP")),
        }
    }
    pub fn version(&mut self, version: &str) {
        if let Self::Http(wire) = self {
            wire.version = version.into();
        }
    }
    pub fn request(&mut self, value: &Value, deadline: Instant) -> Result<Value, Error> {
        match self {
            Self::Stdio(wire) => wire.request(value, deadline),
            Self::Http(wire) => wire
                .post(value, deadline)?
                .ok_or(Error::Invalid("No matching HTTP response")),
        }
    }
    pub fn notify(&mut self, value: &Value, deadline: Instant) -> Result<(), Error> {
        match self {
            Self::Stdio(wire) => wire.send(value, deadline),
            Self::Http(wire) => wire.post(value, deadline).map(|_| ()),
        }
    }
}
