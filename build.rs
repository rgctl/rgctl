use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let pack_root = manifest_dir.join("agent-pack");
    let embed_dir = out_dir.join("agent_pack");
    let zip_path = out_dir.join("agent_pack.zip");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());

    println!("cargo:rerun-if-changed=agent-pack/manifest.yaml");
    println!("cargo:rerun-if-changed=agent-pack/agents/registry.toml");
    println!("cargo:rerun-if-changed=agent-pack/commands");
    println!("cargo:rerun-if-changed=skills/rgctl/SKILL.md");
    println!("cargo:rerun-if-changed=skills/rgctl/README.md");
    println!("cargo:rerun-if-changed=skills/rgctl/references");
    println!("cargo:rerun-if-changed=skills/rgctl-review");

    rgctl_agent_pack_codegen::generate(&pack_root, &embed_dir, &version)
        .expect("agent-pack codegen failed");

    zip_tree(&embed_dir, &zip_path).expect("zip agent pack");

    let count = walkdir::WalkDir::new(&embed_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .count();
    assert!(count > 100, "agent pack embed too small: {count} files");
}

fn zip_tree(src: &Path, dest: &Path) -> std::io::Result<()> {
    let file = File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for ent in walkdir::WalkDir::new(src) {
        let ent = ent?;
        let path = ent.path();
        if !path.is_file() {
            continue;
        }
        let name = path.strip_prefix(src).expect("strip prefix");
        let name = name.to_string_lossy().replace('\\', "/");
        zip.start_file(name, opts)?;
        let mut f = File::open(path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        zip.write_all(&buf)?;
    }
    zip.finish()?;
    Ok(())
}
