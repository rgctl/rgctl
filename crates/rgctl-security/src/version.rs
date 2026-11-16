//! Ecosystem version comparison for OSV range membership.
//!
//! Sync-only APIs (safe under `spawn_blocking`). Not on the discover hot path.

use crate::error::SecurityError;
use crate::osv::{VersionInterval, VersionUpperBound};

/// Pluggable version ordering / range membership.
pub trait VersionEngine: Send + Sync {
    fn ecosystem_id(&self) -> &'static str;
    /// Compare `a` vs `b`: Less = a < b, etc.
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError>;

    /// True if `version` is in `[introduced, fixed)` (fixed exclusive when present).
    ///
    /// **Prefer `in_any_vulnerable_range` for multi-branch advisories.**
    /// This single-interval variant is kept for backward compatibility.
    fn in_vulnerable_range(
        &self,
        version: &str,
        introduced: Option<&str>,
        fixed: Option<&str>,
    ) -> Result<bool, SecurityError> {
        if let Some(intro) = introduced
            && intro != "0"
            && self.cmp_versions(version, intro)? == std::cmp::Ordering::Less
        {
            return Ok(false);
        }
        if let Some(fix) = fixed
            && self.cmp_versions(version, fix)? != std::cmp::Ordering::Less
        {
            return Ok(false);
        }
        Ok(true)
    }

    /// True if `version` falls into any of the provided `VersionInterval`s, or is listed in
    /// `listed_versions` when no intervals are present.
    ///
    /// Supports both exclusive `Fixed` upper bounds (`version < fixed`) and inclusive
    /// `LastAffected` upper bounds (`version <= last_affected`) per the OSV spec (task 1.2).
    fn in_any_vulnerable_range(
        &self,
        version: &str,
        ranges: &[VersionInterval],
        listed_versions: &[String],
    ) -> Result<bool, SecurityError> {
        // Fallback: if no ranges, check exact listed_versions membership.
        if ranges.is_empty() {
            return Ok(listed_versions.iter().any(|v| v == version));
        }

        for interval in ranges {
            // Lower bound: introduced (inclusive). "0" means unbounded from the start.
            if let Some(intro) = &interval.introduced {
                if intro != "0" && self.cmp_versions(version, intro)? == std::cmp::Ordering::Less {
                    continue;
                }
            }

            // Upper bound.
            let in_upper = match &interval.upper {
                VersionUpperBound::Fixed(fix) => {
                    self.cmp_versions(version, fix)? == std::cmp::Ordering::Less
                }
                VersionUpperBound::LastAffected(last) => {
                    self.cmp_versions(version, last)? != std::cmp::Ordering::Greater
                }
                VersionUpperBound::Open => true,
            };

            if in_upper {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

/// Resolve an engine from OSV / PURL / lockfile ecosystem labels.
pub fn engine_for_ecosystem(label: &str) -> Result<&'static dyn VersionEngine, SecurityError> {
    let lower = label.trim().to_ascii_lowercase();
    if lower.starts_with("maven") || lower == "gradle" {
        return Ok(&MavenEngine);
    }
    if lower == "crates.io" || lower == "cargo" || lower == "rust" {
        return Ok(&CargoEngine);
    }
    if lower == "npm" || lower == "javascript" {
        return Ok(&NpmEngine);
    }
    if lower == "go" || lower == "golang" {
        return Ok(&GoEngine);
    }
    if lower == "pypi" || lower == "python" {
        return Ok(&PyPiEngine);
    }
    if lower.starts_with("nuget") || lower == "dotnet" || lower == "csharp" {
        return Ok(&NuGetEngine);
    }
    if lower == "packagist" || lower == "composer" || lower == "php" {
        return Ok(&PackagistEngine);
    }
    if lower == "rubygems" || lower == "gem" || lower == "ruby" {
        return Ok(&RubyGemsEngine);
    }
    if lower == "pub" || lower == "dart" {
        return Ok(&PubEngine);
    }
    if lower == "hex" || lower == "elixir" {
        return Ok(&HexEngine);
    }
    if lower == "swift" || lower == "swifturl" || lower == "swiftpm" {
        return Ok(&SwiftEngine);
    }
    if lower == "puppet forge" || lower == "puppet" || lower == "forge" {
        return Ok(&PuppetEngine);
    }
    if lower == "luarocks" || lower == "lua" {
        return Ok(&LuaRocksEngine);
    }
    if lower == "conan" || lower == "conancenter" || lower == "vcpkg" {
        return Ok(&ConanVcpkgEngine);
    }
    Err(SecurityError::UnsupportedEcosystem(label.to_string()))
}

struct MavenEngine;
struct CargoEngine;
struct NpmEngine;
struct GoEngine;
struct PyPiEngine;
struct NuGetEngine;
struct PackagistEngine;
struct RubyGemsEngine;
struct PubEngine;
struct HexEngine;
struct SwiftEngine;
struct PuppetEngine;
struct LuaRocksEngine;
struct ConanVcpkgEngine;

impl VersionEngine for MavenEngine {
    fn ecosystem_id(&self) -> &'static str {
        "Maven"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        Ok(cmp_maven(a, b))
    }
}

impl VersionEngine for CargoEngine {
    fn ecosystem_id(&self) -> &'static str {
        "crates.io"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        Ok(cmp_semver_strict(a, b, false))
    }
}

impl VersionEngine for NpmEngine {
    fn ecosystem_id(&self) -> &'static str {
        "npm"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        Ok(cmp_semver_strict(a, b, false))
    }
}

