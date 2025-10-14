// Zero-copy matcher with hand-written parsers and arena allocation
// No regex, no HashMap allocations - maximum speed!

use aho_corasick::AhoCorasick;
use arc_swap::ArcSwap;
use bumpalo::Bump;
use im::HashMap as ImHashMap;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct LogTemplate {
    pub template_id: u64,
    pub pattern_type: PatternType,
    pub prefix: String,
    pub example: String,
}

#[derive(Debug, Clone)]
pub enum PatternType {
    // pattern: "cpu_usage: {decimal}% - {rest}"
    CpuUsage,
    // pattern: "memory_usage: {decimal}GB - {rest}"
    MemoryUsage,
    // pattern: "disk_io: {integer}MB/s - {rest}"
    DiskIo,
    // pattern: "network_traffic: {integer}Mbps - {rest}"
    NetworkTraffic,
    // pattern: "error_rate: {decimal}% - {rest}"
    ErrorRate,
    // pattern: "request_latency: {integer}ms - {rest}"
    RequestLatency,
    // pattern: "database_connections: {integer} - {rest}"
    DatabaseConnections,
}

/// Zero-copy result using string slices from original input
#[derive(Debug)]
pub struct MatchResult<'a> {
    pub matched: bool,
    pub template_id: Option<u64>,
    pub values: &'a [(&'a str, &'a str)], // Slices into original string
}

#[derive(Clone)]
struct MatcherSnapshot {
    ac: Arc<AhoCorasick>,
    pattern_to_template: ImHashMap<usize, LogTemplate>,
    next_id: u64,
}

impl MatcherSnapshot {
    fn new() -> Self {
        Self {
            ac: Arc::new(AhoCorasick::new(&[""] as &[&str]).unwrap()),
            pattern_to_template: ImHashMap::new(),
            next_id: 1,
        }
    }

    fn add_template(mut self, template: LogTemplate) -> Self {
        let pattern_idx = self.pattern_to_template.len();
        self.pattern_to_template
            .insert(pattern_idx, template.clone());

        let prefixes: Vec<String> = self
            .pattern_to_template
            .values()
            .map(|t| t.prefix.clone())
            .collect();

        if let Ok(ac) = AhoCorasick::new(&prefixes) {
            self.ac = Arc::new(ac);
        }

        self
    }

    /// Zero-copy matching with hand-written parsers
    fn match_log<'a>(&self, log_line: &'a str, arena: &'a Bump) -> MatchResult<'a> {
        let matches: Vec<_> = self.ac.find_iter(log_line).collect();

        for mat in matches {
            if let Some(template) = self.pattern_to_template.get(&mat.pattern().as_usize()) {
                // Hand-written parser - no regex!
                if let Some(values) = parse_with_pattern(log_line, &template.pattern_type, arena) {
                    return MatchResult {
                        matched: true,
                        template_id: Some(template.template_id),
                        values,
                    };
                }
            }
        }

        MatchResult {
            matched: false,
            template_id: None,
            values: &[],
        }
    }

    fn get_all_templates(&self) -> Vec<LogTemplate> {
        self.pattern_to_template.values().cloned().collect()
    }
}

/// Hand-written parser - MUCH faster than regex
fn parse_with_pattern<'a>(
    log: &'a str,
    pattern: &PatternType,
    arena: &'a Bump,
) -> Option<&'a [(&'a str, &'a str)]> {
    match pattern {
        PatternType::CpuUsage => parse_cpu_usage(log, arena),
        PatternType::MemoryUsage => parse_memory_usage(log, arena),
        PatternType::DiskIo => parse_disk_io(log, arena),
        PatternType::NetworkTraffic => parse_network_traffic(log, arena),
        PatternType::ErrorRate => parse_error_rate(log, arena),
        PatternType::RequestLatency => parse_request_latency(log, arena),
        PatternType::DatabaseConnections => parse_database_connections(log, arena),
    }
}

