use crate::config::model::ServerEntry;
use std::fmt;
use sysinfo::System;

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
pub enum HealthStatus {
    Healthy { latency_ms: u64 },
    #[allow(dead_code)]
    Slow { latency_ms: u64 },
    Down,
    Unknown,
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HealthStatus::Healthy { latency_ms } => write!(f, "OK ({}ms)", latency_ms),
            HealthStatus::Slow { latency_ms } => write!(f, "SLOW ({}ms)", latency_ms),
            HealthStatus::Down => write!(f, "DOWN"),
            HealthStatus::Unknown => write!(f, "N/A"),
        }
    }
}

#[allow(dead_code)]
pub fn check_process(server: &ServerEntry) -> HealthStatus {
    let sys = System::new_all();
    let cmd_name = std::path::Path::new(&server.command)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&server.command);

    // Check if any process matches the command name directly
    for process in sys.processes().values() {
        let proc_name = process.name().to_string_lossy();
        if proc_name.contains(cmd_name) || proc_name == cmd_name {
            return HealthStatus::Unknown;
        }
    }

    // For script runners (npx/node/python), check if server name appears in process args
    if server.command == "npx" || server.command == "node" || server.command == "python" {
        for process in sys.processes().values() {
            let cmd_line: String = process.cmd().iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" ");
            if cmd_line.contains(&server.name) {
                return HealthStatus::Unknown;
            }
        }
    }

    HealthStatus::Down
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{ClientType, ServerStatus};
    use std::collections::HashMap;

    fn make_server(name: &str, cmd: &str) -> ServerEntry {
        ServerEntry {
            name: name.to_string(),
            command: cmd.to_string(),
            args: vec![],
            env: HashMap::new(),
            source_client: ClientType::ClaudeDesktop,
            source_path: std::path::PathBuf::from("/fake"),
            status: ServerStatus::Active,
        }
    }

    #[test]
    fn test_check_nonexistent_process() {
        let server = make_server("test", "nonexistent_binary_xyz_12345");
        let result = check_process(&server);
        assert_eq!(result, HealthStatus::Down);
    }

    #[test]
    fn test_health_status_display() {
        assert_eq!(format!("{}", HealthStatus::Healthy { latency_ms: 50 }), "OK (50ms)");
        assert_eq!(format!("{}", HealthStatus::Down), "DOWN");
        assert_eq!(format!("{}", HealthStatus::Unknown), "N/A");
    }
}
