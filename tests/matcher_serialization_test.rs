/// Tests for LogMatcher serialization/deserialization
///
/// This demonstrates saving and loading the matcher state,
/// which is useful for:
/// - Caching generated templates from Ollama
/// - Distributing pre-built matchers
/// - Persisting learned patterns
use log_analyzer::log_matcher::{LogMatcher, LogTemplate};

#[test]
fn test_save_and_load_binary() {
    // Create matcher and add some templates
    let mut matcher = LogMatcher::new();

    matcher.add_template(LogTemplate {
        template_id: 100,
        pattern: r"(\d{4}-\d{2}-\d{2}) INFO (.+?) logged in".to_string(),
        variables: vec!["timestamp".to_string(), "username".to_string()],
        example: "2025-01-15 INFO alice logged in".to_string(),
    });

    matcher.add_template(LogTemplate {
        template_id: 101,
        pattern: r"ERROR: Connection to (.+?):(\d+) failed".to_string(),
        variables: vec!["host".to_string(), "port".to_string()],
        example: "ERROR: Connection to db.example.com:5432 failed".to_string(),
    });

    // Test matching before save
    let test_log = "2025-01-15 INFO bob logged in";
    let result_before = matcher.match_log(test_log);
    assert!(result_before.is_some(), "Should match before save");

    // Save to binary file
    let path = "test_matcher.bin";
    matcher.save_to_file(path).expect("Failed to save");

    // Load from binary file
    let loaded_matcher = LogMatcher::load_from_file(path).expect("Failed to load");

    // Test matching after load
    let result_after = loaded_matcher.match_log(test_log);
    assert!(result_after.is_some(), "Should match after load");
    assert_eq!(result_before, result_after, "Results should be identical");

    // Verify templates were preserved
    let templates = loaded_matcher.get_all_templates();
    assert!(templates.len() >= 2, "Should have at least 2 templates");

    // Cleanup
    std::fs::remove_file(path).ok();

    println!("✅ Binary serialization test passed");
}

#[test]
fn test_save_and_load_json() {
    // Create matcher and add templates
    let mut matcher = LogMatcher::new();

    matcher.add_template(LogTemplate {
        template_id: 200,
        pattern: r"Request (.+?) completed in (\d+)ms".to_string(),
        variables: vec!["request_id".to_string(), "duration".to_string()],
        example: "Request req_abc123 completed in 145ms".to_string(),
    });

    // Save to JSON file (human-readable)
    let path = "test_matcher.json";
    matcher.save_to_json(path).expect("Failed to save JSON");

    // Verify file exists and is readable
    let json_content = std::fs::read_to_string(path).expect("Failed to read JSON");
    assert!(
        json_content.contains("template_id"),
        "JSON should contain template_id"
    );
    assert!(
        json_content.contains("pattern"),
        "JSON should contain pattern"
    );

    // Load from JSON file
    let loaded_matcher = LogMatcher::load_from_json(path).expect("Failed to load JSON");

    // Test matching
    let test_log = "Request req_xyz789 completed in 230ms";
    let result = loaded_matcher.match_log(test_log);
    assert!(result.is_some(), "Should match loaded template");

    // Cleanup
    std::fs::remove_file(path).ok();

    println!("✅ JSON serialization test passed");
}

#[test]
fn test_preserves_all_template_data() {
    let mut matcher = LogMatcher::new();

    let original_template = LogTemplate {
        template_id: 300,
        pattern: r"cpu_usage: (\d+\.\d+)% - (.*)".to_string(),
        variables: vec!["percentage".to_string(), "message".to_string()],
        example: "cpu_usage: 45.2% - Server load normal".to_string(),
    };

    matcher.add_template(original_template.clone());

    // Save and load
    let path = "test_template_preservation.bin";
    matcher.save_to_file(path).unwrap();
    let loaded_matcher = LogMatcher::load_from_file(path).unwrap();

    // Get the template back
    let templates = loaded_matcher.get_all_templates();
    let loaded_template = templates
        .iter()
        .find(|t| t.template_id == 300)
        .expect("Template should exist");

    // Verify all fields preserved
    assert_eq!(loaded_template.template_id, original_template.template_id);
    assert_eq!(loaded_template.pattern, original_template.pattern);
    assert_eq!(loaded_template.variables, original_template.variables);
    assert_eq!(loaded_template.example, original_template.example);

    std::fs::remove_file(path).ok();

    println!("✅ Template data preservation test passed");
}