/// Parse "cpu_usage: 67.8% - Server load high"
#[inline(always)]
fn parse_cpu_usage<'a>(log: &'a str, arena: &'a Bump) -> Option<&'a [(&'a str, &'a str)]> {
    let bytes = log.as_bytes();

    // Fast prefix check
    if !bytes.starts_with(b"cpu_usage: ") {
        return None;
    }

    let mut pos = 11; // len("cpu_usage: ")

    // Parse decimal number
    let num_start = pos;
    while pos < bytes.len() && (bytes[pos].is_ascii_digit() || bytes[pos] == b'.') {
        pos += 1;
    }

    if pos == num_start || pos >= bytes.len() || bytes[pos] != b'%' {
        return None;
    }

    let percentage = &log[num_start..pos];
    pos += 1; // skip '%'

    // Skip " - "
    if pos + 3 > bytes.len() || &bytes[pos..pos + 3] != b" - " {
        return None;
    }
    pos += 3;

    let message = &log[pos..];

    // Allocate in arena (zero-copy slice storage)
    let values = arena.alloc_slice_copy(&[("percentage", percentage), ("message", message)]);
    Some(values)
}

/// Parse "memory_usage: 2.5GB - Memory stable"
#[inline(always)]
fn parse_memory_usage<'a>(log: &'a str, arena: &'a Bump) -> Option<&'a [(&'a str, &'a str)]> {
    let bytes = log.as_bytes();

    if !bytes.starts_with(b"memory_usage: ") {
        return None;
    }

    let mut pos = 14;
    let num_start = pos;

    while pos < bytes.len() && (bytes[pos].is_ascii_digit() || bytes[pos] == b'.') {
        pos += 1;
    }

    if pos == num_start || pos + 2 >= bytes.len() || &bytes[pos..pos + 2] != b"GB" {
        return None;
    }

    let amount = &log[num_start..pos];
    pos += 2;

    if pos + 3 > bytes.len() || &bytes[pos..pos + 3] != b" - " {
        return None;
    }
    pos += 3;

    let message = &log[pos..];
    let values = arena.alloc_slice_copy(&[("amount", amount), ("message", message)]);
    Some(values)
}

/// Parse "disk_io: 250MB/s - Disk active"
#[inline(always)]
fn parse_disk_io<'a>(log: &'a str, arena: &'a Bump) -> Option<&'a [(&'a str, &'a str)]> {
    let bytes = log.as_bytes();

    if !bytes.starts_with(b"disk_io: ") {
        return None;
    }

    let mut pos = 9;
    let num_start = pos;

    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }

    if pos == num_start || pos + 4 >= bytes.len() || &bytes[pos..pos + 4] != b"MB/s" {
        return None;
    }

    let throughput = &log[num_start..pos];
    pos += 4;

    if pos + 3 > bytes.len() || &bytes[pos..pos + 3] != b" - " {
        return None;
    }
    pos += 3;

    let message = &log[pos..];
    let values = arena.alloc_slice_copy(&[("throughput", throughput), ("message", message)]);
    Some(values)
}

/// Parse "network_traffic: 500Mbps - Network load"
#[inline(always)]
fn parse_network_traffic<'a>(log: &'a str, arena: &'a Bump) -> Option<&'a [(&'a str, &'a str)]> {
    let bytes = log.as_bytes();

    if !bytes.starts_with(b"network_traffic: ") {
        return None;
    }

    let mut pos = 17;
    let num_start = pos;

    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }

    if pos == num_start || pos + 4 >= bytes.len() || &bytes[pos..pos + 4] != b"Mbps" {
        return None;
    }

    let throughput = &log[num_start..pos];
    pos += 4;

    if pos + 3 > bytes.len() || &bytes[pos..pos + 3] != b" - " {
        return None;
    }
    pos += 3;

    let message = &log[pos..];
    let values = arena.alloc_slice_copy(&[("throughput", throughput), ("message", message)]);
    Some(values)
}

/// Parse "error_rate: 0.05% - System healthy"
#[inline(always)]
fn parse_error_rate<'a>(log: &'a str, arena: &'a Bump) -> Option<&'a [(&'a str, &'a str)]> {
    let bytes = log.as_bytes();

    if !bytes.starts_with(b"error_rate: ") {
        return None;
    }

    let mut pos = 12;
    let num_start = pos;

    while pos < bytes.len() && (bytes[pos].is_ascii_digit() || bytes[pos] == b'.') {
        pos += 1;
    }

    if pos == num_start || pos >= bytes.len() || bytes[pos] != b'%' {
        return None;
    }

    let rate = &log[num_start..pos];
    pos += 1;

    if pos + 3 > bytes.len() || &bytes[pos..pos + 3] != b" - " {
        return None;
    }
    pos += 3;

    let message = &log[pos..];
    let values = arena.alloc_slice_copy(&[("rate", rate), ("message", message)]);
    Some(values)
}

