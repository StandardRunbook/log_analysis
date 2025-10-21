// Core modules
pub mod llm_service;
pub mod log_format_detector;
pub mod log_matcher;
pub mod log_matcher_fast; // Optimized matcher with FxHashMap
pub mod log_matcher_zero_copy; // Zero-copy matcher with thread-local buffers
pub mod matcher_config;

// Dependency injection framework for benchmarking
pub mod benchmark_runner;
pub mod dataset_splitter;
pub mod implementations;
pub mod loghub_loader;
pub mod traits;
pub mod smart_template_generator;
pub mod pattern_learner;
pub mod fragment_classifier;
pub mod semantic_template_generator;
pub mod token_classifier;
