use crate::audit::rules::{all_rules, load_all_data, AuditContext, Finding, Severity};

pub fn run_audit(servers: &[crate::config::model::ServerEntry]) -> Vec<Finding> {
    let (known, cve, dep) = load_all_data();
    let ctx = AuditContext {
        servers,
        known_servers: &known,
        cve_db: &cve,
        deprecated: &dep,
    };

    let rules = all_rules();
    let mut all_findings: Vec<Finding> = Vec::new();

    for rule in &rules {
        all_findings.extend(rule.check(&ctx));
    }

    // Sort by severity (Critical first), then by server name
    all_findings.sort_by(|a, b| {
        a.severity
            .priority()
            .cmp(&b.severity.priority())
            .then_with(|| a.server_name.cmp(&b.server_name))
    });

    all_findings
}

#[allow(dead_code)]
pub fn run_audit_with_filter(
    servers: &[crate::config::model::ServerEntry],
    min_severity: Option<Severity>,
) -> Vec<Finding> {
    let findings = run_audit(servers);
    match min_severity {
        Some(min) => findings
            .into_iter()
            .filter(|f| f.severity.priority() <= min.priority())
            .collect(),
        None => findings,
    }
}