impl VersionEngine for GoEngine {
    fn ecosystem_id(&self) -> &'static str {
        "Go"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        let a = a.strip_prefix('v').unwrap_or(a);
        let b = b.strip_prefix('v').unwrap_or(b);
        Ok(cmp_semver_strict(a, b, false))
    }
}

impl VersionEngine for PyPiEngine {
    fn ecosystem_id(&self) -> &'static str {
        "PyPI"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        Ok(cmp_pep440(a, b))
    }
}

impl VersionEngine for NuGetEngine {
    fn ecosystem_id(&self) -> &'static str {
        "NuGet"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        Ok(cmp_semver_strict(a, b, true))
    }
}

impl VersionEngine for PackagistEngine {
    fn ecosystem_id(&self) -> &'static str {
        "Packagist"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        Ok(cmp_packagist(a, b))
    }
}

impl VersionEngine for RubyGemsEngine {
    fn ecosystem_id(&self) -> &'static str {
        "RubyGems"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        Ok(cmp_rubygems(a, b))
    }
}

impl VersionEngine for PubEngine {
    fn ecosystem_id(&self) -> &'static str {
        "Pub"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        Ok(cmp_semver_strict(a, b, false))
    }
}

impl VersionEngine for HexEngine {
    fn ecosystem_id(&self) -> &'static str {
        "Hex"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        Ok(cmp_semver_strict(a, b, false))
    }
}

impl VersionEngine for SwiftEngine {
    fn ecosystem_id(&self) -> &'static str {
        "Swift"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        let a = a.strip_prefix('v').unwrap_or(a);
        let b = b.strip_prefix('v').unwrap_or(b);
        Ok(cmp_semver_strict(a, b, false))
    }
}

impl VersionEngine for PuppetEngine {
    fn ecosystem_id(&self) -> &'static str {
        "Puppet Forge"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        Ok(cmp_semver_strict(a, b, false))
    }
}

impl VersionEngine for LuaRocksEngine {
    fn ecosystem_id(&self) -> &'static str {
        "LuaRocks"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        let a = a.split('-').next().unwrap_or(a);
        let b = b.split('-').next().unwrap_or(b);
        Ok(cmp_semver_strict(a, b, false))
    }
}

