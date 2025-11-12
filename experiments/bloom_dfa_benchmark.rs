/// Benchmark comparing Bloom DFA vs regular Aho-Corasick
///
/// Run with: cargo run --release --example bloom_dfa_benchmark

use log_analyzer::bloom_dfa::BloomDFA;
use aho_corasick::AhoCorasick;
use std::time::Instant;

fn main() {
    println!("=== Bloom DFA vs Aho-Corasick Benchmark ===\n");

    // Generate test fragments (simulating Linux log patterns)
    let fragments = vec![
        "authentication failure; logname= uid=",
        " euid=",
        " tty=",
        " ruser=",
        " rhost=",
        "sshd(pam_unix)[",
        "]: ",
        " - ",
        "session opened for user ",
        "session closed for user ",
        "kernel: ",
        "systemd[",
        "Started ",
        "Stopped ",
        "Failed to start ",
    ];

    // Generate test logs
    let test_logs: Vec<String> = (0..1000).map(|i| {
        format!(
            "Jun 14 15:16:{:02} combo sshd(pam_unix)[{}]: authentication failure; logname= uid=0 euid=0 tty=NODEVssh ruser= rhost=192.168.1.{}",
            i % 60,
            19900 + i,
            i % 255
        )
    }).collect();

    println!("Test dataset:");
    println!("  Fragments: {}", fragments.len());
    println!("  Log lines: {}", test_logs.len());
    println!("  Avg log length: {:.1} chars\n",
        test_logs.iter().map(|s| s.len()).sum::<usize>() as f64 / test_logs.len() as f64);

    // Build Bloom DFA
    println!("Building Bloom DFA...");
    let start = Instant::now();
    let mut bloom_dfa = BloomDFA::new();
    for (idx, frag) in fragments.iter().enumerate() {
        bloom_dfa.add_pattern(frag, idx as u64);
    }
    let build_time = start.elapsed();
    println!("  Build time: {:?}", build_time);
    println!("  Nodes: {}", bloom_dfa.node_count());

    // Build Aho-Corasick
    println!("\nBuilding Aho-Corasick...");
    let start = Instant::now();
    let ac = AhoCorasick::new(&fragments).unwrap();
    let ac_build_time = start.elapsed();
    println!("  Build time: {:?}", ac_build_time);

    // Benchmark Bloom DFA
    println!("\n--- Bloom DFA Search ---");
    let start = Instant::now();
    let mut total_matches = 0;
    for log in &test_logs {
        let matches = bloom_dfa.search(log);
        total_matches += matches.len();
    }
    let bloom_time = start.elapsed();
    let bloom_throughput = (test_logs.len() as f64 / bloom_time.as_secs_f64()) as u64;

    println!("  Time: {:?}", bloom_time);
    println!("  Throughput: {} logs/sec", bloom_throughput);
    println!("  Total matches: {}", total_matches);
    println!("  Avg latency: {:.2}μs", bloom_time.as_micros() as f64 / test_logs.len() as f64);

    // Benchmark Aho-Corasick
    println!("\n--- Aho-Corasick Search ---");
    let start = Instant::now();
    let mut total_matches = 0;
    for log in &test_logs {
        let matches: Vec<_> = ac.find_iter(log).collect();
        total_matches += matches.len();
    }
    let ac_time = start.elapsed();
    let ac_throughput = (test_logs.len() as f64 / ac_time.as_secs_f64()) as u64;

    println!("  Time: {:?}", ac_time);
    println!("  Throughput: {} logs/sec", ac_throughput);
    println!("  Total matches: {}", total_matches);
    println!("  Avg latency: {:.2}μs", ac_time.as_micros() as f64 / test_logs.len() as f64);

    // Comparison
    println!("\n=== Results ===");
    let speedup = ac_time.as_secs_f64() / bloom_time.as_secs_f64();
    if speedup > 1.0 {
        println!("  Bloom DFA is {:.2}x FASTER than Aho-Corasick", speedup);
    } else {
        println!("  Aho-Corasick is {:.2}x FASTER than Bloom DFA", 1.0 / speedup);
    }

    // Memory estimation
    println!("\nMemory (rough estimate):");
    println!("  Bloom DFA nodes: {}", bloom_dfa.node_count());
    println!("  AC patterns: {}", fragments.len());

    // Test with more fragments (scalability)
    println!("\n=== Scalability Test (500 fragments) ===");
    let large_fragments: Vec<String> = (0..500).map(|i| format!("fragment_{}_", i)).collect();

    let start = Instant::now();
    let mut large_bloom = BloomDFA::new();
    for (idx, frag) in large_fragments.iter().enumerate() {
        large_bloom.add_pattern(frag, idx as u64);
    }
    println!("Bloom DFA build (500 patterns): {:?}", start.elapsed());

    let start = Instant::now();
    let large_ac = AhoCorasick::new(&large_fragments).unwrap();
    println!("AC build (500 patterns): {:?}", start.elapsed());

    // Search with large pattern set
    let test_log = "some fragment_123_ in the middle of fragment_456_ text";

    let start = Instant::now();
    for _ in 0..1000 {
        let _matches = large_bloom.search(test_log);
    }
    let bloom_large = start.elapsed();

    let start = Instant::now();
    for _ in 0..1000 {
        let _matches: Vec<_> = large_ac.find_iter(test_log).collect();
    }
    let ac_large = start.elapsed();

    println!("\n1000 searches with 500 patterns:");
    println!("  Bloom DFA: {:?} ({:.2}μs per search)", bloom_large, bloom_large.as_micros() as f64 / 1000.0);
    println!("  AC: {:?} ({:.2}μs per search)", ac_large, ac_large.as_micros() as f64 / 1000.0);

    let large_speedup = ac_large.as_secs_f64() / bloom_large.as_secs_f64();
    if large_speedup > 1.0 {
        println!("  Bloom DFA is {:.2}x FASTER at scale", large_speedup);
    } else {
        println!("  AC is {:.2}x FASTER at scale", 1.0 / large_speedup);
    }
}