#[test]
fn test_aho_corasick_rebuilt_correctly() {
    // Create matcher with multiple templates
    let mut matcher = LogMatcher::new();

    for i in 0..10 {
        matcher.add_template(LogTemplate {
            template_id: 1000 + i,
            pattern: format!(r"Pattern{} (.+?) value: (\d+)", i),
            variables: vec!["field".to_string(), "value".to_string()],
            example: format!("Pattern{} test value: 123", i),
        });
    }

    // Test logs for each template
    let test_logs: Vec<String> = (0..10)
        .map(|i| format!("Pattern{} xyz value: 456", i))
        .collect();

    // Match before save
    let results_before: Vec<_> = test_logs.iter().map(|log| matcher.match_log(log)).collect();

    // Save and load
    let path = "test_aho_corasick.bin";
    matcher.save_to_file(path).unwrap();
    let loaded_matcher = LogMatcher::load_from_file(path).unwrap();

    // Match after load
    let results_after: Vec<_> = test_logs
        .iter()
        .map(|log| loaded_matcher.match_log(log))
        .collect();

    // All results should be identical
    assert_eq!(
        results_before, results_after,
        "Aho-Corasick matching should be identical"
    );

    // All should match
    for (i, result) in results_after.iter().enumerate() {
        assert!(result.is_some(), "Log {} should match", i);
    }

    std::fs::remove_file(path).ok();

    println!("✅ Aho-Corasick rebuild test passed");
}

#[test]
fn test_performance_binary_vs_json() {
    use std::time::Instant;

    // Create matcher with many templates
    let mut matcher = LogMatcher::new();
    for i in 0..100 {
        matcher.add_template(LogTemplate {
            template_id: 2000 + i,
            pattern: format!(r"Event{} (\d+) (.+)", i),
            variables: vec!["id".to_string(), "data".to_string()],
            example: format!("Event{} 123 test", i),
        });
    }

    // Binary save/load
    let bin_path = "perf_test.bin";
    let start = Instant::now();
    matcher.save_to_file(bin_path).unwrap();
    let bin_save_time = start.elapsed();

    let start = Instant::now();
    let _loaded1 = LogMatcher::load_from_file(bin_path).unwrap();
    let bin_load_time = start.elapsed();

    let bin_size = std::fs::metadata(bin_path).unwrap().len();

    // JSON save/load
    let json_path = "perf_test.json";
    let start = Instant::now();
    matcher.save_to_json(json_path).unwrap();
    let json_save_time = start.elapsed();

    let start = Instant::now();
    let _loaded2 = LogMatcher::load_from_json(json_path).unwrap();
    let json_load_time = start.elapsed();

    let json_size = std::fs::metadata(json_path).unwrap().len();

    println!("\n📊 Serialization Performance (100 templates):");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Binary:");
    println!("  Save: {:?}", bin_save_time);
    println!("  Load: {:?}", bin_load_time);
    println!("  Size: {} bytes", bin_size);
    println!("\nJSON:");
    println!("  Save: {:?}", json_save_time);
    println!("  Load: {:?}", json_load_time);
    println!("  Size: {} bytes", json_size);
    println!(
        "\nBinary is {:.1}x smaller",
        json_size as f64 / bin_size as f64
    );
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Cleanup
    std::fs::remove_file(bin_path).ok();
    std::fs::remove_file(json_path).ok();

    println!("✅ Performance comparison test passed");
}