impl VersionEngine for ConanVcpkgEngine {
    fn ecosystem_id(&self) -> &'static str {
        "Conan"
    }
    fn cmp_versions(&self, a: &str, b: &str) -> Result<std::cmp::Ordering, SecurityError> {
        Ok(cmp_semver_strict(a, b, true))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MavenToken {
    nums: Vec<u64>,
    qualifier: String,
    vendor: String,
}

fn parse_maven(v: &str) -> MavenToken {
    let (core, vendor) = if let Some(idx) = v.find("-rhlw-") {
        (&v[..idx], v[idx + 1..].to_string())
    } else if let Some(idx) = v.find("-redhat-") {
        (&v[..idx], v[idx + 1..].to_string())
    } else {
        (v, String::new())
    };
    let mut nums = Vec::new();
    let mut qualifier = String::new();
    for part in core.split(['.', '-']) {
        if part.is_empty() {
            continue;
        }
        if let Ok(n) = part.parse::<u64>() {
            nums.push(n);
        } else {
            qualifier = part.to_ascii_lowercase();
        }
    }
    MavenToken {
        nums,
        qualifier,
        vendor,
    }
}

fn qualifier_rank(q: &str) -> i32 {
    match q {
        "" | "ga" | "final" | "release" => 0,
        "sp" => 1,
        "rc" => -1,
        "milestone" | "m" => -2,
        "alpha" | "a" | "beta" | "b" | "snapshot" => -3,
        _ => -1,
    }
}

fn cmp_maven(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let aa = parse_maven(a);
    let bb = parse_maven(b);
    let len = aa.nums.len().max(bb.nums.len());
    for i in 0..len {
        let x = aa.nums.get(i).copied().unwrap_or(0);
        let y = bb.nums.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    match qualifier_rank(&aa.qualifier).cmp(&qualifier_rank(&bb.qualifier)) {
        Ordering::Equal => {}
        o => return o,
    }
    aa.vendor.cmp(&bb.vendor)
}

/// SemVer 2.0 (plus NuGet 4-segment / case-insensitive pre-release option).
/// Ignores `+build` metadata per SemVer §10 and compares dot-separated pre-release identifiers
/// numerically when both are digits and lexically otherwise per SemVer §11.
fn cmp_semver_strict(a: &str, b: &str, case_insensitive_pre: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let parse = |raw: &str| -> (Vec<u64>, String) {
        let no_build = raw.split('+').next().unwrap_or(raw).trim();
        let (core, pre) = no_build.split_once('-').unwrap_or((no_build, ""));
        let nums: Vec<u64> = core.split('.').filter_map(|p| p.parse().ok()).collect();
        let pre_norm = if case_insensitive_pre {
            pre.to_ascii_lowercase()
        } else {
            pre.to_string()
        };
        (nums, pre_norm)
    };
    let (an, ap) = parse(a);
    let (bn, bp) = parse(b);
    let len = an.len().max(bn.len());
    for i in 0..len {
        let x = an.get(i).copied().unwrap_or(0);
        let y = bn.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    match (ap.is_empty(), bp.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            let mut ai = ap.split('.');
            let mut bi = bp.split('.');
            loop {
                match (ai.next(), bi.next()) {
                    (None, None) => return Ordering::Equal,
                    (None, Some(_)) => return Ordering::Less,
                    (Some(_), None) => return Ordering::Greater,
                    (Some(x), Some(y)) => {
                        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                            (Ok(xn), Ok(yn)) => xn.cmp(&yn),
                            (Ok(_), Err(_)) => Ordering::Less, // SemVer §11.4.1.3: numeric < non-numeric
                            (Err(_), Ok(_)) => Ordering::Greater,
                            (Err(_), Err(_)) => x.cmp(y),
                        };
                        if ord != Ordering::Equal {
                            return ord;
                        }
                    }
                }
            }
        }
    }
}

/// PEP 440 version parser and comparator (`[N!]N(.N)*[{a|b|rc}N][.postN][.devN]`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pep440Version {
    epoch: u64,
    release: Vec<u64>,
    /// Pre-release phase rank (`1 = a`, `2 = b`, `3 = rc`, `4 = no pre-release`, `0 = pure dev`).
    pre_phase: i32,
    pre_num: u64,
    /// Post-release (`None` < `Some(n)`).
    post: Option<u64>,
    /// Dev-release (`Some(n)` < `None` within the same phase/post).
    dev: Option<u64>,
}

