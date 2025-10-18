// Tests for the Aho-Corasick DFA-based log matcher
// Uses the actual LogMatcher implementation from src/log_matcher.rs

use log_analyzer::log_matcher::{LogMatcher, LogTemplate};

#[test]
fn test_aho_corasick_matching() {
    let matcher = LogMatcher::new();

    let log = "cpu_usage: 67.8% - Server load increased";
    let result = matcher.match_log(log);

    assert_eq!(result, Some(1));
}

#[test]
fn test_multi_pattern() {
    let matcher = LogMatcher::new();

    // Test all default patterns
    let test_cases = vec![
        ("cpu_usage: 50.0% - test", Some(1)),
        ("memory_usage: 2.5GB - test", Some(2)),
        ("disk_io: 100MB/s - test", Some(3)),
        ("unknown log format", None),
    ];

    for (log, expected) in test_cases {
        let result = matcher.match_log(log);
        assert_eq!(result, expected, "Failed to match: {}", log);
    }
}

#[test]
fn test_add_template() {
    let mut matcher = LogMatcher::new();

    // Add a new template
    let template = LogTemplate {
        template_id: 0, // Will be auto-assigned
        pattern: r"network_traffic: (\d+)Mbps - (.*)".to_string(),
        variables: vec!["bandwidth".to_string(), "message".to_string()],
        example: "network_traffic: 100Mbps - Network load moderate".to_string(),
    };

    matcher.add_template(template);

    // Test the new template
    let log = "network_traffic: 150Mbps - High traffic detected";
    let result = matcher.match_log(log);

    assert!(result.is_some());
}

#[test]
fn test_batch_matching() {
    let matcher = LogMatcher::new();

    let logs = vec![
        "cpu_usage: 45.2% - Server load normal",
        "memory_usage: 2.5GB - Memory consumption stable",
        "disk_io: 250MB/s - Disk activity moderate",
        "unknown log line",
    ];

    let results = matcher.match_batch(&logs);

    assert_eq!(results.len(), 4);
    assert_eq!(results[0], Some(1)); // cpu_usage
    assert_eq!(results[1], Some(2)); // memory_usage
    assert_eq!(results[2], Some(3)); // disk_io
    assert_eq!(results[3], None); // unknown
}

#[test]
fn test_get_all_templates() {
    let matcher = LogMatcher::new();
    let templates = matcher.get_all_templates();

    // Should have 3 default templates
    assert_eq!(templates.len(), 3);

    // Check template IDs
    let ids: Vec<u64> = templates.iter().map(|t| t.template_id).collect();
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));
    assert!(ids.contains(&3));
}
