//! Training examples for the dual-agent translation layer.
//!
//! Each entry maps natural language to a graph query pattern or PatternMatcher operation string.

/// A single (natural language, query pattern) training pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryExample {
    /// Natural language question or phrase
    pub nl: &'static str,
    /// Graph query pattern or `operation=...|...` string for PatternMatcher
    pub pattern: &'static str,
}

/// Bootstrap examples covering impact, callers, complexity, signatures, type filters,
/// and multi-clause compound queries.
pub fn default_examples() -> &'static [QueryExample] {
    &EXAMPLES
}

/// Number of built-in examples.
pub fn example_count() -> usize {
    EXAMPLES.len()
}

const EXAMPLES: [QueryExample; 29] = [
    // Type filters
    QueryExample {
        nl: "list all functions",
        pattern: "type:Function",
    },
    QueryExample {
        nl: "show me all classes",
        pattern: "type:Class",
    },
    QueryExample {
        nl: "how many structs are there",
        pattern: "type:Struct",
    },
    QueryExample {
        nl: "find all config keys",
        pattern: "type:ConfigKey",
    },
    QueryExample {
        nl: "list files in the project",
        pattern: "type:File",
    },
    // Signature filters
    QueryExample {
        nl: "async functions",
        pattern: "type:Function|signature:*async*",
    },
    QueryExample {
        nl: "functions with async signature",
        pattern: "type:Function|signature:*async*",
    },
    QueryExample {
        nl: "functions returning Result",
        pattern: "type:Function|return_type:Result",
    },
    QueryExample {
        nl: "find functions that return Option",
        pattern: "type:Function|return_type:Option",
    },
    QueryExample {
        nl: "pub fn handlers",
        pattern: "type:Function|signature:*pub fn*",
    },
    // Compound / multi-hop filters
    QueryExample {
        nl: "async functions returning Result",
        pattern: "type:Function|signature:*async*|return_type:Result",
    },
    QueryExample {
        nl: "public api endpoints",
        pattern: "type:Function|label:api|signature:*pub*",
    },
    QueryExample {
        nl: "service classes with Service suffix",
        pattern: "type:Class|name_suffix:Service",
    },
    QueryExample {
        nl: "repository functions in backend repo",
        pattern: "type:Function|repo:backend",
    },
    // Callers
    QueryExample {
        nl: "what calls authenticate",
        pattern: "operation=callers|symbol=authenticate",
    },
    QueryExample {
        nl: "who calls verify_token",
        pattern: "operation=callers|symbol=verify_token",
    },
    QueryExample {
        nl: "callers of process_payment",
        pattern: "operation=callers|symbol=process_payment",
    },
    QueryExample {
        nl: "functions that call main",
        pattern: "operation=callers|symbol=main",
    },
    // Impact / blast radius
    QueryExample {
        nl: "what breaks if I change verify_token",
        pattern: "operation=impact|symbol=verify_token",
    },
    QueryExample {
        nl: "impact of changing authenticate",
        pattern: "operation=impact|symbol=authenticate",
    },
    QueryExample {
        nl: "blast radius of process_order",
        pattern: "operation=impact|symbol=process_order",
    },
    QueryExample {
        nl: "what would break if handle_request changes",
        pattern: "operation=impact|symbol=handle_request",
    },
    // Complexity
    QueryExample {
        nl: "high complexity functions",
        pattern: "operation=high_complexity|threshold=10|query=type:Function",
    },
    QueryExample {
        nl: "functions with complexity above 15",
        pattern: "operation=complexity_filter|threshold=15|query=type:Function",
    },
    QueryExample {
        nl: "most complex functions",
        pattern: "operation=most_complex|query=type:Function",
    },
    QueryExample {
        nl: "complexity of verify_token",
        pattern: "operation=complexity_symbol|symbol=verify_token",
    },
    QueryExample {
        nl: "top 10 complex functions",
        pattern: "operation=top_n|limit=10|query=type:Function",
    },
    // Dependencies & misc analysis
    QueryExample {
        nl: "dependencies of fetch_user",
        pattern: "operation=dependencies|symbol=fetch_user",
    },
    QueryExample {
        nl: "circular dependencies",
        pattern: "operation=circular_deps|query=type:Function",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_at_least_twenty_examples() {
        assert!(example_count() >= 20);
    }
}
