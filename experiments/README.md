# Log Matching Experiments

This folder contains experimental implementations that were explored to optimize log matching performance.

## Experiments

### 1. Demo Aho-Corasick (`demo_aho_corasick.rs`)

A simplified Aho-Corasick implementation with exposed internals for experimentation.

**Features**:
- Exposed AC automaton, fragment strings, and fragment-to-template mappings
- Debug methods: `get_all_matches()`, `get_template_fragments()`
- Simple matching without weighted scoring

**Purpose**: Created as a testbed for experimenting with modifications to the core AC algorithm.

---

### 2. Bloom Filter DFA (`bloom_dfa.rs` + `bloom_dfa_benchmark.rs`)

An experimental approach using bloom filters at each DFA node to speed up transition lookups.

**Approach**:
- Each DFA node maintains bloom filters indexed by fragment length
- When matching, hash substrings of each possible fragment length
- Use bloom filter for fast negative checks before HashMap lookup
- Idea: `might_have_transition()` avoids expensive HashMap lookups

**Implementation**:
```rust
struct DFANode {
    id: usize,
    bloom_by_length: FxHashMap<usize, BloomFilter>,
    transitions: FxHashMap<String, usize>,
    matching_templates: Vec<u64>,
}
```

**Benchmark Results**:
```
Bloom DFA:       111.24μs per log (8,989 logs/sec)
Aho-Corasick:    0.34μs per log (2,972,139 logs/sec)
Result:          Aho-Corasick is 330.63x FASTER
```

**Why it failed**:
1. **O(n × m) complexity**: Must try every start position × every possible fragment length
2. **Bloom filter overhead**: Hashing is more expensive than AC's optimized state transitions
3. **No failure links**: Standard AC has failure links that allow skipping backwards efficiently
4. **Double lookup cost**: Bloom filter check + HashMap lookup adds overhead vs direct HashMap

**Conclusion**: Bloom filters are not beneficial here. The overhead of hashing and checking bloom filters for every substring far exceeds the cost of AC's optimized DFA transitions.

---

## Lessons Learned

### What Worked
- **Weighted fragment scoring**: Successfully handles generic fragments (e.g., " uid=", " tty=") by assigning lower weights
- **Aho-Corasick remains optimal**: For literal multi-pattern string matching, AC is hard to beat

### What Didn't Work
- **Path compression**: Merging fragments includes regex constructs like `(\d+)` which don't exist in actual logs
- **Bloom filter DFA**: Hashing overhead + O(n×m) complexity makes it 330x slower
- **Filtering generic fragments**: Removes important matching information, breaks accuracy

### Key Insight

The performance bottleneck for Linux logs (68K logs/sec vs Apache's 2M logs/sec) is **fundamental to the problem size**:

- Linux: 109 chars × 153 patterns = 16,640 comparisons/log
- Apache: 83 chars × 11 patterns = 916 comparisons/log
- **18x more work per log**

Optimizations like weighted scoring improve semantic matching quality, but can't change the O(n × m) nature of multi-pattern matching. The best approach remains using standard Aho-Corasick with weighted scoring for result ranking.
