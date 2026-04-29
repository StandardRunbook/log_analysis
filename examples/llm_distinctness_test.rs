//! Diagnostic: how many distinct patterns does the configured LLM produce,
//! given one example log per ground-truth event type?
//!
//! Bypasses the matcher entirely — calls the LLM on each log and content-hashes
//! the canonical pattern. If the model produces ~43 distinct hashes for 43
//! distinct event types, then the model is fine and any merging seen in the
//! end-to-end test is the matcher's fragment-overlap threshold being too loose.
//! If the model produces far fewer, the model/prompt is doing the merging.
//!
//! Run with: cargo run --example llm_distinctness_test --release

use log_analyzer::llm_config::MultiLLMConfig;
use log_analyzer::llm_service::LLMServiceClient;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};

fn load_one_log_per_event(csv_path: &str) -> BTreeMap<String, String> {
    let mut by_event: BTreeMap<String, String> = BTreeMap::new();

    if let Ok(file) = File::open(csv_path) {
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        lines.next(); // header

        for line in lines.map_while(Result::ok) {
            let parts: Vec<&str> = line.splitn(11, ',').collect();
            if parts.len() < 10 {
                continue;
            }
            // Reconstruct a log line: Logrecord Date Time Pid Level Component ADDR Content
            let logrecord = parts[1];
            let date = parts[2];
            let time = parts[3];
            let pid = parts[4];
            let level = parts[5];
            let component = parts[6];
            let addr = parts[7];
            let content = parts[8].trim_matches('"');
            let event_id = parts[9].to_string();

            let log_line = format!(
                "{} {} {} {} {} {} [{}] {}",
                logrecord, date, time, pid, level, component, addr, content
            );

            by_event.entry(event_id).or_insert(log_line);
        }
    }

    by_event
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let by_event = load_one_log_per_event("data/loghub/OpenStack/OpenStack_2k.log_structured.csv");
    println!("Loaded {} distinct event types", by_event.len());

    let config = MultiLLMConfig::from_env();
    let provider = &config.providers[0];
    println!(
        "Provider: {}, model: {}\n",
        provider.provider, provider.model
    );

    let client = LLMServiceClient::new_with_config(config)?;

    // event_id → template_id (content hash) — same hash means model produced
    // structurally-identical canonical patterns.
    let mut event_to_tid: BTreeMap<String, u64> = BTreeMap::new();
    let mut event_to_pattern: BTreeMap<String, String> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();

    for (event_id, log_line) in &by_event {
        match client.generate_template(log_line).await {
            Ok(t) => {
                event_to_tid.insert(event_id.clone(), t.template_id);
                event_to_pattern.insert(event_id.clone(), t.pattern);
            }
            Err(e) => {
                failures.push(format!("{}: {}", event_id, e));
            }
        }
    }

    let distinct_tids: BTreeSet<u64> = event_to_tid.values().copied().collect();

    println!("\n=== Results ===");
    println!("Event types fed to LLM:    {}", by_event.len());
    println!("Successful generations:    {}", event_to_tid.len());
    println!("Failed generations:        {}", failures.len());
    println!(
        "Distinct template_ids:     {}  ← this is what we're measuring",
        distinct_tids.len()
    );
    println!(
        "Discrimination ratio:      {:.0}% ({} distinct / {} input event types)",
        100.0 * distinct_tids.len() as f64 / by_event.len() as f64,
        distinct_tids.len(),
        by_event.len()
    );

    // Group event_ids by template_id to see which different events collapsed
    let mut tid_to_events: BTreeMap<u64, Vec<String>> = BTreeMap::new();
    for (event, tid) in &event_to_tid {
        tid_to_events.entry(*tid).or_default().push(event.clone());
    }

    let collisions: Vec<_> = tid_to_events
        .iter()
        .filter(|(_, events)| events.len() > 1)
        .collect();

    if collisions.is_empty() {
        println!("\n✅ No collisions — every event type produced a distinct pattern.");
    } else {
        println!(
            "\n⚠️  {} template_id(s) shared across multiple event types:",
            collisions.len()
        );
        for (tid, events) in &collisions {
            println!("\n  template_id {} groups {} events:", tid, events.len());
            for e in events.iter().take(5) {
                if let Some(p) = event_to_pattern.get(e) {
                    println!("    [{}] pattern: {}", e, p);
                }
            }
        }
    }

    if !failures.is_empty() {
        println!("\nFailures:");
        for f in &failures {
            println!("  {}", f);
        }
    }

    Ok(())
}
