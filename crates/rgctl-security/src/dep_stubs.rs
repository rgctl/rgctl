//! Opt-in transitive dependency call-stub extractor & stitcher (`.rgctl/dep_stubs.bin`).
//!
//! Policy: never runs on default `rgctl discover`. Invoked only when `--with-dep-stubs` is
//! passed to `rgctl vuln analyze` or `rgctl vuln scan`.
//!
//! Capabilities:
//! 1. Zero-decompile JVM `.jar` / `.war` `.class` binary constant-pool & `Code` invoke scanner
//!    (`0xCAFEBABE`, JVMS §4.1/§4.4/§4.7.3) — works on Gradle/shaded JARs even when
//!    `META-INF/maven/pom.xml` is absent.
//! 2. Lockfile-scoped lightweight source call-stub scanner for opted-in `node_modules` /
//!    `vendor` / `.venv` directories across JS/TS, Python, Ruby, PHP, Go, and Rust.
//! 3. Bounded backward call-chain stitcher connecting first-party `parked_relations.bin`
//!    call sites (`App.fn -> Wrapper.m`) to transitive vulnerable sinks (`Wrapper.m -> ... -> Sink.s`).

use crate::error::SecurityError;
use rgctl_extraction::ParkedRelationRecord;
use rgctl_plugin_api::RelationType;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::ZipArchive;

/// Relative filename under `{repo}/.rgctl/`.
pub const DEP_STUBS_FILE: &str = "dep_stubs.bin";

const DEP_STUBS_SCHEMA_VERSION: u32 = 1;
const MAX_JARS_FOR_STUBS: usize = 256;
const MAX_CLASSES_PER_JAR: usize = 20_000;
const MAX_SOURCE_FILES_FOR_STUBS: usize = 2_000;

/// One compact method-to-method call edge extracted from a third-party dependency archive/source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DepCallStub {
    /// Enclosing class or module (e.g. `com.enterprise.audit.AuditClient`).
    pub from_owner: String,
    /// Enclosing method or function (e.g. `logEvent`).
    pub from_method: String,
    /// Target class or module (e.g. `com.fasterxml.jackson.databind.ObjectMapper`).
    pub to_owner: String,
    /// Target method or function (e.g. `readValue`).
    pub to_method: String,
    /// Provenance archive or file path.
    pub provenance: String,
}

/// Persisted `.rgctl/dep_stubs.bin` payload.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepStubsIndex {
    pub schema_version: u32,
    pub stubs: Vec<DepCallStub>,
    /// `(class_fqn, provenance)` discovered inside scanned JARs (enables shaded-JAR detection).
    pub embedded_classes: Vec<(String, String)>,
}

/// A stitched transitive path from a first-party caller through 1+ dependency stubs to an OSV sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitiveStubHit {
    pub from_node_id: uuid::Uuid,
    pub caller_fqn: String,
    pub from_file: String,
    pub call_site_line: u32,
    /// Immediate wrapper target called by first-party code (e.g. `AuditClient.logEvent`).
    pub wrapper_target: String,
    /// Resolved OSV sink reached transitively (e.g. `com.fasterxml.jackson.databind.ObjectMapper.readValue`).
    pub target_sink: String,
    /// Ordered call chain from wrapper entry to OSV sink (`["AuditClient.logEvent", ..., "ObjectMapper.readValue"]`).
    pub stub_chain: Vec<String>,
}

/// Build or load `.rgctl/dep_stubs.bin` for `repo`.
pub fn load_or_build_dep_stubs(
    repo: &Path,
    jar_roots: &[PathBuf],
    node_modules_roots: &[PathBuf],
) -> Result<DepStubsIndex, SecurityError> {
    let index = build_dep_stubs(repo, jar_roots, node_modules_roots)?;
    let out_path = rgctl_graph::paths::artifact_path(repo, DEP_STUBS_FILE);
    if let Some(parent) = out_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(encoded) = bincode::serialize(&index) {
        let _ = fs::write(&out_path, encoded);
    }
    Ok(index)
}

/// Read an existing `.rgctl/dep_stubs.bin` file.
pub fn read_dep_stubs(path: &Path) -> Result<DepStubsIndex, SecurityError> {
    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    bincode::deserialize(&bytes)
        .map_err(|e| SecurityError::Msg(format!("decode {}: {e}", path.display())))
}

