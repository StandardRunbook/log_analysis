/// Learns patterns from multiple log samples by detecting what varies vs what stays static
use regex::Regex;
use std::collections::HashMap;

pub struct PatternLearner;

impl PatternLearner {
    /// Learn a pattern from multiple samples of the same log type
    /// Returns a regex pattern and variable names
    pub fn learn_from_samples(samples: &[String]) -> (String, Vec<String>) {
        if samples.is_empty() {
            return (String::from(".*"), vec![]);
        }

        if samples.len() == 1 {
            return Self::learn_from_single_sample(&samples[0]);
        }

        // Tokenize all samples
        let tokenized: Vec<Vec<Token>> = samples
            .iter()
            .map(|s| Self::tokenize(s))
            .collect();

        // Align tokens and find variable positions
        let pattern_tokens = Self::align_and_detect_variables(&tokenized);

        // Build regex pattern
        Self::build_pattern(&pattern_tokens)
    }

    /// Tokenize a log line into semantic units
    fn tokenize(log: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut current_type = TokenType::Unknown;

        for ch in log.chars() {
            let char_type = Self::classify_char(ch);

            if char_type != current_type && !current.is_empty() {
                tokens.push(Token {
                    value: current.clone(),
                    token_type: current_type,
                });
                current.clear();
            }

            current.push(ch);
            current_type = char_type;
        }

        if !current.is_empty() {
            tokens.push(Token {
                value: current,
                token_type: current_type,
            });
        }

        tokens
    }

    /// Classify a character's type
    fn classify_char(ch: char) -> TokenType {
        if ch.is_ascii_digit() {
            TokenType::Digit
        } else if ch.is_ascii_alphabetic() {
            TokenType::Alpha
        } else if ch.is_whitespace() {
            TokenType::Whitespace
        } else {
            TokenType::Punctuation
        }
    }

    /// Align tokens across samples and detect which positions vary
    fn align_and_detect_variables(tokenized: &[Vec<Token>]) -> Vec<PatternToken> {
        if tokenized.is_empty() {
            return vec![];
        }

        let max_len = tokenized.iter().map(|t| t.len()).max().unwrap_or(0);
        let mut pattern_tokens = Vec::new();

        for pos in 0..max_len {
            let tokens_at_pos: Vec<&Token> = tokenized
                .iter()
                .filter_map(|tokens| tokens.get(pos))
                .collect();

            if tokens_at_pos.is_empty() {
                continue;
            }

            // Check if all tokens at this position have the same value
            let first_value = &tokens_at_pos[0].value;
            let all_same = tokens_at_pos.iter().all(|t| &t.value == first_value);

            if all_same {
                // Static token
                pattern_tokens.push(PatternToken::Static(first_value.clone()));
            } else {
                // Variable token - detect type
                let var_type = Self::detect_variable_type(&tokens_at_pos);
                pattern_tokens.push(PatternToken::Variable(var_type));
            }
        }

        pattern_tokens
    }

    /// Detect what type of variable this is based on the samples
    fn detect_variable_type(tokens: &[&Token]) -> VariableType {
        // Check if all are digits
        if tokens.iter().all(|t| t.token_type == TokenType::Digit) {
            let all_values: Vec<&str> = tokens.iter().map(|t| t.value.as_str()).collect();

            // Check if it looks like a timestamp
            if all_values.iter().any(|v| v.len() == 10 && v.parse::<u32>().is_ok()) {
                return VariableType::UnixTimestamp;
            }

            // Check if it's in timestamp position (first few tokens)
            if tokens[0].value.len() <= 2 {
                return VariableType::Number;
            }

            return VariableType::Number;
        }

        // Check if it's an IP address
        if tokens.iter().any(|t| {
            let parts: Vec<&str> = t.value.split('.').collect();
            parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok())
        }) {
            return VariableType::IPAddress;
        }

        // Check if it's a hex number
        if tokens.iter().all(|t| {
            t.value.starts_with("0x") || t.value.chars().all(|c| c.is_ascii_hexdigit())
        }) {
            return VariableType::HexNumber;
        }

