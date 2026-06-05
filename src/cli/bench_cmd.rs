use crate::config::discover::discover;
use crate::config::model::ServerStatus;
use crate::health::mcp_check::{self, BenchResult};
use std::time::Duration;

pub fn run(server_filter: Option<&str>, rounds: usize, json: bool, timeout_secs: u64) {
    let entries = discover().unwrap_or_else(|e| {
        eprintln!("Warning: {}", e);
        Vec::new()
    });

    if entries.is_empty() {
        println!("No MCP servers found to benchmark.");
        return;
    }

    let filtered: Vec<_> = entries
        .iter()
        .filter(|s| {
            if let Some(filter) = server_filter {
                s.name.to_lowercase().contains(&filter.to_lowercase())
            } else {
                true
            }
        })
        .collect();

    if filtered.is_empty() {
        println!("No servers match filter '{}'.", server_filter.unwrap_or(""));
        return;
    }

    let timeout = Duration::from_secs(timeout_secs);

    if !json {
        println!(
            "Benchmarking {} MCP server(s), {} round(s) each...\n",
            filtered.len(),
            rounds
        );
    }

    let mut all_results: Vec<Vec<BenchResult>> = Vec::new();

    for server in &filtered {
        if matches!(server.status, ServerStatus::ParseError(_)) || server.command.is_empty() {
            if !json {
                println!("  {:<20} — SKIPPED (invalid config)", server.name);
            }
            continue;
        }

        let mut server_rounds = Vec::new();
        for _ in 0..rounds {
            let result = mcp_check::bench_server(
                &server.name,
                &server.command,
                &server.args,
                &server.env,
                timeout,
            );
            server_rounds.push(result);
        }

        if !json {
            print_bench_result(&server_rounds);
        }
        all_results.push(server_rounds);
    }

    if json {
        let json_output: Vec<serde_json::Value> = all_results
            .iter()
            .map(|rounds| {
                let first = &rounds[0];
                let spawn_avg = avg_field(rounds, |r| r.spawn_ms);
                let init_avg = avg_field(rounds, |r| r.init_ms);
                let tools_avg = avg_field(rounds, |r| r.tools_list_ms);
                let total_avg = avg_field(rounds, |r| r.total_ms);
                let total_min = rounds.iter().map(|r| r.total_ms).min().unwrap_or(0);
                let total_max = rounds.iter().map(|r| r.total_ms).max().unwrap_or(0);

                serde_json::json!({
                    "server": first.server_name,
                    "status": first.status,
                    "tools_count": first.tools_count,
                    "rounds": rounds.len(),
                    "spawn_ms": {"avg": spawn_avg},
                    "init_ms": {"avg": init_avg, "min": rounds.iter().map(|r| r.init_ms).min(), "max": rounds.iter().map(|r| r.init_ms).max()},
                    "tools_list_ms": {"avg": tools_avg},
                    "total_ms": {"avg": total_avg, "min": total_min, "max": total_max},
                    "error": first.error,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json_output).unwrap_or_default()
        );
    } else if !all_results.is_empty() {
        print_summary(&all_results);
    }
}

fn print_bench_result(rounds: &[BenchResult]) {
    let first = &rounds[0];
    if first.status == "failed" || first.status == "init_error" {
        println!(
            "  {:<20} spawn: {}ms  — {} ({})",
            first.server_name,
            first.spawn_ms,
            first.status.to_uppercase(),
            first.error.as_deref().unwrap_or("unknown")
        );
        return;
    }
    if first.status == "timeout" {
        println!(
            "  {:<20} spawn: {}ms  init: TIMEOUT — SKIPPED",
            first.server_name, first.spawn_ms
        );
        return;
    }

    let spawn_avg = avg_field(rounds, |r| r.spawn_ms);
    let init_avg = avg_field(rounds, |r| r.init_ms);
    let tools_avg = avg_field(rounds, |r| r.tools_list_ms);
    let total_avg = avg_field(rounds, |r| r.total_ms);
    let tools_count = first.tools_count;

    println!(
        "  {:<20} spawn: {:>4}ms  init: {:>4}ms  tools/list: {:>4}ms  total: {:>4}ms  ({} tools)",
        first.server_name, spawn_avg, init_avg, tools_avg, total_avg, tools_count
    );
}

fn print_summary(all_results: &[Vec<BenchResult>]) {
    let successful: Vec<_> = all_results
        .iter()
        .filter(|r| r.first().is_some_and(|f| f.status == "ok"))
        .collect();

    if successful.is_empty() {
        println!("\nNo servers responded successfully.");
        return;
    }

    let mut by_total: Vec<_> = successful
        .iter()
        .map(|r| {
            let avg = avg_field(r, |x| x.total_ms);
            (&r[0].server_name, avg)
        })
        .collect();
    by_total.sort_by_key(|(_, avg)| *avg);

    println!("\n  Summary:");
    if let Some((name, ms)) = by_total.first() {
        println!("    Fastest:  {} ({}ms avg)", name, ms);
    }
    if let Some((name, ms)) = by_total.last() {
        println!("    Slowest:  {} ({}ms avg)", name, ms);
    }
}

fn avg_field(rounds: &[BenchResult], f: impl Fn(&BenchResult) -> u64) -> u64 {
    if rounds.is_empty() {
        return 0;
    }
    let sum: u64 = rounds.iter().map(&f).sum();
    sum / rounds.len() as u64
}