fn parse_pep440(raw: &str) -> Pep440Version {
    let s = raw
        .trim()
        .trim_start_matches('v')
        .split('+')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let (epoch, rest) = if let Some((ep, r)) = s.split_once('!') {
        (ep.parse::<u64>().unwrap_or(0), r)
    } else {
        (0, s.as_str())
    };

    let mut release = Vec::new();
    let mut pre: Option<(i32, u64)> = None;
    let mut post: Option<u64> = None;
    let mut dev: Option<u64> = None;

    for tok in rest.split(['.', '-', '_']).filter(|t| !t.is_empty()) {
        if let Ok(n) = tok.parse::<u64>() {
            if pre.is_none() && post.is_none() && dev.is_none() {
                release.push(n);
            } else if post.is_some() && dev.is_none() && post == Some(0) {
                post = Some(n);
            } else if dev.is_some() && dev == Some(0) {
                dev = Some(n);
            } else if let Some((phase, 0)) = pre
                && post.is_none()
                && dev.is_none()
            {
                pre = Some((phase, n));
            }
            continue;
        }
        if let Some(num) = tok
            .strip_prefix("post")
            .or_else(|| tok.strip_prefix("rev"))
            .or_else(|| tok.strip_prefix('r'))
            && num.chars().all(|c| c.is_ascii_digit())
        {
            post = Some(num.parse::<u64>().unwrap_or(0));
            continue;
        }
        if let Some(num) = tok.strip_prefix("dev")
            && num.chars().all(|c| c.is_ascii_digit())
        {
            dev = Some(num.parse::<u64>().unwrap_or(0));
            continue;
        }
        // Inline digits + pre-release suffix like `0a1`, `2rc3`, or standalone `alpha1` / `beta2`.
        let split_idx = tok.find(|c: char| !c.is_ascii_digit()).unwrap_or(tok.len());
        if split_idx > 0
            && pre.is_none()
            && post.is_none()
            && dev.is_none()
            && let Ok(n) = tok[..split_idx].parse::<u64>()
        {
            release.push(n);
        }
        let tail = &tok[split_idx..];
        let (phase_opt, tail_num) = if let Some(n) = tail
            .strip_prefix("alpha")
            .or_else(|| tail.strip_prefix('a'))
        {
            (Some(1), n)
        } else if let Some(n) = tail.strip_prefix("beta").or_else(|| tail.strip_prefix('b')) {
            (Some(2), n)
        } else if let Some(n) = tail
            .strip_prefix("rc")
            .or_else(|| tail.strip_prefix("pre"))
            .or_else(|| tail.strip_prefix("preview"))
            .or_else(|| tail.strip_prefix('c'))
        {
            (Some(3), n)
        } else {
            (None, "")
        };
        if let Some(p) = phase_opt {
            pre = Some((p, tail_num.parse::<u64>().unwrap_or(0)));
        }
    }

    // Trim trailing zeroes on release tuple per PEP 440 (`1.0.0 == 1.0`).
    while release.len() > 1 && release.last() == Some(&0) {
        release.pop();
    }

    let (pre_phase, pre_num) = match (pre, post, dev) {
        // `1.0.dev1` (no pre, no post, has dev) sorts BEFORE `1.0a1` per PEP 440!
        (None, None, Some(_)) => (0, 0),
        (Some((p, n)), _, _) => (p, n),
        (None, _, _) => (4, 0),
    };

    Pep440Version {
        epoch,
        release,
        pre_phase,
        pre_num,
        post,
        dev,
    }
}

fn cmp_pep440(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let va = parse_pep440(a);
    let vb = parse_pep440(b);
    match va.epoch.cmp(&vb.epoch) {
        Ordering::Equal => {}
        o => return o,
    }
    let len = va.release.len().max(vb.release.len());
    for i in 0..len {
        let x = va.release.get(i).copied().unwrap_or(0);
        let y = vb.release.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    match va.pre_phase.cmp(&vb.pre_phase) {
        Ordering::Equal => {}
        o => return o,
    }
    match va.pre_num.cmp(&vb.pre_num) {
        Ordering::Equal => {}
        o => return o,
    }
    let post_ord = match (va.post, vb.post) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(x), Some(y)) => x.cmp(&y),
    };
    if post_ord != Ordering::Equal {
        return post_ord;
    }
    match (va.dev, vb.dev) {
        (None, None) => Ordering::Equal,
        (Some(_), None) => Ordering::Less, // dev < non-dev
        (None, Some(_)) => Ordering::Greater,
        (Some(x), Some(y)) => x.cmp(&y),
    }
}

