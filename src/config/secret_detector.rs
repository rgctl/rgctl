//! Secret detection in configuration files
//!
//! Task 2.2.3: Detect hardcoded secrets via patterns and entropy.

use regex::Regex;
use std::collections::HashMap;

/// Severity of a detected secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Low risk
    Low,
    /// Medium risk
    Medium,
    /// High risk
    High,
    /// Critical risk (live keys, passwords)
    Critical,
}

/// A detected secret in configuration content.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedSecret {
    /// Secret type (api_key, password, token, etc.)
    pub secret_type: String,
    /// Matched value (may be redacted)
    pub value: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Severity
    pub severity: Severity,
    /// Detection reason
    pub reason: String,
}

/// Secret detector using pattern matching and entropy heuristics.
pub struct SecretDetector {
    patterns: Vec<(Regex, String, Severity)>,
}

impl Default for SecretDetector {
    fn default() -> Self {
        let patterns = vec![
            (
                Regex::new(r#"(?i)(api[_-]?key|apikey)\s*[:=]\s*["']?([a-zA-Z0-9_\-]{16,})"#)
                    .unwrap(),
                "api_key".to_string(),
                Severity::Critical,
            ),
            (
                Regex::new(r#"(?i)(password|passwd|pwd)\s*[:=]\s*["']?([^\s"']{8,})"#).unwrap(),
                "password".to_string(),
                Severity::Critical,
            ),
            (
                Regex::new(r#"(?i)(secret|token)\s*[:=]\s*["']?([a-zA-Z0-9_\-]{16,})"#).unwrap(),
                "token".to_string(),
                Severity::High,
            ),
            (
                Regex::new(r"sk_live_[a-zA-Z0-9]{16,}").unwrap(),
                "stripe_key".to_string(),
                Severity::Critical,
            ),
            (
                Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
                "aws_key".to_string(),
                Severity::Critical,
            ),
        ];
        Self { patterns }
    }
}

impl SecretDetector {
    /// Create a new secret detector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Scan configuration text for secrets.
    pub fn scan(&self, content: &str) -> Vec<DetectedSecret> {
        let mut secrets = Vec::new();
        let mut seen = HashMap::new();

        for (line_no, line) in content.lines().enumerate() {
            for (re, secret_type, severity) in &self.patterns {
                for cap in re.captures_iter(line) {
                    let value = cap
                        .get(2)
                        .or_else(|| cap.get(0))
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default();
                    if value.is_empty() || is_false_positive(&value) {
                        continue;
                    }
                    let key = format!("{line_no}:{value}");
                    if seen.insert(key, ()).is_some() {
                        continue;
                    }
                    secrets.push(DetectedSecret {
                        secret_type: secret_type.clone(),
                        value: redact(&value),
                        line: line_no + 1,
                        severity: *severity,
                        reason: format!("Pattern match for {secret_type}"),
                    });
                }
            }

            if let Some(entropy_secret) = detect_high_entropy(line) {
                let key = format!("{line_no}:{}", entropy_secret.value);
                if seen.insert(key, ()).is_none() {
                    secrets.push(DetectedSecret {
                        line: line_no + 1,
                        ..entropy_secret
                    });
                }
            }
        }
        secrets
    }
}

fn is_false_positive(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower == "true"
        || lower == "false"
        || lower == "null"
        || lower == "none"
        || lower.starts_with("${")
        || lower.starts_with("your_")
        || lower.starts_with("changeme")
        || value.len() < 8
}

fn redact(value: &str) -> String {
    if value.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}...{}", &value[..4], &value[value.len() - 4..])
    }
}

fn detect_high_entropy(line: &str) -> Option<DetectedSecret> {
    let re = Regex::new(r#"[:=]\s*["']([a-zA-Z0-9+/=_\-]{32,})["']"#).ok()?;
    let cap = re.captures(line)?;
    let value = cap.get(1)?.as_str();
    if is_false_positive(value) {
        return None;
    }
    let entropy = shannon_entropy(value);
    if entropy > 4.0 {
        Some(DetectedSecret {
            secret_type: "high_entropy".to_string(),
            value: redact(value),
            line: 0,
            severity: Severity::Medium,
            reason: format!("High entropy string (H={entropy:.2})"),
        })
    } else {
        None
    }
}

fn shannon_entropy(s: &str) -> f64 {
    let mut freq = HashMap::new();
    for c in s.chars() {
        *freq.entry(c).or_insert(0usize) += 1;
    }
    let len = s.len() as f64;
    freq.values()
        .map(|&count| {
            let p = count as f64 / len;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_detection() {
        let config = r#"
api_key: "sk_live_1234567890abcdef"
password: "mysecretpassword123"
debug: true
"#;
        let detector = SecretDetector::new();
        let secrets = detector.scan(config);
        assert!(secrets.len() >= 2);
        assert!(secrets.iter().any(|s| s.severity == Severity::Critical));
    }

    #[test]
    fn test_false_positive_filtering() {
        let config = "password: changeme\napi_key: your_api_key_here\n";
        let detector = SecretDetector::new();
        let secrets = detector.scan(config);
        assert!(secrets.is_empty());
    }
}
