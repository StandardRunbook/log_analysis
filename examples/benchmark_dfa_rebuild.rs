use std::time::Instant;
use aho_corasick::AhoCorasick;

fn main() {
    println!("DFA Rebuild Time Benchmark");
    println!("==========================\n");

    // Simulate various template counts
    for template_count in [10, 50, 100, 500, 1000, 5000] {
        // Generate fragments (avg 3 fragments per template, 8 chars each)
        let fragments: Vec<String> = (0..template_count * 3)
            .map(|i| format!("fragment_{:08}", i))
            .collect();

        let fragment_strs: Vec<&str> = fragments.iter().map(|s| s.as_str()).collect();

        let start = Instant::now();
        let _ac = AhoCorasick::new(&fragment_strs).unwrap();
        let elapsed = start.elapsed();

        println!("{:5} templates ({:5} fragments): {:>8.2?}",
                 template_count, fragments.len(), elapsed);
    }

    println!("\n\nNow let's calculate opportunity cost:");
    println!("=========================================\n");

    // Real-world scenario: 100 templates
    let template_count = 100;
    let fragments: Vec<String> = (0..template_count * 3)
        .map(|i| format!("fragment_{:08}", i))
        .collect();
    let fragment_strs: Vec<&str> = fragments.iter().map(|s| s.as_str()).collect();

    let start = Instant::now();
    let _ac = AhoCorasick::new(&fragment_strs).unwrap();
    let rebuild_time = start.elapsed();

    // Throughput from benchmark: ~500K logs/sec
    let throughput_per_sec = 500_000.0;
    let rebuild_time_secs = rebuild_time.as_secs_f64();
    let missed_logs = (throughput_per_sec * rebuild_time_secs) as u64;

    println!("Scenario: 100 templates (typical production load)");
    println!("  DFA rebuild time: {:>8.2?}", rebuild_time);
    println!("  Matching throughput: {}/sec", throughput_per_sec as u64);
    println!("  Logs NOT processed during rebuild: {}", missed_logs);
    println!("  (These logs would queue up or be dropped)");
}