/// Scan opted-in JAR/WAR roots and dependency source roots (`node_modules`, `vendor`, `.venv`)
/// to build a deterministic [`DepStubsIndex`].
pub fn build_dep_stubs(
    repo: &Path,
    jar_roots: &[PathBuf],
    node_modules_roots: &[PathBuf],
) -> Result<DepStubsIndex, SecurityError> {
    let mut stubs_set: BTreeSet<DepCallStub> = BTreeSet::new();
    let mut classes_set: BTreeSet<(String, String)> = BTreeSet::new();

    // 1. JVM `.jar` / `.war` `.class` bytecode scanner.
    let mut jars_scanned = 0usize;
    for rel_root in jar_roots {
        let root = if rel_root.is_absolute() {
            rel_root.clone()
        } else {
            repo.join(rel_root)
        };
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
            if jars_scanned >= MAX_JARS_FOR_STUBS {
                break;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.path();
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase());
            if !matches!(ext.as_deref(), Some("jar" | "war")) {
                continue;
            }
            jars_scanned += 1;
            let _ = scan_jar_class_stubs(p, &mut stubs_set, &mut classes_set);
        }
    }

    // 2. Lockfile-scoped / opted-in source cache scanner (`node_modules`, `vendor`, `.venv`).
    let mut source_roots: Vec<PathBuf> = node_modules_roots
        .iter()
        .map(|r| {
            if r.is_absolute() {
                r.clone()
            } else {
                repo.join(r)
            }
        })
        .collect();
    for default_cache in ["vendor", ".venv"] {
        let candidate = repo.join(default_cache);
        if candidate.is_dir() && !source_roots.contains(&candidate) {
            source_roots.push(candidate);
        }
    }

    let mut source_files = 0usize;
    for sroot in source_roots {
        if !sroot.exists() {
            continue;
        }
        for entry in WalkDir::new(&sroot)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_str().unwrap_or("");
                !matches!(name, ".git" | "test" | "tests" | "__tests__" | "docs")
            })
            .filter_map(|e| e.ok())
        {
            if source_files >= MAX_SOURCE_FILES_FOR_STUBS {
                break;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.path();
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            if !matches!(
                ext,
                "js" | "mjs" | "cjs" | "ts" | "py" | "rb" | "php" | "go" | "rs"
            ) {
                continue;
            }
            let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if fname.ends_with(".min.js") || fname.ends_with(".d.ts") {
                continue;
            }
            source_files += 1;
            scan_source_file_stubs(p, &sroot, &mut stubs_set);
        }
    }

    Ok(DepStubsIndex {
        schema_version: DEP_STUBS_SCHEMA_VERSION,
        stubs: stubs_set.into_iter().collect(),
        embedded_classes: classes_set.into_iter().collect(),
    })
}

fn scan_jar_class_stubs(
    jar_path: &Path,
    stubs_out: &mut BTreeSet<DepCallStub>,
    classes_out: &mut BTreeSet<(String, String)>,
) -> Result<(), SecurityError> {
    let file = fs::File::open(jar_path).map_err(|e| SecurityError::Io {
        path: jar_path.display().to_string(),
        source: e,
    })?;
    let mut zip = ZipArchive::new(file)
        .map_err(|e| SecurityError::Msg(format!("zip {}: {e}", jar_path.display())))?;
    let prov = jar_path.display().to_string();
    let count = zip.len().min(MAX_CLASSES_PER_JAR);

    let mut buf = Vec::with_capacity(16 * 1024);
    for idx in 0..count {
        let Ok(mut zf) = zip.by_index(idx) else {
            continue;
        };
        if !zf.name().ends_with(".class") || zf.name().ends_with("module-info.class") {
            continue;
        }
        buf.clear();
        if zf.read_to_end(&mut buf).is_err() {
            continue;
        }
        if let Some((this_cls, edges)) = parse_jvm_class_bytes(&buf) {
            if !is_trivial_jdk_owner(&this_cls) {
                classes_out.insert((this_cls.clone(), prov.clone()));
            }
            for (from_method, to_owner, to_method) in edges {
                if is_trivial_jdk_owner(&to_owner)
                    || to_method == "<init>" && to_owner == "java.lang.Object"
                {
                    continue;
                }
                stubs_out.insert(DepCallStub {
                    from_owner: this_cls.clone(),
                    from_method,
                    to_owner,
                    to_method,
                    provenance: prov.clone(),
                });
            }
        }
    }
    Ok(())
}

