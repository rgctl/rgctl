//! `rbuilder discover` — index and analyze a repository.

use super::context::CliContext;
use anyhow::Result;

pub struct DiscoverArgs {
    pub path: Option<String>,
    pub languages: Option<String>,
    pub exclude: Option<String>,
    pub security: bool,
    pub cfg: bool,
    pub all: bool,
}

pub fn run(ctx: &CliContext, args: DiscoverArgs) -> Result<()> {
    let path = args
        .path
        .as_deref()
        .map(|p| {
            if std::path::Path::new(p).is_absolute() {
                p.to_string()
            } else {
                ctx.repo.join(p).to_string_lossy().into_owned()
            }
        })
        .unwrap_or_else(|| ctx.repo.to_string_lossy().into_owned());

    super::discover_impl::run_full_analysis(
        &path,
        args.languages,
        args.exclude,
        ctx.verbose,
        args.security,
        args.cfg,
        args.all,
        &ctx.db,
    )
}
