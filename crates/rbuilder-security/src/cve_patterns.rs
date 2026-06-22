//! CWE / OWASP vulnerability patterns (Phase 13.5).

use serde::{Deserialize, Serialize};

/// Common Weakness Enumeration pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CwePattern {
    /// CWE id (e.g. CWE-89).
    pub cwe_id: String,
    /// Short name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Severity 1–10.
    pub severity: u8,
    /// Source regex patterns.
    pub source_patterns: Vec<String>,
    /// Sink regex patterns.
    pub sink_patterns: Vec<String>,
    /// Sanitizer regex patterns.
    pub sanitizer_patterns: Vec<String>,
}

/// Built-in OWASP Top 10 oriented patterns.
pub fn default_cwe_patterns() -> Vec<CwePattern> {
    vec![
        CwePattern {
            cwe_id: "CWE-89".into(),
            name: "SQL Injection".into(),
            description: "Improper neutralization of special elements in SQL commands".into(),
            severity: 10,
            source_patterns: vec![
                r"request\.(GET|POST|query|body)".into(),
                r"req\.(query|params|body)".into(),
            ],
            sink_patterns: vec![
                r"\.execute\(".into(),
                r"\.query\(".into(),
                r"cursor\.(execute|executemany)".into(),
            ],
            sanitizer_patterns: vec![r"int\(".into(), r"parseInt\(".into(), r"prepare\(".into()],
        },
        CwePattern {
            cwe_id: "CWE-79".into(),
            name: "Cross-Site Scripting (XSS)".into(),
            description: "Improper neutralization of input during web page generation".into(),
            severity: 9,
            source_patterns: vec![r"request\.(GET|POST)".into(), r"req\.(query|body)".into()],
            sink_patterns: vec![
                r"innerHTML".into(),
                r"document\.write".into(),
                r"\.html\(".into(),
            ],
            sanitizer_patterns: vec![
                r"escape\(".into(),
                r"sanitize\(".into(),
                r"html\.escape".into(),
            ],
        },
        CwePattern {
            cwe_id: "CWE-78".into(),
            name: "OS Command Injection".into(),
            description: "Improper neutralization of special elements in OS commands".into(),
            severity: 10,
            source_patterns: vec![
                r"request\.GET".into(),
                r"req\.query".into(),
                r"sys\.argv".into(),
            ],
            sink_patterns: vec![
                r"os\.system\(".into(),
                r"subprocess\.(call|run|Popen)".into(),
                r"Command::new".into(),
            ],
            sanitizer_patterns: vec![r"shlex\.quote".into(), r"shellEscape\(".into()],
        },
        CwePattern {
            cwe_id: "CWE-22".into(),
            name: "Path Traversal".into(),
            description: "Improper limitation of pathname to a restricted directory".into(),
            severity: 8,
            source_patterns: vec![r"request\.(GET|POST)".into(), r"req\.(query|params)".into()],
            sink_patterns: vec![
                r"open\(".into(),
                r"fs\.readFile".into(),
                r"File::open".into(),
            ],
            sanitizer_patterns: vec![r"os\.path\.basename".into(), r"path\.basename".into()],
        },
        CwePattern {
            cwe_id: "CWE-798".into(),
            name: "Hardcoded Credentials".into(),
            description: "Use of hard-coded credentials".into(),
            severity: 9,
            source_patterns: vec![
                r#"password\s*=\s*['\"]"#.into(),
                r#"api_key\s*=\s*['\"]"#.into(),
                r#"secret\s*=\s*['\"]"#.into(),
            ],
            sink_patterns: vec![],
            sanitizer_patterns: vec![r"env::var".into(), r"process\.env".into()],
        },
        CwePattern {
            cwe_id: "CWE-502".into(),
            name: "Insecure Deserialization".into(),
            description: "Deserialization of untrusted data".into(),
            severity: 9,
            source_patterns: vec![r"request\.(GET|POST|body)".into(), r"req\.body".into()],
            sink_patterns: vec![
                r"pickle\.loads".into(),
                r"yaml\.load".into(),
                r"serde_json::from_str".into(),
            ],
            sanitizer_patterns: vec![r"jsonschema".into(), r"validate\(".into()],
        },
        CwePattern {
            cwe_id: "CWE-918".into(),
            name: "Server-Side Request Forgery (SSRF)".into(),
            description: "Unvalidated URL fetch from user input".into(),
            severity: 8,
            source_patterns: vec![r"request\.(GET|POST)".into(), r"req\.(query|params)".into()],
            sink_patterns: vec![
                r"requests\.get\(".into(),
                r"urllib\.request".into(),
                r"fetch\(".into(),
                r"http\.get\(".into(),
            ],
            sanitizer_patterns: vec![r"allowlist".into(), r"validate_url".into()],
        },
        CwePattern {
            cwe_id: "CWE-352".into(),
            name: "Cross-Site Request Forgery (CSRF)".into(),
            description: "Missing anti-CSRF token on state-changing requests".into(),
            severity: 7,
            source_patterns: vec![r"@app\.route".into(), r"router\.post".into()],
            sink_patterns: vec![r"\.post\(".into(), r"POST".into()],
            sanitizer_patterns: vec![
                r"csrf".into(),
                r"csrf_token".into(),
                r"@csrf\.exempt".into(),
            ],
        },
        CwePattern {
            cwe_id: "CWE-287".into(),
            name: "Improper Authentication".into(),
            description: "Authentication bypass or missing verification".into(),
            severity: 9,
            source_patterns: vec![r"login".into(), r"authenticate".into()],
            sink_patterns: vec![
                r"bypass".into(),
                r"skip_auth".into(),
                r"#\[allow\(unauthenticated\)\]".into(),
            ],
            sanitizer_patterns: vec![r"verify_password".into(), r"check_token".into()],
        },
        CwePattern {
            cwe_id: "CWE-306".into(),
            name: "Missing Authentication for Critical Function".into(),
            description: "Sensitive operation without authentication check".into(),
            severity: 8,
            source_patterns: vec![r"admin".into(), r"delete_user".into()],
            sink_patterns: vec![r"\.delete\(".into(), r"drop_table".into()],
            sanitizer_patterns: vec![r"require_auth".into(), r"@login_required".into()],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_patterns_cover_owasp() {
        let patterns = default_cwe_patterns();
        assert!(patterns.len() >= 10);
        assert!(patterns.iter().any(|p| p.cwe_id == "CWE-89"));
        assert!(patterns.iter().any(|p| p.cwe_id == "CWE-79"));
    }
}
