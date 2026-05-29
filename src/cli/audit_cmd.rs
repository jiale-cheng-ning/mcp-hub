use crate::audit::engine::run_audit;
use crate::audit::report::{findings_to_json, print_report};
use crate::config::discover::discover;

pub fn run(json: bool) {
    let entries = discover().unwrap_or_else(|e| {
        eprintln!("Error discovering configs: {}", e);
        Vec::new()
    });

    if entries.is_empty() {
        println!("No MCP servers found to audit.");
        return;
    }

    let findings = run_audit(&entries);

    if json {
        println!("{}", findings_to_json(&findings));
    } else {
        print_report(&findings);
    }
}
