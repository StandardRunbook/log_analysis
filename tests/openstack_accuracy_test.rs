// OpenStack log grouping accuracy test
// Uses the LLM-based service to parse logs and compares with ground truth

use log_analyzer::llm_service::LLMServiceClient;
use log_analyzer::log_matcher::LogMatcher;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

fn load_ground_truth_structured(path: &str) -> Vec<(String, String)> {
    let mut structured = Vec::new();

    if let Ok(file) = File::open(path) {
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // Skip header
        lines.next();

        for line in lines {
            if let Ok(line) = line {
                // Parse CSV to extract full log line and EventId
                // Format: LineId,Logrecord,Date,Time,Pid,Level,Component,ADDR,Content,EventId,EventTemplate
                let parts: Vec<&str> = line.splitn(11, ',').collect();
                if parts.len() >= 10 {
                    // Logrecord is at index 1 (full log line), EventId at index 9
                    let log_line = parts[1].trim_matches('"').to_string();
                    let event_id = parts[9].to_string();
                    structured.push((log_line, event_id));
                }
            }
        }
    }

    structured
}

fn load_raw_logs(path: &str) -> Vec<String> {
    let mut logs = Vec::new();

    if let Ok(file) = File::open(path) {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(line) = line {
                logs.push(line);
            }
        }
    }

    logs
}

