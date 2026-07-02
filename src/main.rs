//! rBuilder CLI entry point.

use clap::Parser;
use rbuilder::cli::Cli;

fn main() -> anyhow::Result<()> {
    rbuilder::init();
    Cli::parse().run()
}
