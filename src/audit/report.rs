use crate::audit::rules::Finding;

#[allow(dead_code)]
pub fn print_report(findings: &[Finding]) {
    if findings.is_empty() {
        println!("No issues found. All MCP server configs look clean.");
        return;
    }

    let critical: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == "Critical")
        .collect();
    let warnings: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == "Warning")
        .collect();
    let info: Vec<_> = findings.iter().filter(|f| f.severity == "Info").collect();

    if !critical.is_empty() {
        println!("CRITICAL ({})", critical.len());
        for f in &critical {
            println!("  |- {}: {}", f.server_name, f.message);
            println!("     Fix: {}", f.fix);
        }
        println!();
    }

    if !warnings.is_empty() {
        println!("WARNING ({})", warnings.len());
        for f in &warnings {
            println!("  |- {}: {}", f.server_name, f.message);
            println!("     Fix: {}", f.fix);
        }
        println!();
    }

    if !info.is_empty() {
        println!("INFO ({})", info.len());
        for f in &info {
            println!("  |- {}: {}", f.server_name, f.message);
            println!("     Fix: {}", f.fix);
        }
        println!();
    }

    println!("Total findings: {}", findings.len());
}

#[allow(dead_code)]
pub fn findings_to_json(findings: &[Finding]) -> String {
    serde_json::to_string_pretty(findings).unwrap_or_else(|_| "[]".to_string())
}