fn is_trivial_jdk_owner(fqn: &str) -> bool {
    fqn.starts_with("java.lang.")
        || fqn.starts_with("java.util.")
        || fqn.starts_with("java.io.PrintStream")
        || fqn.starts_with("kotlin.jvm.internal.")
}

#[derive(Clone, Debug)]
enum CpEntry {
    Empty,
    Utf8(String),
    Class(u16),
    NameAndType(u16),
    MethodRef(u16, u16),
}

struct ByteCursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn read_u8(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }
    fn read_u16(&mut self) -> Option<u16> {
        let slice = self.data.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(u16::from_be_bytes([slice[0], slice[1]]))
    }
    fn read_u32(&mut self) -> Option<u32> {
        let slice = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }
    fn read_slice(&mut self, len: usize) -> Option<&'a [u8]> {
        let slice = self.data.get(self.pos..self.pos + len)?;
        self.pos += len;
        Some(slice)
    }
    fn skip(&mut self, len: usize) -> Option<()> {
        if self.pos.checked_add(len)? > self.data.len() {
            return None;
        }
        self.pos += len;
        Some(())
    }
}

/// `(from_method, target_class_fqn, target_method)` extracted from a JVM `.class` Methodref/InterfaceMethodref.
pub type JvmClassCallEdge = (String, String, String);

