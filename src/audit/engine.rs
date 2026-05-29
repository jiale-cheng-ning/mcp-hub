use crate::audit::rules::{
    check_duplicates, check_env_secrets, check_permissions, check_version_pinning, Finding,
};
use crate::config::model::ServerEntry;

#[allow(dead_code)]
pub fn run_audit(servers: &[ServerEntry]) -> Vec<Finding> {
    let mut all_findings = Vec::new();
    for server in servers {
        all_findings.extend(check_env_secrets(server));
        all_findings.extend(check_permissions(server));
        all_findings.extend(check_version_pinning(server));
    }
    all_findings.extend(check_duplicates(servers));
    all_findings
}
