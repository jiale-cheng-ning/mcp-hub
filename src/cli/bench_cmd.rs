use crate::config::discover::discover;
use crate::health::mcp_check::{self, BenchResult};
use std::time::Duration;

pub fn run(server_filter: Option<&str>, rounds: usize, json: bool, timeout_secs: u64) {
    if rounds == 0 || rounds > 100 {
        eprintln!("Rounds must be between 1 and 100");
        return;
    }
    let entries = match discover() {
        Ok(entries) => entries,
        Err(_) => {
            eprintln!("Unable to discover server configs");
            return;
        }
    };
    let handles: Vec<_> = entries
        .into_iter()
        .filter(|s| server_filter.is_none_or(|f| s.name.to_lowercase().contains(&f.to_lowercase())))
        .map(|entry| {
            std::thread::spawn(move || {
                (0..rounds)
                    .map(|_| mcp_check::bench_entry(&entry, Duration::from_secs(timeout_secs)))
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let mut output = Vec::new();
    for handle in handles {
        match handle.join() {
            Ok(results) => output.push(summarize(&results)),
            Err(_) => {
                output.push(serde_json::json!({"status":"failed","error":"probe worker failed"}))
            }
        }
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        for result in output {
            println!(
                "  {}: {} ({} successful, {} failed); total avg: {}ms; tools: {}",
                result["server"].as_str().unwrap_or("unknown"),
                result["status"].as_str().unwrap_or("failed"),
                result["successful_rounds"],
                result["failed_rounds"],
                result["total_ms"]["avg"],
                result["tools_count"]
            );
            if let Some(errors) = result["errors"].as_array() {
                for error in errors {
                    println!("    {}", error.as_str().unwrap_or("check failed"));
                }
            }
        }
    }
}

fn summarize(rounds: &[BenchResult]) -> serde_json::Value {
    let good: Vec<_> = rounds.iter().filter(|r| r.status == "ok").collect();
    let failures = rounds.len() - good.len();
    let status = if good.is_empty() {
        rounds
            .first()
            .filter(|first| rounds.iter().all(|r| r.status == first.status))
            .map(|first| first.status.as_str())
            .unwrap_or("failed")
    } else if failures > 0 {
        "partial"
    } else {
        "ok"
    };
    let metrics = |f: fn(&BenchResult) -> u64| {
        if good.is_empty() {
            return serde_json::Value::Null;
        }
        let values: Vec<_> = good.iter().map(|r| f(r)).collect();
        serde_json::json!({"avg":values.iter().sum::<u64>()/values.len() as u64,"min":values.iter().min(),"max":values.iter().max()})
    };
    serde_json::json!({
        "server":rounds.first().map(|r|r.server_name.as_str()).unwrap_or("unknown"),
        "status":status,"rounds":rounds.len(),"successful_rounds":good.len(),"failed_rounds":failures,
        "spawn_ms":metrics(|r|r.spawn_ms),"init_ms":metrics(|r|r.init_ms),
        "tools_list_ms":metrics(|r|r.tools_list_ms),"total_ms":metrics(|r|r.total_ms),
        "tools_count":good.last().map(|r|r.tools_count),
        "errors":rounds.iter().filter_map(|r|r.error.as_ref()).collect::<Vec<_>>(),
        "round_statuses":rounds.iter().map(|r|r.status.as_str()).collect::<Vec<_>>(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_rounds_use_only_successful_timings() {
        let mut good = BenchResult {
            server_name: "test".into(),
            spawn_ms: 1,
            init_ms: 2,
            tools_list_ms: 3,
            total_ms: 10,
            tools_count: 2,
            status: "ok".into(),
            error: None,
        };
        let mut bad = good.clone();
        bad.total_ms = 1000;
        bad.status = "failed".into();
        bad.error = Some("mock failure".into());
        let result = summarize(&[bad.clone(), good.clone()]);
        assert_eq!(result["status"], "partial");
        assert_eq!(result["total_ms"]["avg"], 10);
        good.status = "failed".into();
        assert!(summarize(&[bad, good])["total_ms"].is_null());
    }
}