/// Zero-decompile JVM `.class` binary parser (JVMS §4.1, §4.4, §4.6, §4.7.3).
///
/// Extracts `(this_class_fqn, Vec<JvmClassCallEdge>)`.
pub fn parse_jvm_class_bytes(bytes: &[u8]) -> Option<(String, Vec<JvmClassCallEdge>)> {
    let mut cur = ByteCursor::new(bytes);
    if cur.read_u32()? != 0xCAFE_BABE {
        return None;
    }
    let _minor = cur.read_u16()?;
    let _major = cur.read_u16()?;
    let cp_count = cur.read_u16()? as usize;
    if cp_count == 0 {
        return None;
    }

    let mut cp = vec![CpEntry::Empty; cp_count];
    let mut idx = 1usize;
    while idx < cp_count {
        let tag = cur.read_u8()?;
        match tag {
            1 => {
                let len = cur.read_u16()? as usize;
                let raw = cur.read_slice(len)?;
                cp[idx] = CpEntry::Utf8(String::from_utf8_lossy(raw).into_owned());
                idx += 1;
            }
            3 | 4 => {
                cur.skip(4)?;
                idx += 1;
            }
            5 | 6 => {
                // Long & Double occupy two constant-pool slots per JVMS §4.4.5.
                cur.skip(8)?;
                idx += 2;
            }
            7 => {
                let name_idx = cur.read_u16()?;
                cp[idx] = CpEntry::Class(name_idx);
                idx += 1;
            }
            8 | 16 | 19 | 20 => {
                cur.skip(2)?;
                idx += 1;
            }
            9 => {
                cur.skip(4)?;
                idx += 1;
            }
            10 | 11 => {
                let cls_idx = cur.read_u16()?;
                let nat_idx = cur.read_u16()?;
                cp[idx] = CpEntry::MethodRef(cls_idx, nat_idx);
                idx += 1;
            }
            12 => {
                let name_idx = cur.read_u16()?;
                let _desc_idx = cur.read_u16()?;
                cp[idx] = CpEntry::NameAndType(name_idx);
                idx += 1;
            }
            15 => {
                cur.skip(3)?;
                idx += 1;
            }
            17 | 18 => {
                cur.skip(4)?;
                idx += 1;
            }
            _ => return None,
        }
    }

    let cp_utf8 = |i: u16| -> Option<&str> {
        match cp.get(i as usize)? {
            CpEntry::Utf8(s) => Some(s.as_str()),
            _ => None,
        }
    };
    let cp_class_fqn = |i: u16| -> Option<String> {
        match cp.get(i as usize)? {
            CpEntry::Class(name_idx) => {
                let internal = cp_utf8(*name_idx)?;
                Some(
                    internal
                        .trim_start_matches('[')
                        .trim_start_matches('L')
                        .trim_end_matches(';')
                        .replace('/', "."),
                )
            }
            _ => None,
        }
    };
    let cp_method_ref = |i: u16| -> Option<(String, String)> {
        match cp.get(i as usize)? {
            CpEntry::MethodRef(cls_idx, nat_idx) => {
                let owner = cp_class_fqn(*cls_idx)?;
                let CpEntry::NameAndType(m_idx) = cp.get(*nat_idx as usize)? else {
                    return None;
                };
                let m_name = cp_utf8(*m_idx)?.to_string();
                Some((owner, m_name))
            }
            _ => None,
        }
    };

    let _access = cur.read_u16()?;
    let this_class_idx = cur.read_u16()?;
    let _super_class_idx = cur.read_u16()?;
    let this_class = cp_class_fqn(this_class_idx)?;

    let interfaces_count = cur.read_u16()? as usize;
    cur.skip(interfaces_count.checked_mul(2)?)?;

    // Skip fields table.
    let fields_count = cur.read_u16()? as usize;
    for _ in 0..fields_count {
        cur.skip(6)?;
        let attrs = cur.read_u16()? as usize;
        for _ in 0..attrs {
            cur.skip(2)?;
            let alen = cur.read_u32()? as usize;
            cur.skip(alen)?;
        }
    }

    // Parse methods table & `Code` attributes.
    let methods_count = cur.read_u16()? as usize;
    let mut edges = Vec::new();
    let mut declared_methods = Vec::new();
    let mut saw_any_code_attr = false;

    for _ in 0..methods_count {
        let _m_access = cur.read_u16()?;
        let m_name_idx = cur.read_u16()?;
        let _m_desc_idx = cur.read_u16()?;
        let m_name = cp_utf8(m_name_idx).unwrap_or("").to_string();
        if !m_name.is_empty() && m_name != "<init>" && m_name != "<clinit>" {
            declared_methods.push(m_name.clone());
        }
        let attrs = cur.read_u16()? as usize;
        for _ in 0..attrs {
            let attr_name_idx = cur.read_u16()?;
            let alen = cur.read_u32()? as usize;
            let attr_bytes = cur.read_slice(alen)?;
            if cp_utf8(attr_name_idx) == Some("Code") && !m_name.is_empty() {
                saw_any_code_attr = true;
                for cp_idx in extract_invoke_cp_indices(attr_bytes) {
                    if let Some((target_owner, target_method)) = cp_method_ref(cp_idx) {
                        edges.push((m_name.clone(), target_owner, target_method));
                    }
                }
            }
        }
    }

    // Fallback for minimal synthetic / stripped constant-pool-only `.class` stubs without `Code` attrs.
    if !saw_any_code_attr && !declared_methods.is_empty() {
        for entry in &cp {
            if let CpEntry::MethodRef(cls_idx, nat_idx) = entry
                && let Some(owner) = cp_class_fqn(*cls_idx)
                && owner != this_class
                && let Some(CpEntry::NameAndType(m_idx)) = cp.get(*nat_idx as usize)
                && let Some(target_method) = cp_utf8(*m_idx)
            {
                for dm in &declared_methods {
                    edges.push((dm.clone(), owner.clone(), target_method.to_string()));
                }
            }
        }
    }

    Some((this_class, edges))
}

