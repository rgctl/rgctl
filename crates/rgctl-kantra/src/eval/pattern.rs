//! Shared Kantra glob/regex pattern compilation (JR5, JR6).

use regex::Regex;

/// Compile a Konveyor Kantra `pattern` into a Rust [`Regex`].
///
/// Konveyor treats unescaped `*` as a glob wildcard (`.*`). Rust regex treats
/// `*` as a quantifier on the preceding atom, so `java.util.(Formatter|Scanner)*`
/// would match zero occurrences of the group. This compiler:
/// 1. Rewrites glob `*` (start of pattern or not preceded by `.` / `\`) to `.*`
/// 2. Preserves existing `.*` and `\*`
/// 3. Anchors the match on identifier / package-segment boundaries so bare
///    names like `ClassLoader` do not substring-match `URLClassLoader`
pub fn compile_kantra_pattern(pattern: &str) -> Result<Regex, regex::Error> {
    let normalized = normalize_kantra_glob(pattern);
    let anchored = anchor_kantra_pattern(&normalized);
    Regex::new(&anchored)
}

/// Translate Konveyor glob `*` into regex `.*` without doubling existing `.*`.
pub fn normalize_kantra_glob(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len() + 8);
    let mut i = 0;
    let mut in_class = false;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                out.push('\\');
                if i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                i += 1;
            }
            '[' if !in_class => {
                in_class = true;
                out.push('[');
                i += 1;
            }
            ']' if in_class => {
                in_class = false;
                out.push(']');
                i += 1;
            }
            '*' if !in_class => {
                let preceded_by_dot_or_escape = out
                    .chars()
                    .next_back()
                    .is_some_and(|prev| prev == '.' || prev == '\\');
                if preceded_by_dot_or_escape {
                    out.push('*');
                } else {
                    out.push_str(".*");
                }
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Wrap a normalized pattern with token/segment boundary anchors (JR5).
fn anchor_kantra_pattern(normalized: &str) -> String {
    // Left: start, whitespace, package/inner separators, annotation, or import.
    // Right: end, whitespace, statement/member terminators, generics, arrays.
    format!(r"(?:^|[\s.$@(]|import\s+(?:static\s+)?)(?:{normalized})(?:$|[\s;.*$()<>,\[\]])")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_star_requires_formatter_or_scanner() {
        let re = compile_kantra_pattern("java.util.(Formatter|Scanner)*").unwrap();
        assert!(re.is_match("import java.util.Scanner;"));
        assert!(re.is_match("import java.util.Formatter;"));
        assert!(!re.is_match("import java.util.*;"));
        assert!(!re.is_match("import java.util.List;"));
    }

    #[test]
    fn leading_star_compiles() {
        let re = compile_kantra_pattern("*EntityManager").unwrap();
        assert!(re.is_match("javax.persistence.EntityManager"));
        let re2 = compile_kantra_pattern("*.persistence.*").unwrap();
        assert!(re2.is_match("javax.persistence.Query"));
    }

    #[test]
    fn preserves_dot_star_and_escaped_star() {
        assert_eq!(normalize_kantra_glob(".*"), ".*");
        assert_eq!(normalize_kantra_glob(r"\*"), r"\*");
        assert_eq!(
            normalize_kantra_glob("java.util.(Formatter|Scanner)*"),
            "java.util.(Formatter|Scanner).*"
        );
        assert_eq!(normalize_kantra_glob("*EntityManager"), ".*EntityManager");
    }

    #[test]
    fn bare_classloader_does_not_match_urlclassloader() {
        let re = compile_kantra_pattern("ClassLoader").unwrap();
        assert!(re.is_match("java.lang.ClassLoader"));
        assert!(re.is_match("ClassLoader"));
        assert!(!re.is_match("import java.net.URLClassLoader;"));
        assert!(!re.is_match("URLClassLoader"));
    }
}