        // Check if it's a UUID
        if tokens.iter().any(|t| {
            let parts: Vec<&str> = t.value.split('-').collect();
            parts.len() == 5
        }) {
            return VariableType::UUID;
        }

        // Default to generic string
        VariableType::String
    }

    /// Build regex pattern from pattern tokens
    fn build_pattern(tokens: &[PatternToken]) -> (String, Vec<String>) {
        let mut pattern = String::new();
        let mut variables = Vec::new();
        let mut var_count = HashMap::new();

        for token in tokens {
            match token {
                PatternToken::Static(value) => {
                    pattern.push_str(&regex::escape(value));
                }
                PatternToken::Variable(var_type) => {
                    let (regex_pattern, var_name_base) = var_type.to_regex_and_name();
                    pattern.push_str(regex_pattern);

                    // Generate unique variable name
                    let count = var_count.entry(var_name_base.clone()).or_insert(0);
                    *count += 1;
                    let var_name = if *count == 1 {
                        var_name_base
                    } else {
                        format!("{}_{}", var_name_base, count)
                    };
                    variables.push(var_name);
                }
            }
        }

        (pattern, variables)
    }

    /// Learn from a single sample (fallback)
    fn learn_from_single_sample(sample: &str) -> (String, Vec<String>) {
        // Use heuristics to detect common patterns
        let mut pattern = String::new();
        let mut variables = Vec::new();

        // Detect IP addresses
        let ip_re = Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap();
        let mut last_end = 0;

        for mat in ip_re.find_iter(sample) {
            pattern.push_str(&regex::escape(&sample[last_end..mat.start()]));
            pattern.push_str(r"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})");
            variables.push("ip_address".to_string());
            last_end = mat.end();
        }

        // Add remaining text
        if last_end < sample.len() {
            pattern.push_str(&regex::escape(&sample[last_end..]));
        }

        if pattern.is_empty() {
            pattern = regex::escape(sample);
        }

        (pattern, variables)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Token {
    value: String,
    token_type: TokenType,
}

#[derive(Debug, Clone, PartialEq)]
enum TokenType {
    Digit,
    Alpha,
    Whitespace,
    Punctuation,
    Unknown,
}

#[derive(Debug, Clone)]
enum PatternToken {
    Static(String),
    Variable(VariableType),
}

#[derive(Debug, Clone)]
enum VariableType {
    Number,
    IPAddress,
    HexNumber,
    UUID,
    UnixTimestamp,
    String,
}

impl VariableType {
    fn to_regex_and_name(&self) -> (&'static str, String) {
        match self {
            VariableType::Number => (r"(\d+)", "number".to_string()),
            VariableType::IPAddress => (r"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})", "ip_address".to_string()),
            VariableType::HexNumber => (r"(0x[0-9a-fA-F]+|[0-9a-fA-F]+)", "hex_number".to_string()),
            VariableType::UUID => (r"([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})", "uuid".to_string()),
            VariableType::UnixTimestamp => (r"(\d{10,})", "timestamp".to_string()),
            VariableType::String => (r"(\S+)", "value".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_learn_from_multiple_samples() {
        let samples = vec![
            "Jun 14 15:16:01 combo sshd[19939]: auth failure uid=0 rhost=218.188.2.4".to_string(),
            "Jun 14 15:16:02 combo sshd[19937]: auth failure uid=0 rhost=218.188.2.4".to_string(),
            "Jun 15 02:04:59 combo sshd[20882]: auth failure uid=0 rhost=220.135.151.1".to_string(),
        ];

        let (pattern, variables) = PatternLearner::learn_from_samples(&samples);

        println!("Pattern: {}", pattern);
        println!("Variables: {:?}", variables);

        // Should detect that timestamp, pid, and IP change
        assert!(pattern.contains(r"(\d+)")); // PID
        assert!(variables.iter().any(|v| v.contains("ip") || v.contains("number")));
    }
}
