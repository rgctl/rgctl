//! Grouped top-level help (clap cannot attach headings to subcommands).

use clap::{Command, CommandFactory};

/// Help template for the root command: omit clap's flat `{subcommands}` list and
/// inject our grouped Commands section via `{after-help}` (placed before Options).
/// Place grouped Commands in `{before-help}` (after usage) so clap does not prepend
/// the extra blank lines that `{after-help}` inserts. `{options}` has no heading, so
/// we print `Options:` literally.
pub const ROOT_HELP_TEMPLATE: &str = "\
{about-with-newline}\
{usage-heading} {usage}\n\n\
{before-help}\
Options:\n\
{options}\
";

const GROUPS: &[(&str, &[&str])] = &[
    (
        "Lifecycle",
        &[
            "discover", "status", "update", "watch", "merge", "serve", "diff",
        ],
    ),
    (
        "Query",
        &[
            "find",
            "callers",
            "callees",
            "relations",
            "inventory",
            "query",
            "gql",
        ],
    ),
    (
        "Analysis",
        &[
            "blast-radius",
            "slice",
            "taint",
            "inspect",
            "metrics",
            "semantic",
            "communities",
            "clones",
            "cpg",
        ],
    ),
    ("Security", &["vuln", "deps", "security"]),
    ("Policy", &["check", "review", "pr-check", "rules"]),
    ("Meta", &["export", "install", "completions", "help"]),
];

/// Build the root `Command` with visually grouped subcommand help.
pub fn root_command<P: CommandFactory>() -> Command {
    let cmd = P::command();
    let grouped = render_grouped_commands(&cmd);
    cmd.help_template(ROOT_HELP_TEMPLATE).before_help(grouped)
}

/// Render:
/// ```text
/// Lifecycle:
///   discover  …
///   status    …
/// Query:
///   find      …
/// …
/// ```
pub fn render_grouped_commands(cmd: &Command) -> String {
    let mut abouts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut width = 12usize;
    for sub in cmd.get_subcommands() {
        if sub.is_hide_set() {
            continue;
        }
        let name = sub.get_name().to_string();
        width = width.max(name.len());
        let about = sub
            .get_about()
            .map(|s| s.to_string())
            .unwrap_or_default()
            .replace('\n', " ");
        abouts.insert(name, about);
    }
    // clap's auto `help` subcommand
    abouts
        .entry("help".into())
        .or_insert_with(|| "Print this message or the help of the given subcommand(s)".into());

    let mut out = String::from("Commands:\n");
    let mut seen = std::collections::HashSet::new();
    for (heading, names) in GROUPS {
        out.push_str(heading);
        out.push_str(":\n");
        for name in *names {
            seen.insert(*name);
            let about = abouts.get(*name).map(String::as_str).unwrap_or("");
            if about.is_empty() && *name != "help" {
                continue;
            }
            let _ =
                std::fmt::Write::write_fmt(&mut out, format_args!("  {name:width$}  {about}\n"));
        }
        out.push('\n');
    }

    // Any subcommands not in the map (future-proof).
    let mut extras: Vec<_> = abouts
        .keys()
        .filter(|k| !seen.contains(k.as_str()))
        .cloned()
        .collect();
    extras.sort();
    if !extras.is_empty() {
        out.push_str("Other:\n");
        for name in extras {
            let about = abouts.get(&name).map(String::as_str).unwrap_or("");
            let _ =
                std::fmt::Write::write_fmt(&mut out, format_args!("  {name:width$}  {about}\n"));
        }
        out.push('\n');
    }

    out.push_str(
        "  * gql is experimental — prefer find/callers/relations/inventory\n\
         Canonical verbs: see skill command encyclopedia\n\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;

    #[test]
    fn grouped_help_contains_section_headers() {
        let cmd = Cli::command();
        let text = render_grouped_commands(&cmd);
        for heading in [
            "Lifecycle:",
            "Query:",
            "Analysis:",
            "Security:",
            "Policy:",
            "Meta:",
        ] {
            assert!(text.contains(heading), "missing {heading} in:\n{text}");
        }
        assert!(text.contains("discover"));
        assert!(text.contains("find"));
        assert!(text.contains("vuln"));
    }
}
