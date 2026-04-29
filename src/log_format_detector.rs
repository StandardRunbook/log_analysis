/// Detects the format of log lines and extracts structural patterns
use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub enum LogFormat {
    Syslog { has_pid: bool },
    ISOTimestamp,
    CustomDelimited { delimiter: char },
    Unstructured,
}

pub struct LogFormatDetector;

impl LogFormatDetector {
    /// Detect the format of a log line
    pub fn detect(log_line: &str) -> LogFormat {
        // Check for syslog format: "Month Day HH:MM:SS hostname service[pid]: message"
        if Self::is_syslog_format(log_line) {
            let has_pid = log_line.contains('[') && log_line.contains("]: ");
            return LogFormat::Syslog { has_pid };
        }

        // Check for ISO timestamp format
        if Self::has_iso_timestamp(log_line) {
            return LogFormat::ISOTimestamp;
        }

        // Check for delimited formats (CSV, TSV, etc.)
        if let Some(delimiter) = Self::detect_delimiter(log_line) {
            return LogFormat::CustomDelimited { delimiter };
        }

        LogFormat::Unstructured
    }

    /// Check if log line follows syslog format
    fn is_syslog_format(log_line: &str) -> bool {
        // Syslog pattern: "Month Day HH:MM:SS hostname ..."
        let syslog_pattern =
            Regex::new(r"^[A-Z][a-z]{2}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}\s+\S+\s+\S+").unwrap();
        syslog_pattern.is_match(log_line)
    }

    /// Check if log has ISO 8601 timestamp
    fn has_iso_timestamp(log_line: &str) -> bool {
        let iso_pattern = Regex::new(r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}").unwrap();
        iso_pattern.is_match(log_line)
    }

    /// Detect delimiter in log line
    fn detect_delimiter(log_line: &str) -> Option<char> {
        for delimiter in &[',', '\t', '|', ';'] {
            if log_line.matches(*delimiter).count() >= 3 {
                return Some(*delimiter);
            }
        }
        None
    }

    /// Extract structural components from a syslog line
    pub fn extract_syslog_components(log_line: &str) -> Option<SyslogComponents> {
        let pattern = Regex::new(
            r"^([A-Z][a-z]{2}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2})\s+(\S+)\s+(\S+?)(\[\d+\])?\s*:\s*(.+)$"
        ).ok()?;

        pattern.captures(log_line).map(|caps| SyslogComponents {
            timestamp: caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
            hostname: caps
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
            service: caps
                .get(3)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
            pid: caps.get(4).map(|m| m.as_str().to_string()),
            message: caps
                .get(5)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SyslogComponents {
    pub timestamp: String,
    pub hostname: String,
    pub service: String,
    pub pid: Option<String>,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_syslog_with_pid() {
        let log = "Jun 14 15:16:01 combo sshd(pam_unix)[19939]: authentication failure";
        assert_eq!(
            LogFormatDetector::detect(log),
            LogFormat::Syslog { has_pid: true }
        );
    }

    #[test]
    fn test_detect_syslog_without_pid() {
        let log = "Jul 27 14:41:58 combo kernel: PCI: Using configuration type 1";
        assert_eq!(
            LogFormatDetector::detect(log),
            LogFormat::Syslog { has_pid: false }
        );
    }

    #[test]
    fn test_extract_syslog_components() {
        let log = "Jun 14 15:16:01 combo sshd(pam_unix)[19939]: authentication failure; logname=";
        let components = LogFormatDetector::extract_syslog_components(log).unwrap();

        assert_eq!(components.timestamp, "Jun 14 15:16:01");
        assert_eq!(components.hostname, "combo");
        assert_eq!(components.service, "sshd(pam_unix)");
        assert_eq!(components.pid, Some("[19939]".to_string()));
        assert_eq!(components.message, "authentication failure; logname=");
    }

    #[test]
    fn test_extract_syslog_no_pid() {
        let log = "Jul 27 14:41:58 combo kernel: PCI: Using configuration type 1";
        let components = LogFormatDetector::extract_syslog_components(log).unwrap();

        assert_eq!(components.service, "kernel");
        assert_eq!(components.pid, None);
        assert_eq!(components.message, "PCI: Using configuration type 1");
    }

    #[test]
    fn test_detect_iso_timestamp() {
        // ISO 8601 with T separator and with space separator both count
        for log in [
            "2024-01-15T10:30:45 something happened",
            "2024-01-15 10:30:45 INFO start",
        ] {
            assert_eq!(LogFormatDetector::detect(log), LogFormat::ISOTimestamp);
        }
    }

    #[test]
    fn test_detect_csv_delimited() {
        // ≥ 3 commas → CSV
        let log = "field1,field2,field3,field4";
        assert_eq!(
            LogFormatDetector::detect(log),
            LogFormat::CustomDelimited { delimiter: ',' }
        );
    }

    #[test]
    fn test_detect_pipe_delimited() {
        let log = "a|b|c|d|e";
        assert_eq!(
            LogFormatDetector::detect(log),
            LogFormat::CustomDelimited { delimiter: '|' }
        );
    }

    #[test]
    fn test_detect_tab_delimited() {
        let log = "a\tb\tc\td";
        assert_eq!(
            LogFormatDetector::detect(log),
            LogFormat::CustomDelimited { delimiter: '\t' }
        );
    }

    #[test]
    fn test_detect_semicolon_delimited() {
        let log = "a;b;c;d";
        assert_eq!(
            LogFormatDetector::detect(log),
            LogFormat::CustomDelimited { delimiter: ';' }
        );
    }

    #[test]
    fn test_detect_unstructured_falls_through() {
        // Plain prose with no consistent delimiter and no recognizable
        // timestamp lands as Unstructured.
        let log = "this is just a sentence";
        assert_eq!(LogFormatDetector::detect(log), LogFormat::Unstructured);
    }

    #[test]
    fn test_detect_delimiter_threshold_is_three() {
        // Two commas isn't enough; the function requires ≥ 3.
        let log = "a,b,c";
        assert_eq!(LogFormatDetector::detect(log), LogFormat::Unstructured);
    }

    #[test]
    fn test_extract_syslog_returns_none_for_non_syslog() {
        // The extractor only succeeds when the line actually matches the
        // syslog shape. Garbage input → None.
        assert!(LogFormatDetector::extract_syslog_components("just random text").is_none());
        assert!(LogFormatDetector::extract_syslog_components("").is_none());
    }
}