#[tokio::test]
async fn test_openstack_grouping_accuracy() {
    println!("\n{}", "=".repeat(80));
    println!("📊 OpenStack Log Grouping Accuracy Test");
    println!("    Using LLM-based Template Generation");
    println!("{}\n", "=".repeat(80));

    let data_dir = "data/loghub/OpenStack";

    println!("📝 Loading dataset...");
    let gt_structured =
        load_ground_truth_structured(&format!("{}/OpenStack_2k.log_structured.csv", data_dir));
    let raw_logs = load_raw_logs(&format!("{}/OpenStack_2k.log", data_dir));

    println!(
        "   ✓ Loaded {} structured log entries (ground truth)",
        gt_structured.len()
    );
    println!("   ✓ Loaded {} raw log lines\n", raw_logs.len());

    // Initialize the LLM service and matcher with Ollama (using smaller, faster model)
    let llm_client = LLMServiceClient::new(
        "ollama".to_string(),
        "".to_string(),
        "llama3:latest".to_string(), // 4.7GB - much faster than qwen3-coder
    );

    let matcher = Arc::new(RwLock::new(LogMatcher::new()));

    // Use first 100 logs for quick testing (set to raw_logs.len() for full test)
    let test_size = std::env::var("TEST_LOGS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let test_logs = &raw_logs[..test_size.min(raw_logs.len())];
    let test_gt = &gt_structured[..test_size.min(gt_structured.len())];

    println!(
        "⚡ Parsing {} logs with LLM-based template generation...",
        test_logs.len()
    );
    println!("   (This will automatically generate templates for unknown patterns)");
    println!("   (Set TEST_LOGS=2000 environment variable for full test)\n");

    let start = Instant::now();

    let mut template_assignments: Vec<Option<u64>> = Vec::new();
    let mut generated_templates = 0;

    for (idx, log_line) in test_logs.iter().enumerate() {
        if idx % 10 == 0 && idx > 0 {
            println!("   Processed {}/{} logs...", idx, test_logs.len());
        }

        // Try to match with existing templates
        let match_result = {
            let m = matcher.read().await;
            m.match_log(log_line)
        };

        let template_id = if let Some(tid) = match_result {
            // Matched existing template
            Some(tid)
        } else {
            // Generate new template via LLM
            match llm_client.generate_template(log_line).await {
                Ok(new_template) => {
                    let tid = new_template.template_id;
                    {
                        let mut m = matcher.write().await;
                        m.add_template(new_template);
                    }
                    generated_templates += 1;
                    Some(tid)
                }
                Err(_) => None,
            }
        };

        template_assignments.push(template_id);
    }

    let elapsed = start.elapsed();

    println!("   ✓ Processed all {} logs\n", test_logs.len());

    let throughput = test_logs.len() as f64 / elapsed.as_secs_f64();
    let avg_latency_ms = (elapsed.as_millis() as f64) / (test_logs.len() as f64);

    println!("📈 Performance Metrics:");
    println!("   Total logs:              {:>10}", test_logs.len());
    println!("   Templates generated:     {:>10}", generated_templates);
    println!(
        "   Parse time:              {:>10.2}s",
        elapsed.as_secs_f64()
    );
    println!("   Throughput:              {:>10.0} logs/sec", throughput);
    println!(
        "   Avg latency:             {:>10.2}ms per log\n",
        avg_latency_ms
    );

    // Calculate grouping accuracy
    println!("🎯 Calculating Grouping Accuracy...");
    println!("   (Comparing our groupings with ground truth)\n");

    // Build mapping: for each ground truth event_id, which template_ids did we assign?
    let mut gt_to_predicted: HashMap<String, Vec<u64>> = HashMap::new();

    for (idx, template_id) in template_assignments.iter().enumerate() {
        if let Some((_log_line, gt_event_id)) = test_gt.get(idx) {
            if let Some(tid) = template_id {
                gt_to_predicted
                    .entry(gt_event_id.clone())
                    .or_insert_with(Vec::new)
                    .push(*tid);
            }
        }
    }

    // For each ground truth group, find the most common template_id we assigned (majority vote)
    let mut gt_to_majority_template: HashMap<String, u64> = HashMap::new();

    for (gt_event, template_ids) in &gt_to_predicted {
        let mut counts: HashMap<u64, usize> = HashMap::new();
        for tid in template_ids {
            *counts.entry(*tid).or_insert(0) += 1;
        }

        if let Some((&majority_tid, _)) = counts.iter().max_by_key(|&(_, count)| count) {
            gt_to_majority_template.insert(gt_event.clone(), majority_tid);
        }
    }

    // Calculate accuracy: how many logs are assigned to the majority template for their ground truth group?
    let mut correct = 0;
    let mut total = 0;

    for (idx, template_id) in template_assignments.iter().enumerate() {
        if let Some((_log_line, gt_event_id)) = test_gt.get(idx) {
            if let Some(&majority_tid) = gt_to_majority_template.get(gt_event_id) {
                if let Some(tid) = template_id {
                    total += 1;
                    if *tid == majority_tid {
                        correct += 1;
                    }
                }
            }
        }
    }

    let grouping_accuracy = if total > 0 {
        (correct as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    // Count unique ground truth groups vs our templates
    let unique_gt_groups = test_gt
        .iter()
        .map(|(_, event_id)| event_id)
        .collect::<std::collections::HashSet<_>>()
        .len();

    println!("📊 Grouping Results:");
    println!("   Ground truth groups:     {:>10}", unique_gt_groups);
    println!("   Generated templates:     {:>10}", generated_templates);
    println!(
        "   Template ratio:          {:>10.2}x",
        generated_templates as f64 / unique_gt_groups as f64
    );
    println!();
    println!("   Total logs evaluated:    {:>10}", total);
    println!(
        "   Correctly grouped:       {:>10} ({:.1}%)",
        correct,
        (correct as f64 / total as f64) * 100.0
    );
    println!(
        "   Incorrectly grouped:     {:>10} ({:.1}%)",
        total - correct,
        ((total - correct) as f64 / total as f64) * 100.0
    );
    println!();
    println!("   🎯 Grouping Accuracy:     {:>9.2}%", grouping_accuracy);
    println!();

    // Show ground truth groups and how we split them
    println!("🔍 Top ground truth groups and our template assignments:");
    let mut gt_group_sizes: Vec<_> = gt_to_predicted.iter().collect();
    gt_group_sizes.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    for (gt_event, template_ids) in gt_group_sizes.iter().take(10) {
        let mut counts: HashMap<u64, usize> = HashMap::new();
        for tid in template_ids.iter() {
            *counts.entry(*tid).or_insert(0) += 1;
        }

        println!("   {} ({} logs):", gt_event, template_ids.len());
        let mut sorted_counts: Vec<_> = counts.iter().collect();
        sorted_counts.sort_by(|a, b| b.1.cmp(a.1));

        for (tid, count) in sorted_counts.iter().take(3) {
            let pct = (**count as f64 / template_ids.len() as f64) * 100.0;
            println!("      Template {}: {} logs ({:.1}%)", tid, count, pct);
        }
    }

    println!("\n{}", "=".repeat(80));
    println!("✅ Grouping Accuracy Test Complete!");
    println!("{}", "=".repeat(80));

    // Assert reasonable grouping accuracy (skip if no data)
    if raw_logs.len() > 0 {
        assert!(
            grouping_accuracy > 70.0,
            "Grouping accuracy should be > 70%, got {:.2}%",
            grouping_accuracy
        );
    } else {
        println!("⚠️  Skipping accuracy assertion - no data files found");
        println!("   Place OpenStack dataset in data/loghub/OpenStack/");
    }
}