/// Walk a JVM `Code` attribute's instruction stream and return constant-pool indices referenced
/// by `invokevirtual` (`0xB6`), `invokespecial` (`0xB7`), `invokestatic` (`0xB8`), and
/// `invokeinterface` (`0xB9`).
fn extract_invoke_cp_indices(code_attr: &[u8]) -> Vec<u16> {
    // Code_attribute: u16 max_stack, u16 max_locals, u32 code_length, u8 code[code_length]
    if code_attr.len() < 8 {
        return Vec::new();
    }
    let code_len =
        u32::from_be_bytes([code_attr[4], code_attr[5], code_attr[6], code_attr[7]]) as usize;
    let Some(code) = code_attr.get(8..8 + code_len) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut pc = 0usize;
    while pc < code.len() {
        let op = code[pc];
        match op {
            0xB6..=0xB8 => {
                if let Some(hi) = code.get(pc + 1).copied()
                    && let Some(lo) = code.get(pc + 2).copied()
                {
                    out.push(u16::from_be_bytes([hi, lo]));
                }
                pc += 3;
            }
            0xB9 => {
                if let Some(hi) = code.get(pc + 1).copied()
                    && let Some(lo) = code.get(pc + 2).copied()
                {
                    out.push(u16::from_be_bytes([hi, lo]));
                }
                pc += 5;
            }
            0xBA => pc += 5, // invokedynamic
            // Variable-length switch instructions (4-byte aligned).
            0xAA => {
                // tableswitch
                let aligned = (pc + 4) & !3;
                let Some(bounds) = code.get(aligned + 4..aligned + 12) else {
                    break;
                };
                let low = i32::from_be_bytes([bounds[0], bounds[1], bounds[2], bounds[3]]) as i64;
                let high = i32::from_be_bytes([bounds[4], bounds[5], bounds[6], bounds[7]]) as i64;
                if high < low {
                    break;
                }
                let offsets = (high - low + 1) as usize;
                pc = aligned + 12 + offsets.saturating_mul(4);
            }
            0xAB => {
                // lookupswitch
                let aligned = (pc + 4) & !3;
                let Some(npairs_b) = code.get(aligned + 4..aligned + 8) else {
                    break;
                };
                let npairs =
                    i32::from_be_bytes([npairs_b[0], npairs_b[1], npairs_b[2], npairs_b[3]]).max(0)
                        as usize;
                pc = aligned + 8 + npairs.saturating_mul(8);
            }
            0xC4 => {
                // wide
                let Some(&sub) = code.get(pc + 1) else {
                    break;
                };
                pc += if sub == 0x84 { 6 } else { 4 };
            }
            // 2-byte instructions
            0x10 | 0x12 | 0x15..=0x19 | 0x36..=0x3A | 0xA9 | 0xBC => pc += 2,
            // 3-byte instructions
            0x11
            | 0x13
            | 0x14
            | 0x84
            | 0x99..=0xA8
            | 0xB2..=0xB5
            | 0xBB
            | 0xBD
            | 0xC0
            | 0xC1
            | 0xC6
            | 0xC7 => pc += 3,
            // 4-byte instructions
            0xC5 => pc += 4,
            // 5-byte instructions
            0xC8 | 0xC9 => pc += 5,
            // All other JVM instructions are 1 byte.
            _ => pc += 1,
        }
    }
    out
}

/// Lightweight source call-stub extractor for opted-in `node_modules` / `vendor` / `.venv` files.
fn scan_source_file_stubs(path: &Path, root: &Path, stubs_out: &mut BTreeSet<DepCallStub>) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    let rel = path.strip_prefix(root).unwrap_or(path);
    let pkg_owner = infer_package_owner_from_rel_path(rel);
    let prov = path.display().to_string();

    let mut current_fn = String::from("<module>");
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        if let Some(fn_name) = detect_declared_fn_name(trimmed) {
            current_fn = fn_name;
        }
        for (recv, method) in extract_dotted_calls(trimmed) {
            stubs_out.insert(DepCallStub {
                from_owner: pkg_owner.clone(),
                from_method: current_fn.clone(),
                to_owner: recv,
                to_method: method,
                provenance: prov.clone(),
            });
        }
    }
}

fn infer_package_owner_from_rel_path(rel: &Path) -> String {
    let comps: Vec<&str> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    if let Some(&first) = comps.first() {
        if first.starts_with('@')
            && let Some(&second) = comps.get(1)
        {
            return format!("{first}/{second}");
        }
        return first
            .trim_end_matches(".py")
            .trim_end_matches(".js")
            .trim_end_matches(".rb")
            .to_string();
    }
    "dep".to_string()
}

