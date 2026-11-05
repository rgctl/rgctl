//! `rgctl completions <shell>`: scripts are generated from the clap tree without a repo session.

use std::process::Command;

fn completions(shell: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::new(env!("CARGO_BIN_EXE_rgctl"))
        .args(["completions", shell])
        // An empty directory: completions must not need a repo, cache, or session.
        .current_dir(dir.path())
        .output()
        .expect("run rgctl completions");
    assert!(
        out.status.success(),
        "rgctl completions {shell} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "rgctl completions {shell} wrote to stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("completion script is UTF-8")
}

#[test]
fn generates_a_script_for_every_supported_shell() {
    for shell in ["bash", "zsh", "fish", "elvish", "powershell"] {
        let script = completions(shell);
        assert!(!script.is_empty(), "{shell} completion script is empty");
        assert!(script.contains("rgctl"), "{shell} script never names rgctl");
    }
}

#[test]
fn scripts_cover_subcommands_flags_and_enum_values() {
    let bash = completions("bash");
    for needle in [
        "discover",
        "blast-radius",
        "completions",
        "--format",
        "--with-cfg",
    ] {
        assert!(bash.contains(needle), "bash completions miss {needle}");
    }
    let fish = completions("fish");
    assert!(
        fish.contains("complete -c rgctl") && fish.contains("blast-radius"),
        "fish completions miss subcommands"
    );
    let zsh = completions("zsh");
    assert!(
        zsh.starts_with("#compdef rgctl"),
        "zsh script needs #compdef"
    );
    assert!(zsh.contains("json"), "zsh completions miss --format values");
}

#[test]
fn rejects_unknown_shell() {
    let out = Command::new(env!("CARGO_BIN_EXE_rgctl"))
        .args(["completions", "nushell"])
        .output()
        .expect("run rgctl completions");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("possible values"));
}
