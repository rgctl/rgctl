//! `rgctl completions <shell>`: shell completion scripts generated from the clap command tree.

use std::io::Write;
use std::path::Path;

use anyhow::Context;
use clap_complete::Shell;

use super::Cli;

/// Binary name the generated scripts register completions for.
const BIN_NAME: &str = "rgctl";

/// Write the completion script for `shell` to `output` (or stdout when `None`).
pub(super) fn run(shell: Shell, output: Option<&Path>) -> anyhow::Result<()> {
    let script = generate(shell);
    match output {
        Some(path) => std::fs::write(path, &script)
            .with_context(|| format!("write completion script to {}", path.display())),
        None => {
            let mut stdout = std::io::stdout().lock();
            match stdout.write_all(&script).and_then(|()| stdout.flush()) {
                // `rgctl completions bash | head` closing the pipe early is not an error.
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                res => res.context("write completion script to stdout"),
            }
        }
    }
}

/// Render the completion script for `shell` from the same `Command` tree as `--help`.
pub(super) fn generate(shell: Shell) -> Vec<u8> {
    let mut cmd = super::root_command::<Cli>();
    let mut buf = Vec::new();
    clap_complete::generate(shell, &mut cmd, BIN_NAME, &mut buf);
    buf
}