fn detect_declared_fn_name(trimmed: &str) -> Option<String> {
    for prefix in [
        "export async function ",
        "export function ",
        "async function ",
        "function ",
        "public function ",
        "def ",
        "pub fn ",
        "fn ",
        "func ",
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn extract_dotted_calls(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        if b != b'(' || idx == 0 {
            continue;
        }
        // Walk backward over `receiver.method` or `Receiver::method` or `$recv->method`.
        let before = &line[..idx];
        let token: String = before
            .chars()
            .rev()
            .take_while(|c| {
                c.is_ascii_alphanumeric() || matches!(*c, '_' | '.' | ':' | '\\' | '-' | '>')
            })
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        let normalized = token.replace("->", ".").replace("::", ".");
        if let Some((owner, method)) = normalized.rsplit_once('.')
            && !owner.is_empty()
            && !method.is_empty()
        {
            out.push((
                owner.trim_start_matches('$').to_string(),
                method.to_string(),
            ));
        }
    }
    out
}

/// Stitch first-party `parked_relations` through `stubs` (up to `max_hops`) to any `osv_methods`.
pub fn find_transitive_stub_paths(
    parked: &[ParkedRelationRecord],
    stubs: &DepStubsIndex,
    osv_methods: &[String],
    max_hops: usize,
) -> Vec<TransitiveStubHit> {
    if stubs.stubs.is_empty() || osv_methods.is_empty() || max_hops == 0 {
        return Vec::new();
    }

    // Index stubs by callee short method name for fast backward BFS.
    let mut by_to_method: HashMap<&str, Vec<&DepCallStub>> = HashMap::new();
    for stub in &stubs.stubs {
        by_to_method
            .entry(stub.to_method.as_str())
            .or_default()
            .push(stub);
    }

    // Backward BFS from each OSV sink method to reachable wrapper entrypoints:
    // `(wrapper_owner, wrapper_method) -> (target_osv_sink, Vec<chain_steps>)`
    let mut wrapper_to_sink: HashMap<(String, String), (String, Vec<String>)> = HashMap::new();

    for osv_m in osv_methods {
        let sep = if osv_m.contains("::") { "::" } else { "." };
        let mut parts = osv_m.rsplitn(2, sep);
        let Some(sink_method) = parts.next() else {
            continue;
        };
        let sink_owner_hint = parts.next();

        let mut queue: VecDeque<(&DepCallStub, Vec<String>)> = VecDeque::new();
        let mut visited: HashSet<(&str, &str)> = HashSet::new();

        if let Some(direct_callers) = by_to_method.get(sink_method) {
            for &stub in direct_callers {
                let owner_ok = match sink_owner_hint {
                    Some(owner) => owners_compatible(&stub.to_owner, owner),
                    None => true,
                };
                if owner_ok && visited.insert((stub.from_owner.as_str(), stub.from_method.as_str()))
                {
                    let chain = vec![
                        format!("{}.{}", stub.from_owner, stub.from_method),
                        osv_m.clone(),
                    ];
                    queue.push_back((stub, chain));
                }
            }
        }

        while let Some((curr, chain)) = queue.pop_front() {
            let key = (curr.from_owner.clone(), curr.from_method.clone());
            wrapper_to_sink
                .entry(key)
                .or_insert_with(|| (osv_m.clone(), chain.clone()));

            if chain.len() > max_hops {
                continue;
            }
            if let Some(upstreams) = by_to_method.get(curr.from_method.as_str()) {
                for &up in upstreams {
                    if owners_compatible(&up.to_owner, &curr.from_owner)
                        && visited.insert((up.from_owner.as_str(), up.from_method.as_str()))
                    {
                        let mut next_chain = Vec::with_capacity(chain.len() + 1);
                        next_chain.push(format!("{}.{}", up.from_owner, up.from_method));
                        next_chain.extend(chain.iter().cloned());
                        queue.push_back((up, next_chain));
                    }
                }
            }
        }
    }

    if wrapper_to_sink.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    for rec in parked {
        if rec.relation_type != RelationType::Calls {
            continue;
        }
        let clean_to_name = rec.to_name.split('\u{1f}').next().unwrap_or(&rec.to_name);
        let called_short = clean_to_name
            .rsplit(['.', ':', '\\'])
            .next()
            .unwrap_or(clean_to_name);
        for ((w_owner, w_method), (target_sink, chain)) in &wrapper_to_sink {
            if called_short != w_method {
                continue;
            }
            let w_simple = w_owner
                .rsplit(['.', '/', ':', '\\'])
                .next()
                .unwrap_or(w_owner.as_str());
            let owner_matches = rec.to_name.contains(w_simple)
                || rec
                    .to_qualified_hint
                    .as_deref()
                    .is_some_and(|h| h.contains(w_simple))
                || rec
                    .to_type_hint
                    .as_deref()
                    .is_some_and(|t| t.contains(w_simple))
                || rec
                    .import_module_hint
                    .as_deref()
                    .is_some_and(|i| i.contains(w_simple) || i.contains(w_owner));
            if owner_matches {
                hits.push(TransitiveStubHit {
                    from_node_id: rec.from_node_id,
                    caller_fqn: rec.from_qualified_name.clone(),
                    from_file: rec.from_file.clone(),
                    call_site_line: rec.call_site_line,
                    wrapper_target: format!("{w_simple}.{w_method}"),
                    target_sink: target_sink.clone(),
                    stub_chain: chain.clone(),
                });
            }
        }
    }
    hits
}

fn owners_compatible(stub_owner: &str, expected_owner: &str) -> bool {
    let a = stub_owner.replace(['/', ':'], ".");
    let b = expected_owner.replace(['/', ':'], ".");
    if a.eq_ignore_ascii_case(&b) || a.ends_with(&format!(".{b}")) || b.ends_with(&format!(".{a}"))
    {
        return true;
    }
    let a_simple = a.rsplit('.').next().unwrap_or(&a);
    let b_simple = b.rsplit('.').next().unwrap_or(&b);
    a_simple.eq_ignore_ascii_case(b_simple)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use uuid::Uuid;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    /// Construct a valid JVM `.class` binary with a `Code` attribute containing `invokevirtual` (`0xB6`)
    /// from `this_class.from_method` → `target_class.target_method`.
    fn synthesize_jvm_class_with_invoke(
        this_internal: &str,
        from_method: &str,
        target_internal: &str,
        target_method: &str,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0xCAFE_BABEu32.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes()); // minor
        out.extend_from_slice(&52u16.to_be_bytes()); // major (Java 8)

        // Constant pool entries (1..=9, cp_count = 10):
        // 1: Utf8(this_internal)
        // 2: Class(#1)
        // 3: Utf8("java/lang/Object")
        // 4: Class(#3)
        // 5: Utf8(from_method)
        // 6: Utf8("(Ljava/lang/String;)V")
        // 7: Utf8("Code")
        // 8: Utf8(target_internal)
        // 9: Class(#8)
        // 10: Utf8(target_method)
        // 11: NameAndType(#10, #6)
        // 12: Methodref(#9, #11)
        out.extend_from_slice(&13u16.to_be_bytes());
        let push_utf8 = |buf: &mut Vec<u8>, s: &str| {
            buf.push(1);
            buf.extend_from_slice(&(s.len() as u16).to_be_bytes());
            buf.extend_from_slice(s.as_bytes());
        };
        push_utf8(&mut out, this_internal); // #1
        out.extend_from_slice(&[7, 0, 1]); // #2 Class(#1)
        push_utf8(&mut out, "java/lang/Object"); // #3
        out.extend_from_slice(&[7, 0, 3]); // #4 Class(#3)
        push_utf8(&mut out, from_method); // #5
        push_utf8(&mut out, "(Ljava/lang/String;)V"); // #6
        push_utf8(&mut out, "Code"); // #7
        push_utf8(&mut out, target_internal); // #8
        out.extend_from_slice(&[7, 0, 8]); // #9 Class(#8)
        push_utf8(&mut out, target_method); // #10
        out.extend_from_slice(&[12, 0, 10, 0, 6]); // #11 NameAndType(#10, #6)
        out.extend_from_slice(&[10, 0, 9, 0, 11]); // #12 Methodref(#9, #11)

        out.extend_from_slice(&0x0021u16.to_be_bytes()); // access_flags
        out.extend_from_slice(&2u16.to_be_bytes()); // this_class = #2
        out.extend_from_slice(&4u16.to_be_bytes()); // super_class = #4
        out.extend_from_slice(&0u16.to_be_bytes()); // interfaces_count = 0
        out.extend_from_slice(&0u16.to_be_bytes()); // fields_count = 0
        out.extend_from_slice(&1u16.to_be_bytes()); // methods_count = 1

        // Method: public void <from_method>(String)
        out.extend_from_slice(&0x0001u16.to_be_bytes());
        out.extend_from_slice(&5u16.to_be_bytes()); // name_index = #5
        out.extend_from_slice(&6u16.to_be_bytes()); // descriptor_index = #6
        out.extend_from_slice(&1u16.to_be_bytes()); // attributes_count = 1

        // Code attribute (#7): invokevirtual #12; return
        let bytecode: [u8; 4] = [0xB6, 0x00, 12, 0xB1];
        let mut code_attr = Vec::new();
        code_attr.extend_from_slice(&2u16.to_be_bytes()); // max_stack
        code_attr.extend_from_slice(&2u16.to_be_bytes()); // max_locals
        code_attr.extend_from_slice(&(bytecode.len() as u32).to_be_bytes());
        code_attr.extend_from_slice(&bytecode);
        code_attr.extend_from_slice(&0u16.to_be_bytes()); // exception_table_length
        code_attr.extend_from_slice(&0u16.to_be_bytes()); // attributes_count

        out.extend_from_slice(&7u16.to_be_bytes()); // attribute_name_index = "Code"
        out.extend_from_slice(&(code_attr.len() as u32).to_be_bytes());
        out.extend_from_slice(&code_attr);

        out.extend_from_slice(&0u16.to_be_bytes()); // class attributes_count = 0
        out
    }

    #[test]
    fn shaded_gradle_jar_without_pom_stitches_wrapper_to_jackson_readvalue() {
        let dir = tempdir().unwrap();
        let lib_dir = dir.path().join("lib");
        fs::create_dir_all(&lib_dir).unwrap();
        let jar_path = lib_dir.join("audit-client.jar");
        {
            let f = fs::File::create(&jar_path).unwrap();
            let mut zip = ZipWriter::new(f);
            // Notice: NO META-INF/maven/pom.xml (Gradle / shaded JAR!)
            zip.start_file(
                "com/enterprise/audit/AuditClient.class",
                SimpleFileOptions::default(),
            )
            .unwrap();
            let class_bytes = synthesize_jvm_class_with_invoke(
                "com/enterprise/audit/AuditClient",
                "logEvent",
                "com/fasterxml/jackson/databind/ObjectMapper",
                "readValue",
            );
            zip.write_all(&class_bytes).unwrap();
            zip.finish().unwrap();
        }

        let index = load_or_build_dep_stubs(dir.path(), &[PathBuf::from("lib")], &[]).unwrap();
        assert_eq!(index.stubs.len(), 1);
        assert_eq!(
            index.stubs[0].from_owner,
            "com.enterprise.audit.AuditClient"
        );
        assert_eq!(index.stubs[0].from_method, "logEvent");
        assert_eq!(
            index.stubs[0].to_owner,
            "com.fasterxml.jackson.databind.ObjectMapper"
        );
        assert_eq!(index.stubs[0].to_method, "readValue");

        let parked = vec![ParkedRelationRecord {
            from_node_id: Uuid::new_v4(),
            from_qualified_name: "OrderController.submit".into(),
            from_file: "src/OrderController.java".into(),
            to_name: "auditClient.logEvent".into(),
            to_qualified_hint: None,
            to_type_hint: Some("AuditClient".into()),
            import_module_hint: Some("com.enterprise.audit.AuditClient".into()),
            relation_type: RelationType::Calls,
            call_site_line: 42,
            stub_node_id: None,
        }];
        let stitched = find_transitive_stub_paths(
            &parked,
            &index,
            &["com.fasterxml.jackson.databind.ObjectMapper.readValue".into()],
            4,
        );
        assert_eq!(stitched.len(), 1);
        assert_eq!(stitched[0].wrapper_target, "AuditClient.logEvent");
        assert_eq!(
            stitched[0].stub_chain,
            vec![
                "com.enterprise.audit.AuditClient.logEvent",
                "com.fasterxml.jackson.databind.ObjectMapper.readValue"
            ]
        );
    }
}