fn strip_v_prefix(s: &str) -> &str {
    let s = s.trim();
    s.strip_prefix('v')
        .or_else(|| s.strip_prefix('V'))
        .unwrap_or(s)
}

/// Composer / Packagist comparator (`v`-prefix stripped; `dev < alpha < beta < rc < stable < patch`).
fn cmp_packagist(a: &str, b: &str) -> std::cmp::Ordering {
    cmp_semver_strict(strip_v_prefix(a), strip_v_prefix(b), true)
}

/// RubyGems `Gem::Version` comparator: dot-split numeric/alphanumeric segments where
/// trailing numeric `0`s are insignificant and string segments (e.g. `rc`, `pre`, `beta`)
/// sort strictly less than numeric segments (`7.0.4.rc1 < 7.0.4 < 7.0.4.1`).
fn cmp_rubygems(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Seg {
        Num(u64),
        Str(String),
    }
    let tokenize = |v: &str| -> Vec<Seg> {
        let mut segs = Vec::new();
        for part in v.trim().split(['.', '-']).filter(|s| !s.is_empty()) {
            let mut cur_digits = String::new();
            let mut cur_alpha = String::new();
            for ch in part.chars() {
                if ch.is_ascii_digit() {
                    if !cur_alpha.is_empty() {
                        segs.push(Seg::Str(std::mem::take(&mut cur_alpha)));
                    }
                    cur_digits.push(ch);
                } else if ch.is_ascii_alphanumeric() {
                    if !cur_digits.is_empty() {
                        if let Ok(n) = cur_digits.parse::<u64>() {
                            segs.push(Seg::Num(n));
                        }
                        cur_digits.clear();
                    }
                    cur_alpha.push(ch);
                }
            }
            if !cur_digits.is_empty()
                && let Ok(n) = cur_digits.parse::<u64>()
            {
                segs.push(Seg::Num(n));
            }
            if !cur_alpha.is_empty() {
                segs.push(Seg::Str(cur_alpha));
            }
        }
        segs
    };
    let sa = tokenize(a);
    let sb = tokenize(b);
    let len = sa.len().max(sb.len());
    for i in 0..len {
        let xa = sa.get(i).cloned().unwrap_or(Seg::Num(0));
        let xb = sb.get(i).cloned().unwrap_or(Seg::Num(0));
        let ord = match (xa, xb) {
            (Seg::Num(x), Seg::Num(y)) => x.cmp(&y),
            (Seg::Str(_), Seg::Num(_)) => Ordering::Less,
            (Seg::Num(_), Seg::Str(_)) => Ordering::Greater,
            (Seg::Str(x), Seg::Str(y)) => x.cmp(&y),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osv::{VersionInterval, VersionUpperBound};

    #[test]
    fn maven_jackson_range() {
        let e = engine_for_ecosystem("Maven").unwrap();
        assert!(
            e.in_vulnerable_range("2.13.5", Some("2.0.0"), Some("2.14.0"))
                .unwrap()
        );
        assert!(
            !e.in_vulnerable_range("2.14.0", Some("2.0.0"), Some("2.14.0"))
                .unwrap()
        );
    }

    #[test]
    fn maven_vendor_fixed_exact() {
        let e = engine_for_ecosystem("Maven").unwrap();
        let fixed = "2.7.2.Final-rhlw-00004";
        assert!(
            !e.in_vulnerable_range(fixed, Some("0"), Some(fixed))
                .unwrap()
        );
        assert!(
            e.in_vulnerable_range("2.7.2.Final", Some("0"), Some(fixed))
                .unwrap()
        );
    }

    #[test]
    fn cargo_and_npm() {
        let c = engine_for_ecosystem("crates.io").unwrap();
        assert!(
            c.in_vulnerable_range("1.2.3", Some("1.0.0"), Some("1.3.0"))
                .unwrap()
        );
        let n = engine_for_ecosystem("npm").unwrap();
        assert!(
            !n.in_vulnerable_range("2.0.0", Some("1.0.0"), Some("2.0.0"))
                .unwrap()
        );
    }

    #[test]
    fn unsupported() {
        assert!(engine_for_ecosystem("MadeUpEco").is_err());
    }

    #[test]
    fn multi_range_earlier_branch_matched() {
        let ranges = vec![
            VersionInterval {
                introduced: Some("1.0.0".into()),
                upper: VersionUpperBound::Fixed("1.2.5".into()),
            },
            VersionInterval {
                introduced: Some("2.0.0".into()),
                upper: VersionUpperBound::Fixed("2.1.3".into()),
            },
        ];
        let e = engine_for_ecosystem("Maven").unwrap();
        assert!(e.in_any_vulnerable_range("1.2.4", &ranges, &[]).unwrap());
        assert!(!e.in_any_vulnerable_range("1.2.5", &ranges, &[]).unwrap());
        assert!(e.in_any_vulnerable_range("2.0.5", &ranges, &[]).unwrap());
        assert!(!e.in_any_vulnerable_range("2.1.3", &ranges, &[]).unwrap());
        assert!(!e.in_any_vulnerable_range("1.9.0", &ranges, &[]).unwrap());
    }

    #[test]
    fn last_affected_inclusive_bound() {
        let ranges = vec![VersionInterval {
            introduced: Some("1.0.0".into()),
            upper: VersionUpperBound::LastAffected("1.4.2".into()),
        }];
        let e = engine_for_ecosystem("npm").unwrap();
        assert!(e.in_any_vulnerable_range("1.4.2", &ranges, &[]).unwrap());
        assert!(!e.in_any_vulnerable_range("1.4.3", &ranges, &[]).unwrap());
        assert!(e.in_any_vulnerable_range("1.0.0", &ranges, &[]).unwrap());
    }

    #[test]
    fn listed_versions_fallback_when_no_ranges() {
        let listed = vec!["1.2.3".to_string(), "1.3.0".to_string()];
        let e = engine_for_ecosystem("crates.io").unwrap();
        assert!(e.in_any_vulnerable_range("1.2.3", &[], &listed).unwrap());
        assert!(!e.in_any_vulnerable_range("1.2.4", &[], &listed).unwrap());
    }

    #[test]
    fn pep440_ordering_and_epochs() {
        let py = engine_for_ecosystem("PyPI").unwrap();
        let ordered = [
            "1.0.dev1",
            "1.0a1.dev1",
            "1.0a1",
            "1.0b2",
            "1.0rc1",
            "1.0",
            "1.0.post1.dev1",
            "1.0.post1",
            "1.0.1",
            "1!0.1.0",
        ];
        for win in ordered.windows(2) {
            assert_eq!(
                py.cmp_versions(win[0], win[1]).unwrap(),
                std::cmp::Ordering::Less,
                "expected {} < {}",
                win[0],
                win[1]
            );
        }
        assert_eq!(
            py.cmp_versions("1.0.0", "1.0").unwrap(),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn rubygems_nuget_packagist_and_tier1_ecosystems() {
        let gem = engine_for_ecosystem("RubyGems").unwrap();
        assert!(
            gem.in_vulnerable_range("7.0.4.rc1", Some("7.0.0"), Some("7.0.4"))
                .unwrap()
        );
        assert!(
            !gem.in_vulnerable_range("7.0.4", Some("7.0.0"), Some("7.0.4"))
                .unwrap()
        );
        assert!(
            gem.in_vulnerable_range("7.0.4.1", Some("7.0.4"), Some("7.0.5"))
                .unwrap()
        );

        let nuget = engine_for_ecosystem("NuGet").unwrap();
        assert!(
            nuget
                .in_vulnerable_range("13.0.1-beta1", Some("13.0.0"), Some("13.0.1"))
                .unwrap()
        );

        let composer = engine_for_ecosystem("Packagist").unwrap();
        assert!(
            composer
                .in_vulnerable_range("v6.3.0-RC1", Some("6.0.0"), Some("6.3.0"))
                .unwrap()
        );

        for eco in [
            "Pub",
            "Hex",
            "Swift",
            "Puppet Forge",
            "LuaRocks",
            "Conan",
            "vcpkg",
        ] {
            let eng = engine_for_ecosystem(eco).expect(eco);
            assert!(
                eng.in_vulnerable_range("1.2.0", Some("1.0.0"), Some("1.3.0"))
                    .unwrap()
            );
        }
    }
}