/// Parse "request_latency: 125ms - Response fast"
#[inline(always)]
fn parse_request_latency<'a>(log: &'a str, arena: &'a Bump) -> Option<&'a [(&'a str, &'a str)]> {
    let bytes = log.as_bytes();

    if !bytes.starts_with(b"request_latency: ") {
        return None;
    }

    let mut pos = 17;
    let num_start = pos;

    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }

    if pos == num_start || pos + 2 >= bytes.len() || &bytes[pos..pos + 2] != b"ms" {
        return None;
    }

    let latency = &log[num_start..pos];
    pos += 2;

    if pos + 3 > bytes.len() || &bytes[pos..pos + 3] != b" - " {
        return None;
    }
    pos += 3;

    let message = &log[pos..];
    let values = arena.alloc_slice_copy(&[("latency", latency), ("message", message)]);
    Some(values)
}

/// Parse "database_connections: 45 - Pool healthy"
#[inline(always)]
fn parse_database_connections<'a>(
    log: &'a str,
    arena: &'a Bump,
) -> Option<&'a [(&'a str, &'a str)]> {
    let bytes = log.as_bytes();

    if !bytes.starts_with(b"database_connections: ") {
        return None;
    }

    let mut pos = 22;
    let num_start = pos;

    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }

    if pos == num_start {
        return None;
    }

    let count = &log[num_start..pos];

    if pos + 3 > bytes.len() || &bytes[pos..pos + 3] != b" - " {
        return None;
    }
    pos += 3;

    let message = &log[pos..];
    let values = arena.alloc_slice_copy(&[("count", count), ("message", message)]);
    Some(values)
}

/// Zero-copy matcher with hand-written parsers
pub struct ZeroCopyMatcher {
    snapshot: ArcSwap<MatcherSnapshot>,
    cache: Arc<Mutex<LruCache<String, u64>>>,
}

impl ZeroCopyMatcher {
    pub fn new(cache_size: usize) -> Self {
        let mut snapshot = MatcherSnapshot::new();

        let default_templates = vec![
            LogTemplate {
                template_id: 1,
                pattern_type: PatternType::CpuUsage,
                prefix: "cpu_usage: ".to_string(),
                example: "cpu_usage: 45.2% - Server load normal".to_string(),
            },
            LogTemplate {
                template_id: 2,
                pattern_type: PatternType::MemoryUsage,
                prefix: "memory_usage: ".to_string(),
                example: "memory_usage: 2.5GB - Memory consumption stable".to_string(),
            },
            LogTemplate {
                template_id: 3,
                pattern_type: PatternType::DiskIo,
                prefix: "disk_io: ".to_string(),
                example: "disk_io: 250MB/s - Disk activity moderate".to_string(),
            },
        ];

        for template in default_templates {
            snapshot = snapshot.add_template(template);
        }

        Self {
            snapshot: ArcSwap::new(Arc::new(snapshot)),
            cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(cache_size).unwrap(),
            ))),
        }
    }

    /// Zero-copy matching with arena allocation
    pub fn match_log<'a>(&self, log_line: &'a str, arena: &'a Bump) -> MatchResult<'a> {
        let snapshot = self.snapshot.load();
        snapshot.match_log(log_line, arena)
    }

    pub fn add_template(&self, template: LogTemplate) {
        self.snapshot.rcu(|old_snapshot| {
            let new_snapshot = (**old_snapshot).clone().add_template(template.clone());
            Arc::new(new_snapshot)
        });
    }

    pub fn get_all_templates(&self) -> Vec<LogTemplate> {
        let snapshot = self.snapshot.load();
        snapshot.get_all_templates()
    }
}

impl Clone for ZeroCopyMatcher {
    fn clone(&self) -> Self {
        Self {
            snapshot: ArcSwap::new(self.snapshot.load_full()),
            cache: Arc::clone(&self.cache),
        }
    }
}
