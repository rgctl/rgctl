//! Interactive chat CLI command

use crate::output::formatter::{format_complexity_level, format_count};
use rbuilder_error::Result;
use rbuilder_graph::CodeGraph;
use rbuilder_nlp::conversation::ConversationContext;
use rbuilder_nlp::pattern_matcher::{PatternMatcher, QueryResult};
use std::io::{self, BufRead, Write};
use std::path::Path;

/// Run `rbuilder chat` interactive REPL.
pub fn run_chat(repo_root: &Path) -> Result<()> {
    let graph = CodeGraph::load_from_repo(repo_root)?;
    let matcher = PatternMatcher::from_graph(graph.backend())?;
    let backend = graph.backend();
    let mut ctx = ConversationContext::new();

    println!("rBuilder Chat — ask questions about your codebase");
    println!("Type 'exit' or 'quit' to leave, 'history' to show recent queries.\n");

    let stdin = io::stdin();
    loop {
        print!("rBuilder> ");
        io::stdout().flush().ok();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_err() {
            break;
        }

        let question = line.trim();
        if question.is_empty() {
            continue;
        }

        match question.to_ascii_lowercase().as_str() {
            "exit" | "quit" => {
                println!("Goodbye!");
                break;
            }
            "history" => {
                if ctx.history().count() == 0 {
                    println!("No history yet.");
                } else {
                    for (i, q) in ctx.history().enumerate() {
                        println!("  {}. {}", i + 1, q);
                    }
                }
                continue;
            }
            "help" => {
                print_help();
                continue;
            }
            _ => {}
        }

        let resolved = ctx.resolve_references(question);
        ctx.add_query(question);

        let translated = match matcher.translate(&resolved) {
            Ok(t) => t,
            Err(e) => {
                println!("Could not understand question: {e}");
                continue;
            }
        };

        let result = match matcher.execute(&translated, backend) {
            Ok(r) => r,
            Err(e) => {
                println!("Query failed: {e}");
                continue;
            }
        };

        ctx.update_from_result(question, &result);
        print_result(&result, backend, &matcher, &resolved);
    }

    Ok(())
}

fn print_result(
    result: &QueryResult,
    backend: &rbuilder_graph::backend::MemoryBackend,
    matcher: &PatternMatcher,
    question: &str,
) {
    match result {
        QueryResult::Count(n) => {
            println!("{}", format_count("result(s)", *n));
        }
        QueryResult::Nodes(nodes) => {
            if nodes.is_empty() {
                println!("No results found.");
            } else {
                println!("Found {} result(s):", nodes.len());
                for node in nodes.iter().take(20) {
                    let file = node.file_path.as_deref().unwrap_or("?");
                    println!("  - {} ({:?}) @ {}", node.name, node.node_type, file);
                }
                if nodes.len() > 20 {
                    println!("  ... and {} more", nodes.len() - 20);
                }

                // Show complexity if asking about complexity
                if question.to_lowercase().contains("complexity") && nodes.len() == 1 {
                    let c = nodes[0]
                        .get_property("cyclomatic")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1);
                    println!("\n{}", format_complexity_level(&nodes[0].name, c));
                }
            }
        }
        QueryResult::Text(lines) => {
            for line in lines {
                println!("{line}");
            }
        }
    }

    // Community follow-up hint
    if question.to_lowercase().contains("community") || question.to_lowercase().contains("module") {
        if let Ok(report) = matcher.analyze_communities(backend) {
            if !report.is_empty() {
                println!("\n{report}");
            }
        }
    }
}

fn print_help() {
    println!("Commands:");
    println!("  exit, quit  — leave chat");
    println!("  history     — show recent queries");
    println!("  help        — show this help");
    println!("\nExample questions:");
    println!("  How many functions?");
    println!("  Who calls verify_token?");
    println!("  What's its complexity?  (after discussing a symbol)");
}
